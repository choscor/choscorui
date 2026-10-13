//! A text input with addons: text, icons, and buttons around the text.

use super::input::default_width;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Parts};
use crate::style::{
    AddonAlign, Content, ControlKind, ControlSize, PaintContext, ResolvedStyle, Slot, StyleQuery,
    TextAlign, VisualState, content_rect,
};
use crate::text_field::{TextField, ValidationState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, TextInputInfo,
};

/// What an addon shows.
#[derive(Clone, Debug, PartialEq)]
pub enum AddonContent {
    Text(String),
    Icon(Icon),
    /// A ghost button. A click, or Space or Return while the button has
    /// focus, emits `ItemActivated` with the addon index. Tab moves the
    /// focus from the text to each button addon in order.
    Button {
        label: String,
        icon: Option<Icon>,
        size: ControlSize,
    },
}

/// One addon of an [`InputGroup`] and the edge it sits on.
#[derive(Clone, Debug, PartialEq)]
pub struct InputGroupAddon {
    pub align: AddonAlign,
    pub content: AddonContent,
}

impl InputGroupAddon {
    pub fn text(align: AddonAlign, text: impl Into<String>) -> Self {
        Self {
            align,
            content: AddonContent::Text(text.into()),
        }
    }

    pub fn icon(align: AddonAlign, icon: Icon) -> Self {
        Self {
            align,
            content: AddonContent::Icon(icon),
        }
    }

    /// A button with `label`, an optional icon, and a size of `Xs`,
    /// `IconXs`, `Sm`, or `IconSm`. Icon sizes show only the icon.
    pub fn button(
        align: AddonAlign,
        label: impl Into<String>,
        icon: Option<Icon>,
        size: ControlSize,
    ) -> Self {
        Self {
            align,
            content: AddonContent::Button {
                label: label.into(),
                icon,
                size,
            },
        }
    }

    fn is_inline(&self) -> bool {
        matches!(self.align, AddonAlign::InlineStart | AddonAlign::InlineEnd)
    }
}

/// A text input in one box with addons at its edges. Inline addons sit
/// before or after the text; block addons sit above or below it.
pub struct InputGroup {
    editor: TextField,
    addons: Vec<InputGroupAddon>,
    /// Each addon's slot rectangle, from the last arrange.
    rects: Vec<Rect>,
    /// Each button addon's button rectangle, from the last arrange. Other
    /// addons have an empty rectangle.
    buttons: Vec<Rect>,
    text: Rect,
    parts: Parts,
    /// The button addon that has the focus, or `None` for the text.
    focus: Option<usize>,
}

impl InputGroup {
    pub fn new(field: TextField, addons: Vec<InputGroupAddon>) -> Self {
        Self {
            editor: field,
            addons,
            rects: Vec::new(),
            buttons: Vec::new(),
            text: Rect::default(),
            parts: Parts::default(),
            focus: None,
        }
    }

    pub fn value(&self) -> &str {
        self.editor.value()
    }

    /// Text shown in muted color while the field is empty.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.editor = self.editor.with_placeholder(placeholder);
        self
    }

    pub fn addons(&self) -> &[InputGroupAddon] {
        &self.addons
    }

    pub fn set_validation(&mut self, validation: ValidationState) {
        self.editor.set_validation_state(validation);
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::InputGroup, slot)
    }

    fn addon_query(addon: &InputGroupAddon, state: VisualState) -> StyleQuery {
        Self::query(Slot::Addon).variant(addon.align).state(state)
    }

    fn button_query(size: ControlSize, state: VisualState) -> StyleQuery {
        Self::query(Slot::Action).size(size).state(state)
    }

    fn content(addon: &InputGroupAddon) -> Content<'_> {
        match &addon.content {
            AddonContent::Text(text) => Content::label(text),
            AddonContent::Icon(icon) => Content {
                leading: Some(*icon),
                ..Content::default()
            },
            AddonContent::Button { label, icon, size } => Content {
                leading: *icon,
                label: if icon.is_some()
                    && matches!(size, ControlSize::IconXs | ControlSize::IconSm)
                {
                    ""
                } else {
                    label
                },
                trailing: None,
            },
        }
    }

    /// The size of an addon's slot: its padding around its content.
    fn addon_size(
        addon: &InputGroupAddon,
        style: &dyn Fn(StyleQuery) -> ResolvedStyle,
        measure: &dyn crate::style::TextMeasure,
    ) -> Size {
        let slot = style(Self::addon_query(addon, VisualState::NONE));
        let inner = match &addon.content {
            AddonContent::Button { size, .. } => Self::content(addon).size(
                &style(Self::button_query(*size, VisualState::NONE)),
                measure,
            ),
            _ => {
                let content = Self::content(addon);
                Size::new(
                    content.width(&slot, measure),
                    slot.text.line_height.max(slot.icon_size),
                )
            }
        };
        Size::new(
            inner.width + slot.padding.horizontal(),
            inner.height + slot.padding.vertical(),
        )
    }

    /// The button addon under `point`: the button itself, not its slot.
    fn hit(&self, point: Point) -> Option<usize> {
        hosted::hit(&self.buttons, point)
    }

    /// The tab stops: the text (`None`), then each button addon in order.
    fn stops(&self) -> Vec<Option<usize>> {
        let buttons = self
            .addons
            .iter()
            .enumerate()
            .filter(|(_, addon)| matches!(addon.content, AddonContent::Button { .. }))
            .map(|(index, _)| Some(index));
        std::iter::once(None).chain(buttons).collect()
    }

    /// The rectangle of a button of `size` in the slot content `inner` of
    /// `addon`.
    fn button_rect(addon: &InputGroupAddon, size: Size, inner: Rect, rtl: bool) -> Rect {
        let x = match (addon.is_inline(), rtl) {
            (true, _) => inner.center().x - size.width / 2.0,
            (false, false) => inner.origin.x,
            (false, true) => inner.max_x() - size.width,
        };
        Rect::from_xywh(
            x,
            inner.center().y - size.height / 2.0,
            size.width,
            size.height,
        )
    }

    /// The label of button addon `index`.
    fn label(&self, index: usize) -> &str {
        match &self.addons[index].content {
            AddonContent::Button { label, .. } => label,
            _ => "",
        }
    }

    /// Handles the keys and focus changes that move the focus among the
    /// text and the button addons, and the keys of a focused button.
    /// Returns `None` for an event that the text takes.
    fn focus_event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
    ) -> Option<EventReply> {
        match event {
            ControlEvent::FocusChanged(true) if self.parts.pressed.is_none() => {
                // Shift+Tab into the group lands on its last stop.
                let last = self.stops().last().copied().flatten();
                self.move_focus(cx, last.filter(|_| cx.modifiers().shift));
                None
            }
            ControlEvent::FocusChanged(false) => {
                self.move_focus(cx, None);
                None
            }
            ControlEvent::Key(KeyEvent::Down { key: Key::Tab, .. }) => {
                let stops = self.stops();
                let current = stops.iter().position(|stop| *stop == self.focus)?;
                let next = hosted::tab_stop(current, stops.len(), cx.modifiers().shift)?;
                self.move_focus(cx, stops[next]);
                Some(EventReply::Stop)
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => {
                let index = self.focus?;
                if matches!(key, Key::Space | Key::Enter) && !repeat {
                    cx.emit(ControlMessage::ItemActivated(index));
                    return Some(EventReply::Stop);
                }
                Some(EventReply::Continue)
            }
            ControlEvent::Text(_) | ControlEvent::Compose(_) | ControlEvent::Edit(_)
                if self.focus.is_some() =>
            {
                Some(EventReply::Continue)
            }
            _ => None,
        }
    }

    fn move_focus(&mut self, cx: &mut ControlContext<'_>, focus: Option<usize>) {
        if self.focus != focus {
            self.focus = focus;
            cx.request_paint();
            cx.request_semantics();
        }
    }
}

impl CustomControl for InputGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root));
        let input = cx.style(Self::query(Slot::Input));
        let style = |query| cx.style(query);
        // Inline addons center in the text row and do not grow it.
        let row = input.text.line_height + input.padding.vertical();
        let blocks: f64 = self
            .addons
            .iter()
            .filter(|addon| !addon.is_inline())
            .map(|addon| Self::addon_size(addon, &style, cx.text).height)
            .sum();
        let height = (row + blocks + root.border_width * 2.0).max(root.min_height);
        Measured {
            size: Size::new(default_width(cx, &root), height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root));
        let inner = content_rect(bounds, &root, cx.rtl());
        let inner = Rect::from_xywh(
            inner.origin.x + root.border_width,
            inner.origin.y + root.border_width,
            (inner.size.width - root.border_width * 2.0).max(0.0),
            (inner.size.height - root.border_width * 2.0).max(0.0),
        );
        let style = |query| cx.style(query);
        let sizes: Vec<Size> = self
            .addons
            .iter()
            .map(|addon| Self::addon_size(addon, &style, cx.text))
            .collect();
        let mut top = inner.origin.y;
        let mut bottom = inner.max_y();
        let mut rects = vec![Rect::default(); self.addons.len()];
        for (index, addon) in self.addons.iter().enumerate() {
            match addon.align {
                AddonAlign::BlockStart => {
                    rects[index] =
                        Rect::from_xywh(inner.origin.x, top, inner.size.width, sizes[index].height);
                    top += sizes[index].height;
                }
                AddonAlign::BlockEnd => {
                    bottom -= sizes[index].height;
                    rects[index] = Rect::from_xywh(
                        inner.origin.x,
                        bottom,
                        inner.size.width,
                        sizes[index].height,
                    );
                }
                _ => {}
            }
        }
        // Inline addons share the text row, from the edges inward. Start
        // addons stack forward from the leading edge and end addons stack
        // back from the trailing edge, so both keep reading order.
        let (mut start, mut end) = (0.0, 0.0);
        let row_height = (bottom - top).max(0.0);
        let rtl = cx.rtl();
        let order = self
            .addons
            .iter()
            .enumerate()
            .filter(|(_, addon)| addon.align == AddonAlign::InlineStart)
            .chain(
                self.addons
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|(_, addon)| addon.align == AddonAlign::InlineEnd),
            );
        for (index, addon) in order {
            let width = sizes[index].width;
            let leading = match addon.align {
                AddonAlign::InlineStart => true,
                AddonAlign::InlineEnd => false,
                _ => continue,
            };
            let offset = if leading { start } else { end };
            let x = if leading != rtl {
                inner.origin.x + offset
            } else {
                inner.max_x() - offset - width
            };
            rects[index] = Rect::from_xywh(x, top, width, row_height);
            if leading {
                start += width;
            } else {
                end += width;
            }
        }
        let input = cx.style(Self::query(Slot::Input));
        let left = if rtl { end } else { start };
        let row = Rect::from_xywh(
            inner.origin.x + left,
            top,
            (inner.size.width - start - end).max(0.0),
            row_height,
        );
        self.text = content_rect(row, &input, rtl);
        self.buttons = self
            .addons
            .iter()
            .zip(&rects)
            .map(|(addon, rect)| match &addon.content {
                AddonContent::Button { size, .. } => {
                    let slot = cx.style(Self::addon_query(addon, VisualState::NONE));
                    let style = cx.style(Self::button_query(*size, VisualState::NONE));
                    let natural = Self::content(addon).size(&style, cx.text);
                    Self::button_rect(addon, natural, content_rect(*rect, &slot, rtl), rtl)
                }
                _ => Rect::default(),
            })
            .collect();
        self.rects = rects;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| self.hit(point));
        self.parts = parts;
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            }) if self.parts.pressed.is_some() => return EventReply::Stop,
            ControlEvent::Pointer(PointerEvent::Up { .. }) if pressed.is_some() => {
                if let Some(index) = pressed.filter(|index| self.parts.hovered == Some(*index)) {
                    cx.emit(ControlMessage::ItemActivated(index));
                }
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Down { .. }) => self.move_focus(cx, None),
            _ => {}
        }
        if let Some(reply) = self.focus_event(cx, event) {
            return reply;
        }
        let text = self.text;
        self.editor.hosted_event(cx, event, text)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        // While a button addon has the focus, the text and the box show
        // no focus, and the button shows the focus ring.
        let host = match self.focus {
            Some(_) => cx
                .state
                .set(VisualState::FOCUSED, false)
                .set(VisualState::FOCUS_VISIBLE, false),
            None => cx.state,
        };
        let state = self.editor.hosted_state(host);
        let root = cx.style(Self::query(Slot::Root).state(state));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        let disabled = VisualState::NONE.set(VisualState::DISABLED, state.disabled());
        let addons = self.addons.iter().zip(&self.rects).zip(&self.buttons);
        for (index, ((addon, rect), button)) in addons.enumerate() {
            let content = Self::content(addon);
            if let AddonContent::Button { size, .. } = &addon.content {
                let focused = self.focus == Some(index) && cx.state.focused();
                let button_state = self
                    .parts
                    .state(index, disabled)
                    .set(VisualState::FOCUSED, focused)
                    .set(VisualState::FOCUS_VISIBLE, focused);
                let style = cx.style(Self::button_query(*size, button_state));
                cx.slot(*button, &style, |cx, inner| {
                    content.draw(cx, inner, &style, TextAlign::Center);
                });
                continue;
            }
            let slot = cx.style(Self::addon_query(addon, disabled));
            let align = if addon.is_inline() {
                TextAlign::Center
            } else {
                TextAlign::Start
            };
            let inner = content_rect(*rect, &slot, cx.rtl);
            content.draw(cx, inner, &slot, align);
        }
        self.editor.paint_hosted_text(cx, self.text, state);
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        match self.focus {
            Some(_) => None,
            None => self.editor.hosted_text_input(),
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(choscorui_core::Localizer::builtin())
    }

    fn semantics_in(&self, strings: &choscorui_core::Localizer) -> SemanticNode {
        // A focused button addon is the node, so assistive technology
        // reads it as a button.
        if let Some(index) = self.focus {
            let actions = vec![SemanticAction::Activate, SemanticAction::Focus];
            return hosted::node(SemanticRole::Button, self.label(index), actions);
        }
        let mut node = self.editor.hosted_semantics(strings);
        // Addon text describes the value, for example a unit or a domain.
        // It follows the validation message, which stays.
        let addons = self.addons.iter().filter_map(|addon| match &addon.content {
            AddonContent::Text(text) => Some(text.as_str()),
            _ => None,
        });
        let parts: Vec<&str> = std::iter::once(node.description.as_str())
            .filter(|text| !text.is_empty())
            .chain(addons)
            .collect();
        node.description = parts.join(" ");
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => match self.focus {
                Some(index) => {
                    cx.emit(ControlMessage::ItemActivated(index));
                    true
                }
                None => false,
            },
            SemanticAction::SetValue => self.editor.hosted_set_value(cx, value.unwrap_or_default()),
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.editor.set_enabled_state(enabled);
    }

    /// Takes `Text`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.editor.take_text(value)
    }
}
