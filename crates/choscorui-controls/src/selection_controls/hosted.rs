//! The single-node forms of `RadioGroup` and `Tabs` for a control host.
//! `RadioGroup` and `Tabs` name each option by the node that shows it, so
//! they cannot mount as one node. These forms name each option by its
//! index, hit-test their parts from the geometry of the last arrange, and
//! report `Changed(Index(Some(i)))`.

use super::{
    RadioGroup, SelectionError, Tabs, navigation_target, paint_radio, paint_tab, tabs_query,
};
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::style::{
    Content, ControlKind, Orientation, PaintContext, Slot, StyleQuery, TabsVariant, VisualState,
    choice_size, content_rect,
};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size,
};

#[derive(Clone, Debug, PartialEq)]
struct Choice {
    label: String,
    enabled: bool,
}

/// The selection, input, and assistive behavior that both forms share.
#[derive(Clone, Debug)]
struct Choices {
    label: String,
    items: Vec<Choice>,
    selected: usize,
    enabled: bool,
    hovered: Option<usize>,
    pressed: Option<usize>,
    /// The part rectangles from the last arrange, in item order.
    rects: Vec<Rect>,
}

impl Choices {
    fn new<S: Into<String>>(
        label: impl Into<String>,
        labels: impl IntoIterator<Item = S>,
        selected: usize,
    ) -> Result<Self, SelectionError> {
        let items: Vec<Choice> = labels
            .into_iter()
            .map(|label| Choice {
                label: label.into(),
                enabled: true,
            })
            .collect();
        if items.is_empty() {
            return Err(SelectionError::Empty);
        }
        if selected >= items.len() {
            return Err(SelectionError::UnknownSelection);
        }
        Ok(Self {
            label: label.into(),
            items,
            selected,
            enabled: true,
            hovered: None,
            pressed: None,
            rects: Vec::new(),
        })
    }

    fn disable(&mut self, index: usize) -> Result<(), SelectionError> {
        let item = self
            .items
            .get_mut(index)
            .ok_or(SelectionError::UnknownSelection)?;
        if index == self.selected {
            return Err(SelectionError::DisabledSelection);
        }
        item.enabled = false;
        Ok(())
    }

    fn is_enabled(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|item| item.enabled)
    }

    /// Selects `index` and reports it. A disabled or current option
    /// reports nothing.
    fn select(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        if index == self.selected || !self.is_enabled(index) {
            return;
        }
        self.selected = index;
        cx.emit(ControlMessage::Changed(ControlValue::Index(Some(index))));
        cx.request_paint();
        cx.request_semantics();
    }

    fn cancel_press(&mut self, cx: &mut ControlContext<'_>) {
        if self.pressed.take().is_some() {
            cx.capture_pointer(false);
            cx.request_paint();
        }
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
                    hosted::hit(&self.rects, *position).filter(|index| self.is_enabled(*index));
                if self.pressed.is_none() {
                    return EventReply::Continue;
                }
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) => {
                let Some(pressed) = self.pressed.take() else {
                    return EventReply::Continue;
                };
                cx.capture_pointer(false);
                if hosted::hit(&self.rects, *position) == Some(pressed) {
                    self.select(cx, pressed);
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
            ControlEvent::FocusChanged(_) | ControlEvent::Dismiss => {
                self.cancel_press(cx);
                cx.request_paint();
                EventReply::Continue
            }
            // Selection follows focus, so Space and Enter select the
            // current option again, which reports nothing.
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                ..
            }) => EventReply::Stop,
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                match navigation_target(&self.items, self.selected, *key, |item| item.enabled) {
                    Some(target) => {
                        self.select(cx, target);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            _ => EventReply::Continue,
        }
    }

    /// The option that an assistive `SetValue` names: its label, or its
    /// index.
    fn named(&self, value: &str) -> Option<usize> {
        self.items
            .iter()
            .position(|item| item.label == value)
            .or_else(|| value.trim().parse().ok())
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let key = match action {
            SemanticAction::SetValue => {
                let Some(index) = value
                    .and_then(|value| self.named(value))
                    .filter(|index| self.is_enabled(*index))
                else {
                    return false;
                };
                self.select(cx, index);
                return true;
            }
            SemanticAction::Increment => Key::ArrowDown,
            SemanticAction::Decrement => Key::ArrowUp,
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        if let Some(target) =
            navigation_target(&self.items, self.selected, key, |item| item.enabled)
        {
            self.select(cx, target);
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.hovered = None;
            self.pressed = None;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Index(Some(index)) if self.is_enabled(*index) => {
                self.selected = *index;
                true
            }
            _ => false,
        }
    }

    fn semantics(&self, role: SemanticRole) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::SetValue,
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(role, &self.label, actions);
        node.value = Some(self.items[self.selected].label.clone());
        node.state.disabled = !self.enabled;
        node
    }

    /// The recipe flags of the part at `index`. `current` marks the
    /// selected part, and only it shows the host's focus.
    fn part_state(&self, host: VisualState, index: usize, current: VisualState) -> VisualState {
        let selected = index == self.selected;
        VisualState::NONE
            .set(
                VisualState::DISABLED,
                host.disabled() || !self.enabled || !self.items[index].enabled,
            )
            .set(current, selected)
            .set(VisualState::FOCUSED, selected && host.focused())
            .set(VisualState::FOCUS_VISIBLE, selected && host.focus_visible())
            .set(VisualState::HOVERED, self.hovered == Some(index))
            .set(VisualState::PRESSED, self.pressed == Some(index))
    }
}

/// A radio group mounted as one node: the options stack from the top, and
/// arrow keys move the selection. Build it with [`RadioGroup::hosted`].
#[derive(Clone, Debug)]
pub struct HostedRadioGroup {
    choices: Choices,
    invalid: bool,
}

impl RadioGroup {
    /// The single-node form for a control host, with one option for each
    /// label and the option at `selected` selected. Fails for an empty
    /// list or a selection out of range.
    pub fn hosted<S: Into<String>>(
        label: impl Into<String>,
        options: impl IntoIterator<Item = S>,
        selected: usize,
    ) -> Result<HostedRadioGroup, SelectionError> {
        Ok(HostedRadioGroup {
            choices: Choices::new(label, options, selected)?,
            invalid: false,
        })
    }
}

impl HostedRadioGroup {
    /// The group with the invalid state from the start. Each option
    /// shows the invalid look.
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    /// The recipe flags of the option at `index`: the host's flags on
    /// the selected option, with checked and invalid.
    pub fn option_state(&self, host: VisualState, index: usize) -> VisualState {
        self.choices
            .part_state(host, index, VisualState::CHECKED)
            .set(VisualState::INVALID, host.invalid() || self.invalid)
    }

    /// Disables the option at `index`. Fails for an index out of range or
    /// the selected option.
    pub fn with_disabled(mut self, index: usize) -> Result<Self, SelectionError> {
        self.choices.disable(index)?;
        Ok(self)
    }

    /// Disables the option at `index` in place. Fails for an index out of
    /// range or the selected option, and then changes nothing.
    pub fn disable(&mut self, index: usize) -> Result<(), SelectionError> {
        self.choices.disable(index)
    }

    fn sizes(&self, cx: &MeasureContext<'_>) -> Vec<Size> {
        self.choices
            .items
            .iter()
            .map(|item| {
                choice_size(
                    ControlKind::RadioGroup,
                    &item.label,
                    cx.styles,
                    cx.tokens,
                    cx.text,
                )
            })
            .collect()
    }

    fn gap(cx: &MeasureContext<'_>) -> f64 {
        cx.style(StyleQuery::new(ControlKind::RadioGroup, Slot::Group))
            .gap
    }
}

impl CustomControl for HostedRadioGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let sizes = self.sizes(cx);
        let gaps = Self::gap(cx) * sizes.len().saturating_sub(1) as f64;
        let size = Size::new(
            sizes.iter().map(|size| size.width).fold(0.0, f64::max),
            sizes.iter().map(|size| size.height).sum::<f64>() + gaps,
        );
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let gap = Self::gap(cx);
        let mut y = bounds.origin.y;
        self.choices.rects = self
            .sizes(cx)
            .iter()
            .map(|size| {
                let rect = Rect::from_xywh(bounds.origin.x, y, bounds.size.width, size.height);
                y += size.height + gap;
                rect
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.choices.event(cx, event)
    }

    /// Paints each option in its row. The circle sits at the start in
    /// reading order.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        for (index, rect) in self.choices.rects.iter().enumerate() {
            let state = self.option_state(host, index);
            paint_radio(&mut cx.at(*rect), &self.choices.items[index].label, state);
        }
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.choices.semantics(SemanticRole::RadioGroup);
        node.state.invalid = self.invalid;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.choices.perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.choices.set_enabled(enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.choices.set_value(value)
    }
}

/// A tab list mounted as one node: the tabs sit in the list surface in
/// reading order, and arrow keys move the selection. The application
/// shows the panel of the selected tab. Build it with [`Tabs::hosted`].
#[derive(Clone, Debug)]
pub struct HostedTabs {
    choices: Choices,
    variant: TabsVariant,
    orientation: Orientation,
}

impl Tabs {
    /// The single-node form for a control host, with one tab for each
    /// label and the tab at `selected` selected. Fails for an empty list
    /// or a selection out of range.
    pub fn hosted<S: Into<String>>(
        label: impl Into<String>,
        tabs: impl IntoIterator<Item = S>,
        selected: usize,
    ) -> Result<HostedTabs, SelectionError> {
        Ok(HostedTabs {
            choices: Choices::new(label, tabs, selected)?,
            variant: TabsVariant::Default,
            orientation: Orientation::Horizontal,
        })
    }
}

impl HostedTabs {
    #[must_use]
    pub fn with_variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Disables the tab at `index`. Fails for an index out of range or the
    /// selected tab.
    pub fn with_disabled(mut self, index: usize) -> Result<Self, SelectionError> {
        self.choices.disable(index)?;
        Ok(self)
    }

    /// Disables the tab at `index` in place. Fails for an index out of
    /// range or the selected tab, and then changes nothing.
    pub fn disable(&mut self, index: usize) -> Result<(), SelectionError> {
        self.choices.disable(index)
    }

    fn sizes(&self, cx: &MeasureContext<'_>) -> Vec<Size> {
        let style = cx.style(tabs_query(self.variant, Slot::Trigger, VisualState::NONE));
        self.choices
            .items
            .iter()
            .map(|item| Content::label(&item.label).size(&style, cx.text))
            .collect()
    }
}

impl CustomControl for HostedTabs {
    /// The tabs along the orientation, inside the list padding. A
    /// horizontal list is at least the list height.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let list = cx.style(tabs_query(self.variant, Slot::Group, VisualState::NONE));
        let sizes = self.sizes(cx);
        let gaps = list.gap * sizes.len().saturating_sub(1) as f64;
        let widest = sizes.iter().map(|size| size.width).fold(0.0, f64::max);
        let tallest = sizes.iter().map(|size| size.height).fold(0.0, f64::max);
        let content = match self.orientation {
            Orientation::Horizontal => Size::new(
                sizes.iter().map(|size| size.width).sum::<f64>() + gaps,
                tallest,
            ),
            Orientation::Vertical => Size::new(
                widest,
                sizes.iter().map(|size| size.height).sum::<f64>() + gaps,
            ),
        };
        // Only a horizontal list has the fixed list height; a vertical
        // list fits its tabs.
        let min_height = match self.orientation {
            Orientation::Horizontal => list.min_height,
            Orientation::Vertical => 0.0,
        };
        let size = Size::new(
            content.width + list.padding.horizontal(),
            (content.height + list.padding.vertical()).max(min_height),
        );
        Measured::new(cx.constraints.constrain(size))
    }

    /// Places the tabs from the start in reading order, or from the top
    /// when vertical.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let rtl = cx.rtl();
        let list = cx.style(tabs_query(self.variant, Slot::Group, VisualState::NONE));
        let inner = content_rect(bounds, &list, rtl);
        let mut offset = 0.0;
        self.choices.rects = self
            .sizes(cx)
            .iter()
            .map(|size| match self.orientation {
                Orientation::Horizontal => {
                    let x = if rtl {
                        inner.max_x() - offset - size.width
                    } else {
                        inner.origin.x + offset
                    };
                    offset += size.width + list.gap;
                    Rect::from_xywh(x, inner.origin.y, size.width, inner.size.height)
                }
                Orientation::Vertical => {
                    let rect = Rect::from_xywh(
                        inner.origin.x,
                        inner.origin.y + offset,
                        inner.size.width,
                        size.height,
                    );
                    offset += size.height + list.gap;
                    rect
                }
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.choices.event(cx, event)
    }

    /// Paints the list surface, then each tab in its rectangle.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let list = cx.style(tabs_query(self.variant, Slot::Group, VisualState::NONE));
        let bounds = cx.bounds;
        cx.slot(bounds, &list, |_, _| {});
        for (index, rect) in self.choices.rects.iter().enumerate() {
            let state = self.choices.part_state(host, index, VisualState::SELECTED);
            let label = &self.choices.items[index].label;
            paint_tab(
                &mut cx.at(*rect),
                self.variant,
                self.orientation,
                label,
                state,
            );
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.choices.semantics(SemanticRole::TabList)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.choices.perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.choices.set_enabled(enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.choices.set_value(value)
    }
}
