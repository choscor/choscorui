use std::collections::VecDeque;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Byte offsets of every extended grapheme boundary, including 0 and
/// `text.len()`. Lets callers map positions in O(log n) by binary search.
pub fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries: Vec<usize> = text.grapheme_indices(true).map(|(byte, _)| byte).collect();
    boundaries.push(text.len());
    boundaries
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextError {
    InvalidSelection,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSelection => f.write_str("the selection is outside the text"),
        }
    }
}

impl std::error::Error for TextError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextMotion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

/// One reversible change. Bytes `start..start + inserted.len()` hold
/// `inserted`, which replaced `removed`. Selections are (anchor, caret)
/// grapheme indices. Storing the delta instead of the whole text keeps
/// history proportional to what was typed, not to document size.
#[derive(Clone, Debug, Eq, PartialEq)]
struct TextEdit {
    start: usize,
    removed: String,
    inserted: String,
    before: (usize, usize),
    after: (usize, usize),
}

/// Oldest edits are dropped beyond this many undo steps.
pub const UNDO_LIMIT: usize = 10_000;

/// UTF-8 text with explicit boundary conversions. Byte offsets may be inside
/// a grapheme; grapheme APIs only accept whole extended grapheme clusters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextBuffer {
    text: String,
    anchor: usize,
    caret: usize,
    undo: VecDeque<TextEdit>,
    redo: Vec<TextEdit>,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            anchor: 0,
            caret: 0,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn grapheme_count(&self) -> usize {
        self.text.graphemes(true).count()
    }

    pub fn grapheme_to_byte(&self, index: usize) -> Option<usize> {
        if index == self.grapheme_count() {
            return Some(self.text.len());
        }
        self.text
            .grapheme_indices(true)
            .nth(index)
            .map(|(byte, _)| byte)
    }

    pub fn byte_to_grapheme(&self, byte: usize) -> Option<usize> {
        if byte == self.text.len() {
            return Some(self.grapheme_count());
        }
        self.text
            .grapheme_indices(true)
            .enumerate()
            .find_map(|(index, (start, _))| (start == byte).then_some(index))
    }

    pub fn byte_to_utf16(&self, byte: usize) -> Option<usize> {
        if byte > self.text.len() || !self.text.is_char_boundary(byte) {
            return None;
        }
        Some(self.text[..byte].encode_utf16().count())
    }

    pub fn utf16_to_byte(&self, offset: usize) -> Option<usize> {
        let mut units = 0;
        for (byte, scalar) in self.text.char_indices() {
            if units == offset {
                return Some(byte);
            }
            units += scalar.len_utf16();
            if units > offset {
                return None;
            }
        }
        (units == offset).then_some(self.text.len())
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    pub fn selected_text(&self) -> &str {
        let selected = self.selection();
        let start = self
            .grapheme_to_byte(selected.start)
            .expect("selection in bounds");
        let end = self
            .grapheme_to_byte(selected.end)
            .expect("selection in bounds");
        &self.text[start..end]
    }

    /// Returns display-only masking text. A password control must also restrict
    /// copy, accessibility value exposure, and undo history at its boundary.
    pub fn password_display(&self) -> String {
        "•".repeat(self.grapheme_count())
    }

    pub fn set_selection(&mut self, anchor: usize, caret: usize) -> Result<(), TextError> {
        let len = self.grapheme_count();
        if anchor > len || caret > len {
            return Err(TextError::InvalidSelection);
        }
        self.anchor = anchor;
        self.caret = caret;
        Ok(())
    }

    /// Places a caret obtained from visual hit testing. With `extend`, the
    /// existing anchor is retained; otherwise the selection collapses.
    pub fn set_caret(&mut self, caret: usize, extend: bool) -> Result<(), TextError> {
        if caret > self.grapheme_count() {
            return Err(TextError::InvalidSelection);
        }
        self.caret = caret;
        if !extend {
            self.anchor = caret;
        }
        Ok(())
    }

    /// Moves among extended grapheme boundaries. Without extension, a
    /// horizontal move first collapses an existing selection to that edge.
    pub fn move_caret(&mut self, motion: TextMotion, extend: bool) -> usize {
        let selected = self.selection();
        let len = self.grapheme_count();
        let next = if !extend && !selected.is_empty() {
            match motion {
                TextMotion::Left | TextMotion::WordLeft => selected.start,
                TextMotion::Right | TextMotion::WordRight => selected.end,
                TextMotion::Start => 0,
                TextMotion::End => len,
            }
        } else {
            match motion {
                TextMotion::Left => self.caret.saturating_sub(1),
                TextMotion::Right => (self.caret + 1).min(len),
                TextMotion::Start => 0,
                TextMotion::End => len,
                TextMotion::WordLeft => {
                    let byte = self.grapheme_to_byte(self.caret).expect("caret in bounds");
                    self.text
                        .unicode_word_indices()
                        .take_while(|(start, _)| *start < byte)
                        .last()
                        .and_then(|(start, _)| self.byte_to_grapheme(start))
                        .unwrap_or(0)
                }
                TextMotion::WordRight => {
                    let byte = self.grapheme_to_byte(self.caret).expect("caret in bounds");
                    self.text
                        .unicode_word_indices()
                        .map(|(start, word)| start + word.len())
                        .find(|end| *end > byte)
                        .and_then(|end| self.byte_to_grapheme(end))
                        .unwrap_or(len)
                }
            }
        };
        self.caret = next;
        if !extend {
            self.anchor = next;
        }
        next
    }

    pub fn replace_selection(&mut self, replacement: &str) {
        let before = (self.anchor, self.caret);
        self.replace_selection_from(replacement, before);
    }

    pub fn cut_selection(&mut self) -> String {
        let text = self.selected_text().to_owned();
        if !text.is_empty() {
            self.replace_selection("");
        }
        text
    }

    pub fn paste(&mut self, text: &str) {
        self.replace_selection(text);
    }

    fn replace_selection_from(&mut self, replacement: &str, before: (usize, usize)) {
        let selected = self.selection();
        let start = self
            .grapheme_to_byte(selected.start)
            .expect("selection in bounds");
        let end = self
            .grapheme_to_byte(selected.end)
            .expect("selection in bounds");
        let removed = self.text[start..end].to_owned();
        self.text.replace_range(start..end, replacement);
        let inserted_end = start + replacement.len();
        let caret = self
            .text
            .grapheme_indices(true)
            .enumerate()
            .find_map(|(index, (byte, _))| (byte >= inserted_end).then_some(index))
            .unwrap_or_else(|| self.grapheme_count());
        self.anchor = caret;
        self.caret = caret;
        if removed != replacement || before != (caret, caret) {
            if self.undo.len() == UNDO_LIMIT {
                self.undo.pop_front();
            }
            self.undo.push_back(TextEdit {
                start,
                removed,
                inserted: replacement.to_owned(),
                before,
                after: (caret, caret),
            });
            self.redo.clear();
        }
    }

    pub fn delete_backward(&mut self) {
        if self.selection().is_empty() && self.caret > 0 {
            let before = (self.anchor, self.caret);
            self.anchor = self.caret - 1;
            self.replace_selection_from("", before);
        } else if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }

    pub fn delete_forward(&mut self) {
        if self.selection().is_empty() && self.caret < self.grapheme_count() {
            let before = (self.anchor, self.caret);
            self.anchor = self.caret;
            self.caret += 1;
            self.replace_selection_from("", before);
        } else if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo.pop_back() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.inserted.len(), &edit.removed);
        (self.anchor, self.caret) = edit.before;
        self.redo.push(edit);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.removed.len(), &edit.inserted);
        (self.anchor, self.caret) = edit.after;
        self.undo.push_back(edit);
        true
    }
}
