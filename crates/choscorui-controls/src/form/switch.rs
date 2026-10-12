//! A two-state switch with an optional label.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Press, PressOutcome};
use crate::style::{
    ControlKind, ControlSize, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventReply, Measured, Point, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// A switch: a pill track with a sliding thumb, and a label after it. A
/// click anywhere on the control, Space, or Enter flips it.
#[derive(Clone, Debug)]
pub struct Switch {
    label: String,
    checked: bool,
    size: ControlSize,
    enabled: bool,
    press: Press,
}

impl Switch {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            size: ControlSize::Default,
            enabled: true,
            press: Press::default(),
        }
    }

    /// `Default` or `Sm`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    /// The flags the recipe reads: `host` with checked and pressed.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::CHECKED, self.checked)
            .set(VisualState::PRESSED, self.press.pressed)
    }

    fn query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Switch, slot)
            .size(self.size)
            .state(state)
    }

    fn flip(&mut self, cx: &mut ControlContext<'_>) {
        self.checked = !self.checked;
        cx.emit(ControlMessage::Changed(ControlValue::Bool(self.checked)));
        cx.request_paint();
        cx.request_semantics();
    }
}

impl CustomControl for Switch {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let track = cx.style(self.query(Slot::Root, VisualState::NONE));
        let mut size = Size::new(track.min_width, track.min_height);
        if !self.label.is_empty() {
            let label = cx.style(self.query(Slot::Label, VisualState::NONE));
            size.width += label.gap + cx.text.width(&self.label, &label.text);
            size.height = size.height.max(label.text.line_height);
        }
        Measured {
            size,
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
        let state = self.visual_state(cx.state);
        let (track, thumb, label) = (
            cx.style(self.query(Slot::Root, state)),
            cx.style(self.query(Slot::Thumb, state)),
            cx.style(self.query(Slot::Label, state)),
        );
        let bounds = cx.bounds;
        let y = bounds.origin.y + (bounds.size.height - track.min_height) / 2.0;
        let x = if cx.rtl {
            bounds.max_x() - track.min_width
        } else {
            bounds.origin.x
        };
        let rail = Rect::from_xywh(x, y, track.min_width, track.min_height);
        cx.slot(rail, &track, |_, _| {});
        // The thumb rests inside the track's border at the start, and at
        // the end while checked.
        let inset = (track.min_height - thumb.min_height) / 2.0;
        let at_end = self.checked != cx.rtl;
        let thumb_x = if at_end {
            rail.max_x() - inset - thumb.min_width
        } else {
            rail.origin.x + inset
        };
        let knob = Rect::new(
            Point::new(thumb_x, rail.origin.y + inset),
            Size::new(thumb.min_width, thumb.min_height),
        );
        let mut knob_style = thumb;
        knob_style.opacity = track.opacity;
        cx.slot(knob, &knob_style, |_, _| {});
        if !self.label.is_empty() {
            let width = (bounds.size.width - track.min_width - label.gap).max(0.0);
            let text_x = if cx.rtl {
                bounds.origin.x
            } else {
                rail.max_x() + label.gap
            };
            let text = Rect::from_xywh(text_x, bounds.origin.y, width, bounds.size.height);
            cx.label(text, &self.label, &label, TextAlign::Start);
        }
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Checkbox, &self.label, actions);
        node.description = "switch".into();
        node.state.checked = Some(self.checked);
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
        let ControlValue::Bool(checked) = value else {
            return false;
        };
        self.checked = *checked;
        true
    }
}
