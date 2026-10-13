//! The editable text of a spin box: one line of text with a caret and a
//! selection, as in an input, next to the stepper column.

use crate::InputFilter;
use crate::custom::{ControlContext, ControlEvent};
use crate::hosted::Parts;
use crate::range_controls::paint_stepper_box;
use crate::style::{PaintContext, VisualState};
use crate::text_field::{TextField, ValidationState};
use choscorui_core::{
    EventReply, Key, Localizer, Point, PointerButton, PointerEvent, Rect, SemanticNode,
};

/// The text of a spin box. Its edits report no message: the spin box
/// reports its value when the user commits the text.
pub(crate) struct StepText {
    editor: TextField,
    /// Hover and press of the steppers, by the key that they step with.
    steppers: Parts<Key>,
}

impl StepText {
    /// The text `text` of the spin box `name`, which takes only what
    /// `filter` accepts.
    pub fn new(name: &str, text: &str, filter: InputFilter) -> Self {
        Self {
            editor: TextField::new(name, text).with_filter(filter).quiet(),
            steppers: Parts::default(),
        }
    }

    pub fn text(&self) -> &str {
        self.editor.value()
    }

    /// Replaces the text from outside the filter, and selects it all, so
    /// that typed text replaces it.
    pub fn set(&mut self, text: &str) {
        self.editor.replace_value(text);
        self.editor.select_all_text();
    }

    /// Selects the whole text.
    pub fn select_all(&mut self) {
        self.editor.select_all_text();
    }

    pub fn set_filter(&mut self, filter: InputFilter) {
        self.editor.set_filter(filter);
    }

    pub fn set_validation(&mut self, validation: ValidationState) {
        self.editor.set_validation_state(validation);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.editor.set_enabled_state(enabled);
    }

    /// Handles an editing event for the text in `text`: a press places
    /// the caret, and keys and typed text edit. Returns the reply and true
    /// when the text changed.
    pub fn event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        text: Rect,
    ) -> (EventReply, bool) {
        let before = self.text().to_owned();
        let reply = self.editor.hosted_event(cx, event, text);
        (reply, self.text() != before)
    }

    /// Tracks hover and press of the steppers at `steppers`. Returns the
    /// key of the stepper that a primary press starts on.
    pub fn track_steppers(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        steppers: Rect,
    ) -> Option<Key> {
        self.steppers
            .track(cx, event, |point| stepper_at(steppers, point));
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            }) => self.steppers.pressed,
            _ => None,
        }
    }

    /// The text part of the spin box's semantics.
    pub fn semantics(&self, strings: &Localizer) -> SemanticNode {
        self.editor.hosted_semantics(strings)
    }

    /// The recipe flags: `host` with the text's focus, invalid, and
    /// disabled flags.
    pub fn state(&self, host: VisualState) -> VisualState {
        self.editor.hosted_state(host)
    }

    /// Paints the input box, the text with its caret and selection, and
    /// the steppers at `steppers`, each hovered and pressed on its own.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState, steppers: Rect) {
        let arrows = [Key::ArrowUp, Key::ArrowDown].map(|key| self.steppers.state(key, state));
        paint_stepper_box(cx, state, steppers, arrows, |cx, rect| {
            self.editor.paint_hosted_text(cx, rect, state);
        });
    }
}

/// The stepper at `point` in the column `steppers`: the up stepper in
/// the top half and the down stepper in the bottom half.
fn stepper_at(steppers: Rect, point: Point) -> Option<Key> {
    let up = point.y < steppers.center().y;
    steppers
        .contains(point)
        .then_some(if up { Key::ArrowUp } else { Key::ArrowDown })
}

/// The text part of a spin box's `bounds`: all but the stepper column.
pub(crate) fn text_rect(bounds: Rect, steppers: Rect) -> Rect {
    let width = (bounds.size.width - steppers.size.width).max(0.0);
    let x = if steppers.origin.x <= bounds.origin.x {
        steppers.max_x()
    } else {
        bounds.origin.x
    };
    Rect::from_xywh(x, bounds.origin.y, width, bounds.size.height)
}
