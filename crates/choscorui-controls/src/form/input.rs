//! A single-line text input mounted through the control host, and the
//! editing glue that input groups share.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::style::{ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, VisualState};
use crate::text_field::{TextField, TextFieldChange, TextFieldInput, ValidationState};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, Size, Tree, grapheme_boundaries,
};
use std::cell::RefCell;

/// Where each caret stop of the painted text is, so a press can place the
/// caret. Paint records it; events read it.
#[derive(Default)]
struct CaretMap {
    rect: Rect,
    scroll: f64,
    rtl: bool,
    /// Offset of each caret stop from the start of the text.
    offsets: Vec<f64>,
}

impl CaretMap {
    fn caret_at(&self, x: f64) -> usize {
        let offset = if self.rtl {
            self.rect.max_x() - x + self.scroll
        } else {
            x - self.rect.origin.x + self.scroll
        };
        self.offsets
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - offset).abs().total_cmp(&(*b - offset).abs()))
            .map_or(0, |(index, _)| index)
    }
}

/// A `TextField` model driven by host events, with the caret geometry of
/// its last paint.
pub(crate) struct Editor {
    pub field: TextField,
    carets: RefCell<CaretMap>,
    dragging: bool,
}

impl Editor {
    pub fn new(field: TextField) -> Self {
        Self {
            field,
            carets: RefCell::new(CaretMap::default()),
            dragging: false,
        }
    }

    fn apply(&mut self, cx: &mut ControlContext<'_>, input: TextFieldInput) -> TextFieldChange {
        let node = cx.node();
        let change = self
            .field
            .handle(cx.tree_mut(), node, input)
            .unwrap_or(TextFieldChange::None);
        match change {
            TextFieldChange::None => {}
            TextFieldChange::Edited => {
                cx.emit(ControlMessage::Changed(ControlValue::Text(
                    self.field.value().to_string(),
                )));
            }
            TextFieldChange::Committed => cx.emit(ControlMessage::Activated),
            _ => {}
        }
        if change != TextFieldChange::None {
            cx.request_paint();
            cx.request_semantics();
        }
        change
    }

    /// Handles `event` for text inside `text`. Returns `Continue` for
    /// events the editor does not use.
    pub fn event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        text: Rect,
    ) -> EventReply {
        let modifiers = cx.modifiers();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if text.contains(*position) || cx.bounds().contains(*position) => {
                cx.request_focus();
                let caret = self.carets.borrow().caret_at(position.x);
                self.apply(
                    cx,
                    TextFieldInput::SetCaret {
                        caret,
                        extend: modifiers.shift,
                    },
                );
                self.dragging = true;
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                let caret = self.carets.borrow().caret_at(position.x);
                self.apply(
                    cx,
                    TextFieldInput::SetCaret {
                        caret,
                        extend: true,
                    },
                );
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.dragging => {
                self.dragging = false;
                cx.capture_pointer(false);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) if cx.is_focused() => {
                let input = match (*key, modifiers.command) {
                    (Key::Character('a'), true) => TextFieldInput::SelectAll,
                    (Key::Character('z'), true) if modifiers.shift => TextFieldInput::Redo,
                    (Key::Character('z'), true) => TextFieldInput::Undo,
                    (Key::Character('x'), true) => TextFieldInput::Cut,
                    (Key::Tab | Key::Escape, _) | (_, true) => return EventReply::Continue,
                    (_, false) => TextFieldInput::Key {
                        event: KeyEvent::Down {
                            key: *key,
                            repeat: false,
                        },
                        extend: modifiers.shift,
                        word: modifiers.option,
                    },
                };
                self.apply(cx, input);
                EventReply::Stop
            }
            ControlEvent::Text(text) if cx.is_focused() => {
                self.apply(cx, TextFieldInput::Text(text.clone()));
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Replaces the value for an assistive `SetValue`.
    pub fn set_value(&mut self, cx: &mut ControlContext<'_>, value: &str) -> bool {
        if !self.field.is_enabled() {
            return false;
        }
        self.apply(cx, TextFieldInput::SetValue(value.to_string()));
        true
    }

    /// Replaces the value without a `Changed` message, also while
    /// disabled. The field drops line breaks. The caller invalidates the
    /// node.
    pub fn replace(&mut self, value: &str) {
        // The field takes a value from outside through any live node; this
        // one exists only for the call.
        let mut scratch = Tree::new(Rect::default());
        let node = scratch.root();
        let enabled = self.field.is_enabled();
        self.field.set_enabled_state(true);
        let input = TextFieldInput::SetValue(value.to_string());
        let _ = self.field.handle(&mut scratch, node, input);
        self.field.set_enabled_state(enabled);
    }

    /// Takes `Text` from the application, as
    /// [`CustomControl::set_value`] describes.
    pub fn set_text(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(text) = value else {
            return false;
        };
        self.replace(text);
        true
    }

    /// Paints the text in `rect` and records where its caret stops are.
    pub fn paint_text(&self, cx: &mut PaintContext<'_>, rect: Rect, state: VisualState) {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        let display = self.field.display_value();
        let offsets: Vec<f64> = grapheme_boundaries(&display)
            .iter()
            .map(|end| cx.measure(&display[..*end], &style))
            .collect();
        let caret_x = offsets.get(self.field.caret()).copied().unwrap_or(0.0);
        let scroll = if state.focused() {
            (caret_x - rect.size.width + 1.0).max(0.0)
        } else {
            0.0
        };
        *self.carets.borrow_mut() = CaretMap {
            rect,
            scroll,
            rtl: cx.rtl,
            offsets,
        };
        self.field.paint_text(cx, rect, state);
    }

    /// The recipe flags: `host` with focus, invalid, and disabled from
    /// the model.
    pub fn state(&self, host: VisualState) -> VisualState {
        self.field.state_for(host.focused()).with(host)
    }

    pub fn semantics(&self) -> SemanticNode {
        self.field.semantics_for(false).shared()
    }
}

/// A text input: the `Input` recipe box around one line of editable text.
/// It fills the width it is given.
pub struct Input {
    editor: Editor,
}

impl Input {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            editor: Editor::new(TextField::new(name, value)),
        }
    }

    /// An input that shows bullets in place of its characters.
    pub fn password(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            editor: Editor::new(TextField::password(name, value)),
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.editor.field = self.editor.field.with_placeholder(placeholder);
        self
    }

    /// The text model: value, selection, and composition.
    pub fn field(&self) -> &TextField {
        &self.editor.field
    }

    pub fn value(&self) -> &str {
        self.editor.field.value()
    }

    /// Marks the input invalid with a message, or valid.
    pub fn set_validation(&mut self, validation: ValidationState) {
        self.editor.field.set_validation_state(validation);
    }

    fn style(&self, style: impl Fn(StyleQuery) -> ResolvedStyle) -> ResolvedStyle {
        style(StyleQuery::new(ControlKind::Input, Slot::Root))
    }
}

/// The width an input asks for without a width constraint: 60 spacing
/// units.
pub(crate) fn default_width(cx: &MeasureContext<'_>, style: &ResolvedStyle) -> f64 {
    let natural = (cx.tokens.spacing * 60.0).max(style.min_width);
    let max = cx.constraints.max.width;
    if max.is_finite() {
        max.max(style.min_width)
    } else {
        natural
    }
}

impl CustomControl for Input {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = self.style(|query| cx.style(query));
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured {
            size: Size::new(default_width(cx, &style), height),
            baseline: None,
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let bounds = cx.bounds();
        self.editor.event(cx, event, bounds)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.editor.state(cx.state);
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        let bounds = cx.bounds;
        let editor = &self.editor;
        cx.slot(bounds, &style, |cx, inner| {
            editor.paint_text(cx, inner, state)
        });
    }

    fn semantics(&self) -> SemanticNode {
        self.editor.semantics()
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
