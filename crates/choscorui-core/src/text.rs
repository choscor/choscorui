use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextError {
    InvalidSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextMotion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TextSnapshot {
    text: String,
    anchor: usize,
    caret: usize,
}

/// UTF-8 text with explicit boundary conversions. Byte offsets may be inside
/// a grapheme; grapheme APIs only accept whole extended grapheme clusters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextBuffer {
    text: String,
    anchor: usize,
    caret: usize,
    undo: Vec<TextSnapshot>,
    redo: Vec<TextSnapshot>,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            anchor: 0,
            caret: 0,
            undo: Vec::new(),
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
        let previous = self.snapshot();
        self.replace_selection_from(replacement, previous);
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

    fn replace_selection_from(&mut self, replacement: &str, previous: TextSnapshot) {
        let selected = self.selection();
        let start = self
            .grapheme_to_byte(selected.start)
            .expect("selection in bounds");
        let end = self
            .grapheme_to_byte(selected.end)
            .expect("selection in bounds");
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
        if self.snapshot() != previous {
            self.undo.push(previous);
            self.redo.clear();
        }
    }

    pub fn delete_backward(&mut self) {
        if self.selection().is_empty() && self.caret > 0 {
            let previous = self.snapshot();
            self.anchor = self.caret - 1;
            self.replace_selection_from("", previous);
        } else if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }

    pub fn delete_forward(&mut self) {
        if self.selection().is_empty() && self.caret < self.grapheme_count() {
            let previous = self.snapshot();
            self.anchor = self.caret;
            self.caret += 1;
            self.replace_selection_from("", previous);
        } else if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    fn snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }

    fn restore(&mut self, snapshot: TextSnapshot) {
        self.text = snapshot.text;
        self.anchor = snapshot.anchor;
        self.caret = snapshot.caret;
    }
}
