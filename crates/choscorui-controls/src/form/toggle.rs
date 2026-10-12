//! Two-state toggle buttons, alone or in a group.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Press, PressOutcome};
use crate::style::{
    Content, ControlKind, ControlSize, Orientation, PaintContext, Slot, StyleQuery, TextAlign,
    ToggleVariant, VisualState, join_segment,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Measured, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, Size,
};

/// A button that stays on or off. A click, Space, or Enter flips it.
#[derive(Clone, Debug)]
pub struct Toggle {
    label: String,
    icon: Option<Icon>,
    on: bool,
    variant: ToggleVariant,
    size: ControlSize,
    invalid: bool,
    enabled: bool,
    press: Press,
}

impl Toggle {
    pub fn new(label: impl Into<String>, on: bool) -> Self {
        Self {
            label: label.into(),
            icon: None,
            on,
            variant: ToggleVariant::Default,
            size: ControlSize::Default,
            invalid: false,
            enabled: true,
            press: Press::default(),
        }
    }

    /// Shows `icon` before the label.
    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn with_variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }

    /// `Sm`, `Default`, or `Lg`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// The toggle with the invalid state from the start.
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    /// The flags the recipe reads: `host` with on (checked), pressed,
    /// and invalid.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::CHECKED, self.on)
            .set(VisualState::PRESSED, self.press.pressed)
            .set(VisualState::INVALID, self.invalid)
    }

    fn query(&self, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Toggle, Slot::Root)
            .variant(self.variant)
            .size(self.size)
            .state(state)
    }

    fn content(&self) -> Content<'_> {
        Content {
            leading: self.icon,
            label: &self.label,
            trailing: None,
        }
    }

    fn flip(&mut self, cx: &mut ControlContext<'_>) {
        self.on = !self.on;
        cx.emit(ControlMessage::Changed(ControlValue::Bool(self.on)));
        cx.request_paint();
        cx.request_semantics();
    }
}

impl CustomControl for Toggle {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(self.query(VisualState::NONE));
        Measured {
            size: self.content().size(&style, cx.text),
            baseline: None,
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let outcome = self.press.event(cx, event);
        if let PressOutcome::Activated(_) = outcome {
            self.flip(cx);
        }
        hosted::reply(outcome)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(self.query(self.visual_state(cx.state)));
        let bounds = cx.bounds;
        let content = self.content();
        cx.slot(bounds, &style, |cx, inner| {
            content.draw(cx, inner, &style, TextAlign::Center);
        });
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Button, &self.label, actions);
        node.description = "toggle".into();
        node.state.checked = Some(self.on);
        node.state.invalid = self.invalid;
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate if self.enabled => {
                self.flip(cx);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Takes `Bool`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Bool(on) = value else {
            return false;
        };
        self.on = *on;
        true
    }
}

/// One item of a [`ToggleGroup`].
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleItem {
    pub label: String,
    pub icon: Option<Icon>,
    pub enabled: bool,
}

impl ToggleItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// A row or column of toggles. With `multiple`, each item flips on its
/// own; otherwise at most one item is on. The group is one Tab stop:
/// arrow keys move focus between enabled items, and Space or Enter flips
/// the focused item.
#[derive(Clone, Debug)]
pub struct ToggleGroup {
    name: String,
    items: Vec<ToggleItem>,
    on: Vec<bool>,
    multiple: bool,
    variant: ToggleVariant,
    size: ControlSize,
    spacing: f64,
    orientation: Orientation,
    focused: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    rects: Vec<Rect>,
    rtl: bool,
    invalid: bool,
    enabled: bool,
}

impl ToggleGroup {
    pub fn new(name: impl Into<String>, items: Vec<ToggleItem>, multiple: bool) -> Self {
        let count = items.len();
        Self {
            name: name.into(),
            items,
            on: vec![false; count],
            multiple,
            variant: ToggleVariant::Default,
            size: ControlSize::Default,
            spacing: 0.0,
            orientation: Orientation::Horizontal,
            focused: 0,
            hovered: None,
            pressed: None,
            rects: Vec::new(),
            rtl: false,
            invalid: false,
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// The gap between items. Zero joins them into one bar.
    #[must_use]
    pub fn with_spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// The group with the invalid state from the start. Each item shows
    /// the invalid look.
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    /// The indices of the items that are on, in order.
    pub fn selected(&self) -> Vec<usize> {
        (0..self.on.len()).filter(|index| self.on[*index]).collect()
    }

    /// Turns on the items at `indices`. A single-choice group keeps only
    /// the first valid index.
    pub fn set_selected(&mut self, indices: &[usize]) {
        self.on.fill(false);
        let count = self.on.len();
        for index in indices.iter().copied().filter(|index| *index < count) {
            self.on[index] = true;
            if !self.multiple {
                break;
            }
        }
    }

    /// The item that has the roving focus.
    pub fn focused(&self) -> usize {
        self.focused
    }

    /// The item rectangles from the last arrange, in item order.
    pub fn item_rects(&self) -> &[Rect] {
        &self.rects
    }

    fn query(&self, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::ToggleGroup, Slot::Item)
            .variant(self.variant)
            .size(self.size)
            .state(state)
    }

    fn content(item: &ToggleItem) -> Content<'_> {
        Content {
            leading: item.icon,
            label: &item.label,
            trailing: None,
        }
    }

    /// Flips `index` and reports the new selection.
    fn flip(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        if !self.items[index].enabled {
            return;
        }
        let on = !self.on[index];
        if !self.multiple {
            self.on.fill(false);
        }
        self.on[index] = on;
        self.focused = index;
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            self.selected(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    /// The item a roving key moves focus to, skipping disabled items.
    fn rove(&self, key: choscorui_core::Key) -> Option<usize> {
        let enabled: Vec<usize> = (0..self.items.len())
            .filter(|index| self.items[*index].enabled)
            .collect();
        let current = enabled
            .iter()
            .position(|index| *index == self.focused)
            .unwrap_or(0);
        let horizontal = self.orientation == Orientation::Horizontal;
        hosted::rove(key, current, enabled.len(), horizontal, self.rtl)
            .map(|position| enabled[position])
    }
}

impl CustomControl for ToggleGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(self.query(VisualState::NONE));
        let sizes: Vec<Size> = self
            .items
            .iter()
            .map(|item| Self::content(item).size(&style, cx.text))
            .collect();
        let gaps = self.spacing * sizes.len().saturating_sub(1) as f64;
        let size = match self.orientation {
            Orientation::Horizontal => Size::new(
                sizes.iter().map(|size| size.width).sum::<f64>() + gaps,
                sizes.iter().map(|size| size.height).fold(0.0, f64::max),
            ),
            Orientation::Vertical => Size::new(
                sizes.iter().map(|size| size.width).fold(0.0, f64::max),
                sizes.iter().map(|size| size.height).sum::<f64>() + gaps,
            ),
        };
        Measured {
            size,
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        let style = cx.style(self.query(VisualState::NONE));
        let mut offset = 0.0;
        self.rects = self
            .items
            .iter()
            .map(|item| {
                let size = Self::content(item).size(&style, cx.text);
                match self.orientation {
                    Orientation::Horizontal => {
                        let x = if self.rtl {
                            bounds.max_x() - offset - size.width
                        } else {
                            bounds.origin.x + offset
                        };
                        offset += size.width + self.spacing;
                        Rect::from_xywh(x, bounds.origin.y, size.width, bounds.size.height)
                    }
                    Orientation::Vertical => {
                        let rect = Rect::from_xywh(
                            bounds.origin.x,
                            bounds.origin.y + offset,
                            bounds.size.width,
                            size.height,
                        );
                        offset += size.height + self.spacing;
                        rect
                    }
                }
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                self.pressed =
                    hosted::hit(&self.rects, *position).filter(|index| self.items[*index].enabled);
                if self.pressed.is_some() {
                    cx.capture_pointer(true);
                    cx.request_paint();
                }
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) => {
                let Some(pressed) = self.pressed.take() else {
                    return EventReply::Continue;
                };
                cx.capture_pointer(false);
                if hosted::hit(&self.rects, *position) == Some(pressed) {
                    self.flip(cx, pressed);
                }
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = hosted::hit(&self.rects, *position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => {
                let Some(key) = hosted::key_down(event) else {
                    return EventReply::Continue;
                };
                if matches!(key, choscorui_core::Key::Space | choscorui_core::Key::Enter) {
                    let focused = self.focused;
                    self.flip(cx, focused);
                    return EventReply::Stop;
                }
                match self.rove(key) {
                    Some(target) => {
                        self.focused = target;
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let joined = self.spacing == 0.0;
        for (index, (item, rect)) in self.items.iter().zip(&self.rects).enumerate() {
            let state = VisualState::NONE
                .set(VisualState::CHECKED, self.on[index])
                .set(
                    VisualState::HOVERED,
                    host.hovered() && self.hovered == Some(index),
                )
                .set(VisualState::PRESSED, self.pressed == Some(index))
                .set(VisualState::INVALID, self.invalid)
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && self.focused == index,
                )
                .set(VisualState::DISABLED, host.disabled() || !item.enabled);
            let mut style = cx.style(self.query(state));
            let mut rect = *rect;
            if joined {
                join_segment(
                    &mut style,
                    &mut rect,
                    index,
                    self.items.len(),
                    self.orientation,
                    cx.rtl,
                );
            }
            let content = Self::content(item);
            cx.slot(rect, &style, |cx, inner| {
                content.draw(cx, inner, &style, TextAlign::Center);
            });
        }
    }

    fn semantics(&self) -> SemanticNode {
        let role = if self.multiple {
            SemanticRole::Group
        } else {
            SemanticRole::RadioGroup
        };
        let actions = if self.enabled {
            vec![SemanticAction::SetValue, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(role, &self.name, actions);
        let on: Vec<&str> = self
            .selected()
            .into_iter()
            .map(|index| self.items[index].label.as_str())
            .collect();
        node.value = Some(on.join(", "));
        if let Some(item) = self.items.get(self.focused) {
            node.description = item.label.clone();
        }
        node.state.invalid = self.invalid;
        node.state.disabled = !self.enabled;
        node
    }

    /// `SetValue` with an item label flips that item.
    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::SetValue if self.enabled => {
                let Some(index) = self
                    .items
                    .iter()
                    .position(|item| Some(item.label.as_str()) == value)
                else {
                    return false;
                };
                self.flip(cx, index);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Takes `Indices`. Rejects an index past the items, and more than
    /// one index in a single-choice group.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(indices) = value else {
            return false;
        };
        let fits = indices.iter().all(|index| *index < self.items.len())
            && (self.multiple || indices.len() <= 1);
        if fits {
            self.set_selected(indices);
        }
        fits
    }
}
