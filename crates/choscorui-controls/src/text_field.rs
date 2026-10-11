use crate::InputText;
use crate::style::{PaintContext, VisualState};
use choscorui_core::{
    Dirty, Key, KeyEvent, NodeId, SemanticAction as CoreSemanticAction, SemanticNode, SemanticRole,
    SemanticState, SemanticText, TextBuffer, TextMotion, Tree, TreeError,
};
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationState {
    Valid,
    Intermediate,
    Invalid(String),
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
        }
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
        let focused = tree.focus() == Some(id);
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
        let display = self.display_value();
        let marked = self.composition.as_ref().map(|composition| {
            let start = composition.replacement.start;
            start..start + choscorui_core::grapheme_boundaries(&composition.text).len() - 1
        });
        InputText {
            text: &display,
            placeholder: &self.placeholder,
            caret: Some(self.caret()),
            selection: self.selection(),
            marked,
        }
        .paint(cx, state);
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
        match input {
            TextFieldInput::Text(text) => {
                self.composition = None;
                self.buffer
                    .replace_selection(&text.replace(['\n', '\r'], ""));
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
                if let Some(range) =
                    replacement_utf16.and_then(|range| self.utf16_range_to_graphemes(range))
                {
                    self.buffer
                        .set_selection(range.start, range.end)
                        .expect("converted UTF-16 range is on grapheme boundaries");
                }
                self.buffer
                    .replace_selection(&text.replace(['\n', '\r'], ""));
            }
            TextFieldInput::UnmarkMarkedText => {
                if let Some(composition) = self.composition.take() {
                    self.buffer
                        .set_selection(composition.replacement.start, composition.replacement.end)
                        .expect("composition replacement remains valid");
                    self.buffer.replace_selection(&composition.text);
                }
            }
            TextFieldInput::CancelMarkedText => self.composition = None,
            TextFieldInput::SetValue(text) => {
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
                if !self.secure {
                    self.buffer.cut_selection();
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
        if edited {
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

    pub fn semantics(&self, tree: &Tree, id: NodeId) -> TextFieldSemantics {
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
            focused: tree.focus() == Some(id),
            validation: self.validation.kind(),
            announcement: match &self.validation {
                ValidationState::Invalid(message) => Some(message.clone()),
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
