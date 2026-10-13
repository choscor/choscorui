//! The editable text of a spin box: one line of text with a caret and a
//! selection, as in an input, next to the stepper column.

use super::input::Editor;
use crate::InputFilter;
use crate::custom::{ControlContext, ControlEvent};
use crate::range_controls::paint_stepper_box;
use crate::style::{PaintContext, VisualState};
use crate::text_field::{TextField, ValidationState};
use choscorui_core::{EventReply, Localizer, Rect, SemanticNode};

/// The text of a spin box. Its edits report no message: the spin box
/// reports its value when the user commits the text.
pub(crate) struct StepText {
    editor: Editor,
}

impl StepText {
    /// The text `text` of the spin box `name`, which takes only what
    /// `filter` accepts.
    pub fn new(name: &str, text: &str, filter: InputFilter) -> Self {
        Self {
            editor: Editor::quiet(TextField::new(name, text).with_filter(filter)),
        }
    }

    pub fn text(&self) -> &str {
        self.editor.field.value()
    }

    /// Replaces the text from outside the filter, and selects it all, so
    /// that typed text replaces it.
    pub fn set(&mut self, text: &str) {
        self.editor.replace(text);
        self.editor.field.select_all_text();
    }

    /// Selects the whole text.
    pub fn select_all(&mut self) {
        self.editor.field.select_all_text();
    }

    pub fn set_filter(&mut self, filter: InputFilter) {
        self.editor.field.set_filter(filter);
    }

    pub fn set_validation(&mut self, validation: ValidationState) {
        self.editor.field.set_validation_state(validation);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.editor.field.set_enabled_state(enabled);
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
        let reply = self.editor.event(cx, event, text);
        (reply, self.text() != before)
    }

    /// The text part of the spin box's semantics.
    pub fn semantics(&self, strings: &Localizer) -> SemanticNode {
        self.editor.semantics(strings)
    }

    /// The recipe flags: `host` with the text's focus, invalid, and
    /// disabled flags.
    pub fn state(&self, host: VisualState) -> VisualState {
        self.editor.state(host)
    }

    /// Paints the input box, the text with its caret and selection, and
    /// the steppers at `steppers`.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState, steppers: Rect) {
        paint_stepper_box(cx, state, steppers, |cx, rect| {
            self.editor.paint_text(cx, rect, state);
        });
    }
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
