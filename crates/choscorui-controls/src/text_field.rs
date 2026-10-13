use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, VisualState};
use crate::{Accepted, InputFilter, InputText, ValidationError};
use choscorui_core::Localizer;
use choscorui_core::{
    Dirty, Key, KeyEvent, NodeId, PointerButton, PointerEvent, Rect,
    SemanticAction as CoreSemanticAction, SemanticNode, SemanticRole, SemanticState, SemanticText,
    Size, TextBuffer, TextMotion, Tree, TreeError,
};
use std::cell::RefCell;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum TextFieldInput {
    Text(String),
    SetMarkedText {
        text: String,
        selected_utf16: Range<usize>,
        replacement_utf16: Option<Range<usize>>,
    },
    CommitMarkedText {
        text: String,
        replacement_utf16: Option<Range<usize>>,
    },
    UnmarkMarkedText,
    CancelMarkedText,
    Key {
        event: KeyEvent,
        extend: bool,
        word: bool,
    },
    SetValue(String),
    SetValidation(ValidationState),
    SetCaret {
        caret: usize,
        extend: bool,
    },
    Undo,
    Redo,
    SelectAll,
    Cut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextFieldChange {
    None,
    Selection,
    Edited,
    Committed,
    CommitRejected,
    ValidationChanged,
    Composition,
    /// The filter rejected the edit. Nothing changed.
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextComposition {
    pub text: String,
    pub selected_utf16: Range<usize>,
    pub replacement: Range<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationKind {
    Valid,
    Intermediate,
    Invalid,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValidationState {
    Valid,
    Intermediate,
    Invalid(ValidationError),
}

impl ValidationState {
    pub fn kind(&self) -> ValidationKind {
        match self {
            Self::Valid => ValidationKind::Valid,
            Self::Intermediate => ValidationKind::Intermediate,
            Self::Invalid(_) => ValidationKind::Invalid,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextFieldSemanticAction {
    SetValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextFieldSemantics {
    pub role: &'static str,
    pub name: String,
    pub value: String,
    pub enabled: bool,
    pub focused: bool,
    pub validation: ValidationKind,
    pub announcement: Option<String>,
    pub actions: &'static [TextFieldSemanticAction],
    pub character_count_utf16: usize,
    pub selection_utf16: Range<usize>,
    pub secure: bool,
}

impl TextFieldSemantics {
    pub fn shared(&self) -> SemanticNode {
        SemanticNode {
            role: if self.secure {
                SemanticRole::PasswordField
            } else {
                SemanticRole::TextField
            },
            name: self.name.clone(),
            description: self.announcement.clone().unwrap_or_default(),
            value: Some(self.value.clone()),
            state: SemanticState {
                disabled: !self.enabled,
                invalid: self.validation == ValidationKind::Invalid,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: self
                .enabled
                .then_some([CoreSemanticAction::SetValue, CoreSemanticAction::Focus])
                .into_iter()
                .flatten()
                .collect(),
            text: Some(SemanticText {
                character_count_utf16: self.character_count_utf16,
                selection_utf16: self.selection_utf16.clone(),
                multiline: false,
            }),
        }
    }
}

/// Single-line editing behavior. Platform text services feed committed text
/// through `Text`; validation and application commit policy remain external.
pub struct TextField {
    name: String,
    buffer: TextBuffer,
    enabled: bool,
    validation: ValidationState,
    composition: Option<TextComposition>,
    secure: bool,
    placeholder: String,
    /// What the user may type, paste, or set.
    filter: InputFilter,
    /// Caret placement by pointer in a control host.
    pointer: HostedPointer,
}

impl TextField {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        let mut buffer = TextBuffer::new(value.into());
        let end = buffer.grapheme_count();
        buffer
            .set_selection(end, end)
            .expect("end is a valid boundary");
        Self {
            name: name.into(),
            buffer,
            enabled: true,
            validation: ValidationState::Valid,
            composition: None,
            secure: false,
            placeholder: String::new(),
            filter: InputFilter::new(),
            pointer: HostedPointer::default(),
        }
    }

    /// Lets the user type, paste, and set only what `filter` accepts.
    #[must_use]
    pub fn with_filter(mut self, filter: InputFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the filter of later edits. The current text stays.
    pub fn set_filter(&mut self, filter: InputFilter) {
        self.filter = filter;
    }

    pub fn filter(&self) -> &InputFilter {
        &self.filter
    }

    /// Text shown in muted color while the value is empty.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    pub fn password(name: impl Into<String>, value: impl Into<String>) -> Self {
        let mut field = Self::new(name, value);
        field.secure = true;
        field
    }

    pub fn value(&self) -> &str {
        self.buffer.as_str()
    }
    pub fn selection(&self) -> Range<usize> {
        self.buffer.selection()
    }
    pub fn caret(&self) -> usize {
        self.buffer.caret()
    }
    pub fn selected_text(&self) -> &str {
        if self.secure {
            ""
        } else {
            self.buffer.selected_text()
        }
    }
    pub fn composition(&self) -> Option<&TextComposition> {
        self.composition.as_ref()
    }
    pub fn display_value(&self) -> String {
        let base = if self.secure {
            self.buffer.password_display()
        } else {
            self.buffer.as_str().to_owned()
        };
        let Some(composition) = &self.composition else {
            return base;
        };
        let display_buffer = TextBuffer::new(&base);
        let start = display_buffer
            .grapheme_to_byte(composition.replacement.start)
            .expect("composition replacement starts on a grapheme boundary");
        let end = display_buffer
            .grapheme_to_byte(composition.replacement.end)
            .expect("composition replacement ends on a grapheme boundary");
        let mut display = base;
        display.replace_range(start..end, &composition.text);
        display
    }
    pub fn marked_range_utf16(&self) -> Option<Range<usize>> {
        let composition = self.composition.as_ref()?;
        let byte = self
            .buffer
            .grapheme_to_byte(composition.replacement.start)?;
        let start = self.buffer.byte_to_utf16(byte)?;
        Some(start..start + composition.text.encode_utf16().count())
    }

    /// The flags the input recipe reads. A focused field always shows
    /// the focus ring, as text inputs do.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> VisualState {
        self.state_for(tree.focus() == Some(id))
    }

    /// The recipe flags with focus given directly.
    pub(crate) fn state_for(&self, focused: bool) -> VisualState {
        VisualState::NONE
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::FOCUSED, focused)
            .set(VisualState::FOCUS_VISIBLE, focused)
            .set(
                VisualState::INVALID,
                self.validation.kind() == ValidationKind::Invalid,
            )
    }

    /// Paints the input box, the display value or placeholder, the
    /// selection, marked text, and the caret.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let state = self.visual_state(tree, id);
        self.paint_state(cx, state);
    }

    /// Paints like [`paint`](Self::paint) with the recipe flags given.
    pub(crate) fn paint_state(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let display = self.display_value();
        self.input_text(&display).paint(cx, state);
    }

    /// Paints only the text, selection, marks, and caret inside `rect`.
    pub(crate) fn paint_text(&self, cx: &mut PaintContext<'_>, rect: Rect, state: VisualState) {
        let display = self.display_value();
        self.input_text(&display).paint_text(cx, rect, state);
    }

    fn input_text<'a>(&'a self, display: &'a str) -> InputText<'a> {
        let marked = self.composition.as_ref().map(|composition| {
            let start = composition.replacement.start;
            start..start + choscorui_core::grapheme_boundaries(&composition.text).len() - 1
        });
        InputText {
            text: display,
            placeholder: &self.placeholder,
            caret: Some(self.caret()),
            selection: self.selection(),
            marked,
        }
    }

    /// Selects the whole text, without a tree, for hosted fields.
    pub(crate) fn select_all_text(&mut self) {
        let count = self.buffer.grapheme_count();
        let _ = self.buffer.set_selection(0, count);
    }

    /// Sets the validation state without a tree, for hosted inputs.
    pub(crate) fn set_validation_state(&mut self, validation: ValidationState) {
        self.validation = validation;
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables without a tree, for hosted inputs.
    pub(crate) fn set_enabled_state(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.composition = None;
        }
    }

    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.composition = None;
            }
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: TextFieldInput,
    ) -> Result<TextFieldChange, TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        let external = matches!(
            input,
            TextFieldInput::SetValue(_) | TextFieldInput::SetValidation(_)
        );
        if (!self.enabled && !matches!(input, TextFieldInput::SetValidation(_)))
            || (!external && tree.focus() != Some(id))
        {
            return Ok(TextFieldChange::None);
        }
        let before_value = self.buffer.as_str().to_owned();
        let before_selection = self.buffer.selection();
        let before_validation = self.validation.clone();
        let before_composition = self.composition.clone();
        let mut committed = false;
        let mut commit_rejected = false;
        let mut rejected = false;
        match input {
            TextFieldInput::Text(text) => {
                self.composition = None;
                let range = self.buffer.selection();
                rejected = !self.edit(range, &text.replace(['\n', '\r'], ""));
            }
            TextFieldInput::SetMarkedText {
                text,
                selected_utf16,
                replacement_utf16,
            } => {
                let text = text.replace(['\n', '\r'], "");
                let length = text.encode_utf16().count();
                let start = selected_utf16.start.min(length);
                let end = selected_utf16.end.min(length).max(start);
                self.composition = Some(TextComposition {
                    text,
                    selected_utf16: start..end,
                    replacement: replacement_utf16
                        .and_then(|range| self.utf16_range_to_graphemes(range))
                        .unwrap_or_else(|| self.buffer.selection()),
                });
            }
            TextFieldInput::CommitMarkedText {
                text,
                replacement_utf16,
            } => {
                self.composition = None;
                let range = replacement_utf16
                    .and_then(|range| self.utf16_range_to_graphemes(range))
                    .unwrap_or_else(|| self.buffer.selection());
                rejected = !self.edit(range, &text.replace(['\n', '\r'], ""));
            }
            TextFieldInput::UnmarkMarkedText => {
                if let Some(composition) = self.composition.take() {
                    rejected = !self.edit(composition.replacement, &composition.text);
                }
            }
            TextFieldInput::CancelMarkedText => self.composition = None,
            TextFieldInput::SetValue(text) => {
                self.composition = None;
                let all = 0..self.buffer.grapheme_count();
                let text = text.replace(['\n', '\r'], "");
                rejected = text != self.buffer.as_str() && !self.edit(all, &text);
            }
            TextFieldInput::Undo => {
                self.buffer.undo();
            }
            TextFieldInput::Redo => {
                self.buffer.redo();
            }
            TextFieldInput::SelectAll => {
                let count = self.buffer.grapheme_count();
                self.buffer
                    .set_selection(0, count)
                    .expect("buffer range is valid");
            }
            TextFieldInput::Cut => {
                let range = self.buffer.selection();
                if !self.secure && !range.is_empty() {
                    if self.filter.mask().is_some() {
                        self.edit(range, "");
                    } else {
                        self.buffer.cut_selection();
                    }
                }
            }
            TextFieldInput::SetCaret { caret, extend } => {
                let _ = self.buffer.set_caret(caret, extend);
            }
            TextFieldInput::SetValidation(validation) => {
                self.validation = validation;
            }
            TextFieldInput::Key {
                event: KeyEvent::Down { key, repeat },
                extend,
                word,
            } => match key {
                Key::Backspace | Key::Delete if self.filter.mask().is_some() => {
                    let selection = self.buffer.selection();
                    let (caret, count) = (self.buffer.caret(), self.buffer.grapheme_count());
                    let range = if !selection.is_empty() {
                        selection
                    } else if key == Key::Backspace {
                        caret.saturating_sub(1)..caret
                    } else {
                        caret..(caret + 1).min(count)
                    };
                    if !range.is_empty() {
                        self.edit(range, "");
                    }
                }
                Key::Backspace => self.buffer.delete_backward(),
                Key::Delete => self.buffer.delete_forward(),
                Key::ArrowLeft => {
                    self.buffer.move_caret(
                        if word {
                            TextMotion::WordLeft
                        } else {
                            TextMotion::Left
                        },
                        extend,
                    );
                }
                Key::ArrowRight => {
                    self.buffer.move_caret(
                        if word {
                            TextMotion::WordRight
                        } else {
                            TextMotion::Right
                        },
                        extend,
                    );
                }
                Key::Home => {
                    self.buffer.move_caret(TextMotion::Start, extend);
                }
                Key::End => {
                    self.buffer.move_caret(TextMotion::End, extend);
                }
                Key::Enter if !repeat => {
                    if self.validation == ValidationState::Valid {
                        committed = true;
                    } else {
                        commit_rejected = true;
                    }
                }
                _ => {}
            },
            TextFieldInput::Key { .. } => {}
        }
        let edited = self.buffer.as_str() != before_value;
        let selection_changed = self.buffer.selection() != before_selection;
        let validation_changed = self.validation != before_validation;
        let composition_changed = self.composition != before_composition;
        if rejected {
            Ok(TextFieldChange::Rejected)
        } else if edited {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            Ok(TextFieldChange::Edited)
        } else if validation_changed {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            Ok(TextFieldChange::ValidationChanged)
        } else if committed {
            Ok(TextFieldChange::Committed)
        } else if commit_rejected {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            Ok(TextFieldChange::CommitRejected)
        } else if selection_changed {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            Ok(TextFieldChange::Selection)
        } else if composition_changed {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            Ok(TextFieldChange::Composition)
        } else {
            Ok(TextFieldChange::None)
        }
    }

    /// The semantics, with the built-in English messages.
    pub fn semantics(&self, tree: &Tree, id: NodeId) -> TextFieldSemantics {
        self.semantics_for(tree.focus() == Some(id), Localizer::builtin())
    }

    /// The semantics with focus given directly, and the validation
    /// message in the locale of `strings`.
    pub(crate) fn semantics_for(&self, focused: bool, strings: &Localizer) -> TextFieldSemantics {
        const ACTIONS: &[TextFieldSemanticAction] = &[TextFieldSemanticAction::SetValue];
        TextFieldSemantics {
            role: if self.secure {
                "password field"
            } else {
                "text field"
            },
            name: self.name.clone(),
            value: if self.secure {
                self.buffer.password_display()
            } else {
                self.buffer.as_str().to_owned()
            },
            enabled: self.enabled,
            focused,
            validation: self.validation.kind(),
            announcement: match &self.validation {
                ValidationState::Invalid(error) => Some(error.message(strings)),
                _ => None,
            },
            actions: if self.enabled { ACTIONS } else { &[] },
            character_count_utf16: self.buffer.as_str().encode_utf16().count(),
            selection_utf16: {
                let selected = self.buffer.selection();
                let start = self
                    .buffer
                    .grapheme_to_byte(selected.start)
                    .and_then(|byte| self.buffer.byte_to_utf16(byte))
                    .expect("selection start is a grapheme boundary");
                let end = self
                    .buffer
                    .grapheme_to_byte(selected.end)
                    .and_then(|byte| self.buffer.byte_to_utf16(byte))
                    .expect("selection end is a grapheme boundary");
                start..end
            },
            secure: self.secure,
        }
    }

    /// Puts `insert` in place of the grapheme `range` when the filter
    /// accepts it, as one undoable edit. Returns false when the filter
    /// rejects it: then nothing changes.
    fn edit(&mut self, range: Range<usize>, insert: &str) -> bool {
        match self
            .filter
            .accept(self.buffer.as_str(), range.clone(), insert)
        {
            None => false,
            Some(Accepted::Insert(text)) => {
                self.buffer
                    .set_selection(range.start, range.end)
                    .expect("the edited range is on grapheme boundaries");
                self.buffer.replace_selection(&text);
                true
            }
            Some(Accepted::Value { text, caret }) => {
                if text != self.buffer.as_str() {
                    let count = self.buffer.grapheme_count();
                    self.buffer
                        .set_selection(0, count)
                        .expect("buffer range is valid");
                    self.buffer.replace_selection(&text);
                }
                let _ = self.buffer.set_caret(caret, false);
                true
            }
        }
    }

    /// Replaces the whole value with `text` without line breaks, as one
    /// undoable edit, and drops marked text. The value comes from the
    /// application, so the filter does not check it, but a mask formats
    /// it.
    pub(crate) fn replace_value(&mut self, text: &str) {
        let masked = self.filter.mask().map(|mask| mask.apply(text).display);
        let text = masked.as_deref().unwrap_or(text);
        self.composition = None;
        let text = text.replace(['\n', '\r'], "");
        if text != self.buffer.as_str() {
            let count = self.buffer.grapheme_count();
            self.buffer
                .set_selection(0, count)
                .expect("buffer range is valid");
            self.buffer.replace_selection(&text);
        }
    }

    fn utf16_range_to_graphemes(&self, range: Range<usize>) -> Option<Range<usize>> {
        if range.start > range.end {
            return None;
        }
        let start = self
            .buffer
            .utf16_to_byte(range.start)
            .and_then(|byte| self.buffer.byte_to_grapheme(byte))?;
        let end = self
            .buffer
            .utf16_to_byte(range.end)
            .and_then(|byte| self.buffer.byte_to_grapheme(byte))?;
        Some(start..end)
    }
}

/// Where each caret stop of the painted text is, so a press in a control
/// host can place the caret. Paint records it; events read it.
#[derive(Default)]
struct CaretStops {
    rect: Rect,
    scroll: f64,
    rtl: bool,
    /// Offset of each caret stop from the start of the text.
    offsets: Vec<f64>,
}

impl CaretStops {
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

/// The pointer state of a hosted field: the caret stops of the last paint
/// and a selection drag.
#[derive(Default)]
struct HostedPointer {
    stops: RefCell<CaretStops>,
    dragging: bool,
}

impl TextField {
    /// Runs `input` through `handle` and reports an edit as
    /// `Changed(Text)` and a commit as `Activated`.
    fn apply(&mut self, cx: &mut crate::ControlContext<'_>, input: TextFieldInput) {
        let node = cx.node();
        let change = self
            .handle(cx.tree_mut(), node, input)
            .unwrap_or(TextFieldChange::None);
        match change {
            TextFieldChange::None => return,
            TextFieldChange::Rejected => {
                cx.beep();
                return;
            }
            TextFieldChange::Edited => cx.emit(crate::ControlMessage::Changed(
                crate::ControlValue::Text(self.value().to_owned()),
            )),
            TextFieldChange::Committed => cx.emit(crate::ControlMessage::Activated),
            _ => {}
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// The field input for a key-down event with the held modifiers, or
    /// `None` for a key that the field leaves to its ancestors.
    fn key_input(key: Key, modifiers: crate::ShortcutModifiers) -> Option<TextFieldInput> {
        Some(match (key, modifiers.command) {
            (Key::Character('a'), true) => TextFieldInput::SelectAll,
            (Key::Character('z'), true) if modifiers.shift => TextFieldInput::Redo,
            (Key::Character('z'), true) => TextFieldInput::Undo,
            (Key::Character('x'), true) => TextFieldInput::Cut,
            (Key::Tab | Key::Escape, _) | (_, true) => return None,
            (_, false) => TextFieldInput::Key {
                event: KeyEvent::Down { key, repeat: false },
                extend: modifiers.shift,
                word: modifiers.option,
            },
        })
    }
}

impl crate::CustomControl for TextField {
    /// The input recipe height, and the width of an input.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        let size = Size::new(crate::form::input::default_width(cx, &style), height);
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use crate::ControlEvent;
        use choscorui_core::EventReply;
        if cx.phase() != choscorui_core::EventPhase::Target {
            return EventReply::Continue;
        }
        let modifiers = cx.modifiers();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                cx.request_focus();
                let caret = self.pointer.stops.borrow().caret_at(position.x);
                let extend = modifiers.shift;
                self.apply(cx, TextFieldInput::SetCaret { caret, extend });
                self.pointer.dragging = true;
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.pointer.dragging => {
                let caret = self.pointer.stops.borrow().caret_at(position.x);
                self.apply(
                    cx,
                    TextFieldInput::SetCaret {
                        caret,
                        extend: true,
                    },
                );
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.pointer.dragging => {
                self.pointer.dragging = false;
                cx.capture_pointer(false);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) if cx.is_focused() => {
                match Self::key_input(*key, modifiers) {
                    Some(input) => {
                        self.apply(cx, input);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            ControlEvent::Text(text) if cx.is_focused() => {
                self.apply(cx, TextFieldInput::Text(text.clone()));
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) | ControlEvent::Dismiss => {
                if self.pointer.dragging {
                    self.pointer.dragging = false;
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints like [`TextField::paint`] with the host's focus, and records
    /// the caret stops for pointer input.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.state_for(cx.state.focused()).with(cx.state);
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |cx, rect| {
            let display = self.display_value();
            let offsets: Vec<f64> = choscorui_core::grapheme_boundaries(&display)
                .iter()
                .map(|end| cx.measure(&display[..*end], &style))
                .collect();
            let caret_x = offsets.get(self.caret()).copied().unwrap_or(0.0);
            let scroll = if state.focused() {
                (caret_x - rect.size.width + 1.0).max(0.0)
            } else {
                0.0
            };
            *self.pointer.stops.borrow_mut() = CaretStops {
                rect,
                scroll,
                rtl: cx.rtl,
                offsets,
            };
            self.paint_text(cx, rect, state);
        });
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.semantics_for(false, strings).shared()
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: CoreSemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            CoreSemanticAction::SetValue if self.enabled => {
                let text = value.unwrap_or_default().to_owned();
                self.apply(cx, TextFieldInput::SetValue(text));
                true
            }
            CoreSemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.set_enabled_state(enabled);
        if !enabled {
            self.pointer.dragging = false;
        }
    }

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match value {
            crate::ControlValue::Text(text) => {
                self.replace_value(text);
                true
            }
            _ => false,
        }
    }
}
