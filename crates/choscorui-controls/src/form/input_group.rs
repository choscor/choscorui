//! A text input with addons: text, icons, and buttons around the text.

use super::input::{Editor, default_width};
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::style::{
    AddonAlign, Content, ControlKind, ControlSize, PaintContext, ResolvedStyle, Slot, StyleQuery,
    TextAlign, VisualState, content_rect,
};
use crate::text_field::{TextField, ValidationState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Measured, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, Size,
};

/// What an addon shows.
#[derive(Clone, Debug, PartialEq)]
pub enum AddonContent {
    Text(String),
    Icon(Icon),
    /// A ghost button. A click emits `ItemActivated` with the addon index.
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
    editor: Editor,
    addons: Vec<InputGroupAddon>,
    /// Each addon's slot rectangle, from the last arrange.
    rects: Vec<Rect>,
    text: Rect,
    pressed: Option<usize>,
    hovered: Option<usize>,
}

impl InputGroup {
    pub fn new(field: TextField, addons: Vec<InputGroupAddon>) -> Self {
        Self {
            editor: Editor::new(field),
            addons,
            rects: Vec::new(),
            text: Rect::default(),
            pressed: None,
            hovered: None,
        }
    }

    pub fn value(&self) -> &str {
        self.editor.field.value()
    }

    pub fn addons(&self) -> &[InputGroupAddon] {
        &self.addons
    }

    pub fn set_validation(&mut self, validation: ValidationState) {
        self.editor.field.set_validation_state(validation);
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

    fn hit(&self, point: choscorui_core::Point) -> Option<usize> {
        self.rects
            .iter()
            .position(|rect| rect.contains(point))
            .filter(|index| matches!(self.addons[*index].content, AddonContent::Button { .. }))
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
        self.rects = rects;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if self.hit(*position).is_some() => {
                self.pressed = self.hit(*position);
                cx.capture_pointer(true);
                cx.request_paint();
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                let pressed = self.pressed.take();
                cx.capture_pointer(false);
                if pressed.is_some() && self.hit(*position) == pressed {
                    cx.emit(ControlMessage::ItemActivated(pressed.unwrap_or_default()));
                }
                cx.request_paint();
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = self.hit(*position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
            }
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
            }
            _ => {}
        }
        let text = self.text;
        self.editor.event(cx, event, text)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.editor.state(cx.state);
        let root = cx.style(Self::query(Slot::Root).state(state));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        let disabled = VisualState::NONE.set(VisualState::DISABLED, state.disabled());
        for (index, (addon, rect)) in self.addons.iter().zip(&self.rects).enumerate() {
            let slot = cx.style(Self::addon_query(addon, disabled));
            let content = Self::content(addon);
            let inner = content_rect(*rect, &slot, cx.rtl);
            let align = if addon.is_inline() {
                TextAlign::Center
            } else {
                TextAlign::Start
            };
            match &addon.content {
                AddonContent::Button { size, .. } => {
                    let button_state = disabled
                        .set(VisualState::HOVERED, self.hovered == Some(index))
                        .set(VisualState::PRESSED, self.pressed == Some(index));
                    let style = cx.style(Self::button_query(*size, button_state));
                    let natural = content.size(&style, cx.text);
                    let x = match (addon.is_inline(), cx.rtl) {
                        (true, _) => inner.center().x - natural.width / 2.0,
                        (false, false) => inner.origin.x,
                        (false, true) => inner.max_x() - natural.width,
                    };
                    let button = Rect::from_xywh(
                        x,
                        inner.center().y - natural.height / 2.0,
                        natural.width,
                        natural.height,
                    );
                    cx.slot(button, &style, |cx, inner| {
                        content.draw(cx, inner, &style, TextAlign::Center);
                    });
                }
                _ => content.draw(cx, inner, &slot, align),
            }
        }
        self.editor.paint_text(cx, self.text, state);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.editor.semantics();
        // Addon text describes the value, for example a unit or a domain.
        node.description = self
            .addons
            .iter()
            .filter_map(|addon| match &addon.content {
                AddonContent::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::SetValue => self.editor.set_value(cx, value.unwrap_or_default()),
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.editor.field.set_enabled_state(enabled);
    }

    /// Takes `Text`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.editor.set_text(value)
    }
}
