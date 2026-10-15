//! The text of a node for the `Text` interface. AT-SPI counts offsets in
//! characters (Unicode scalar values); the semantic tree counts UTF-16
//! code units.

use choscorui_core::{SemanticEntry, SemanticRole};
use std::ops::Range;

/// The text of a node that has the `Text` interface: the value of a text
/// control, or the name of static text.
pub fn content(entry: &SemanticEntry) -> Option<&str> {
    let node = &entry.node;
    if node.text.is_some() {
        Some(node.value.as_deref().unwrap_or_default())
    } else if node.role == SemanticRole::StaticText {
        Some(&node.name)
    } else {
        None
    }
}

/// The character count of `text`.
pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// The character offset of the UTF-16 offset `utf16` in `text`. An
/// offset inside a character, or past the end, rounds down.
pub fn char_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.chars().enumerate() {
        units += character.len_utf16();
        if units > utf16 {
            return index;
        }
    }
    char_count(text)
}

/// The selection of a text node in characters.
pub fn selection(entry: &SemanticEntry) -> Option<Range<usize>> {
    let text = content(entry)?;
    let range = &entry.node.text.as_ref()?.selection_utf16;
    Some(char_offset(text, range.start)..char_offset(text, range.end))
}

/// The characters `range` of `text`. The range is clamped to the text.
pub fn slice(text: &str, range: Range<usize>) -> String {
    let start = range.start.min(range.end);
    text.chars()
        .skip(start)
        .take(range.end.saturating_sub(start))
        .collect()
}

/// A text unit of `GetTextAtOffset` and `GetStringAtOffset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unit {
    Char,
    Word,
    /// A sentence, a line, or a paragraph. A line ends at a line break.
    Line,
}

impl Unit {
    /// The unit of an `AtspiTextBoundaryType`.
    pub fn from_boundary(boundary: u32) -> Self {
        match boundary {
            0 => Self::Char,
            1 | 2 => Self::Word,
            _ => Self::Line,
        }
    }

    /// The unit of an `AtspiTextGranularity`.
    pub fn from_granularity(granularity: u32) -> Self {
        match granularity {
            0 => Self::Char,
            1 => Self::Word,
            _ => Self::Line,
        }
    }
}

/// The unit of `text` at the character `offset`: a character, a word
/// with its trailing spaces, or a line with its line break.
pub fn unit_at(text: &str, offset: usize, unit: Unit) -> Range<usize> {
    let chars: Vec<char> = text.chars().collect();
    if offset >= chars.len() {
        return chars.len()..chars.len();
    }
    match unit {
        Unit::Char => offset..offset + 1,
        Unit::Word => {
            let space = |index: usize| chars[index].is_whitespace();
            let mut start = offset;
            // In the spaces after a word, the unit is that word.
            while start > 0 && space(start) {
                start -= 1;
            }
            while start > 0 && !space(start - 1) {
                start -= 1;
            }
            let mut end = start;
            while end < chars.len() && !space(end) {
                end += 1;
            }
            while end < chars.len() && space(end) {
                end += 1;
            }
            start..end
        }
        Unit::Line => {
            let mut start = offset;
            while start > 0 && chars[start - 1] != '\n' {
                start -= 1;
            }
            let mut end = offset;
            while end < chars.len() && chars[end] != '\n' {
                end += 1;
            }
            if end < chars.len() {
                end += 1;
            }
            start..end
        }
    }
}

/// The change from `old` to `new`: the character offset where they
/// differ, the removed text, and the inserted text.
pub fn change(old: &str, new: &str) -> Option<(usize, String, String)> {
    if old == new {
        return None;
    }
    let old: Vec<char> = old.chars().collect();
    let new: Vec<char> = new.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old[prefix..old.len() - suffix].iter().collect();
    let inserted = new[prefix..new.len() - suffix].iter().collect();
    Some((prefix, removed, inserted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_convert_between_units() {
        let text = "a😀b";
        assert_eq!(char_offset(text, 0), 0);
        assert_eq!(char_offset(text, 1), 1);
        assert_eq!(char_offset(text, 3), 2);
        assert_eq!(char_offset(text, 9), 3);
        assert_eq!(slice(text, 1..3), "😀b");
    }

    #[test]
    fn units_cover_words_and_lines() {
        let text = "one two\nthree";
        assert_eq!(unit_at(text, 1, Unit::Word), 0..4);
        assert_eq!(unit_at(text, 3, Unit::Word), 0..4);
        assert_eq!(unit_at(text, 5, Unit::Word), 4..8);
        assert_eq!(unit_at(text, 2, Unit::Line), 0..8);
        assert_eq!(unit_at(text, 9, Unit::Line), 8..13);
        assert_eq!(unit_at(text, 0, Unit::Char), 0..1);
        assert_eq!(unit_at(text, 13, Unit::Char), 13..13);
    }

    #[test]
    fn changes_find_the_edited_range() {
        assert_eq!(change("abc", "abc"), None);
        assert_eq!(
            change("hello", "help!"),
            Some((3, "lo".to_owned(), "p!".to_owned()))
        );
        assert_eq!(change("", "x"), Some((0, String::new(), "x".to_owned())));
    }
}
