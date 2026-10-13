//! [`InputFilter`]: what a text field lets the user type, paste, or set.
//! The field asks the filter before an edit applies. A rejected edit
//! leaves the text, the caret, and the selection as they were, adds no
//! undo step, and beeps.

use crate::InputMask;
use choscorui_core::grapheme_boundaries;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;

/// The characters that a field allows.
#[derive(Clone)]
pub enum CharClass {
    /// The ASCII digits `0` to `9`.
    Digits,
    /// The ASCII letters and digits.
    AsciiAlphanumeric,
    /// The characters for which the predicate is true.
    Custom(Rc<dyn Fn(char) -> bool>),
}

impl CharClass {
    /// True when the class allows `ch`.
    pub fn allows(&self, ch: char) -> bool {
        match self {
            Self::Digits => ch.is_ascii_digit(),
            Self::AsciiAlphanumeric => ch.is_ascii_alphanumeric(),
            Self::Custom(allows) => allows(ch),
        }
    }
}

impl fmt::Debug for CharClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Digits => f.write_str("Digits"),
            Self::AsciiAlphanumeric => f.write_str("AsciiAlphanumeric"),
            Self::Custom(_) => f.write_str("Custom"),
        }
    }
}

/// What a field does with an edit that its filter accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Accepted {
    /// Puts this text in place of the edited range. A paste longer than
    /// the room that `max_length` leaves is cut to the room.
    Insert(String),
    /// Replaces the whole text with `text` and puts the caret at grapheme
    /// `caret`, as a mask does when it puts its literals in.
    Value { text: String, caret: usize },
}

/// A rule that the whole text of a field must follow while the user
/// types.
type TextRule = Rc<dyn Fn(&str) -> bool>;

/// The rules of a field's text: the characters it allows, its maximum
/// length in grapheme clusters, a rule that the whole text must follow
/// while the user types, and an input mask. The default filter allows
/// every edit.
#[derive(Clone, Default)]
pub struct InputFilter {
    chars: Option<CharClass>,
    max_length: Option<usize>,
    rule: Option<TextRule>,
    mask: Option<InputMask>,
}

impl fmt::Debug for InputFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputFilter")
            .field("chars", &self.chars)
            .field("max_length", &self.max_length)
            .field("rule", &self.rule.is_some())
            .field("mask", &self.mask)
            .finish()
    }
}

impl InputFilter {
    /// A filter that allows every edit.
    pub fn new() -> Self {
        Self::default()
    }

    /// A filter that fits the text into `mask`. An edit keeps the
    /// characters that fit and drops the others. An edit of which no
    /// character fits is rejected.
    pub fn masked(mask: InputMask) -> Self {
        Self {
            mask: Some(mask),
            ..Self::default()
        }
    }

    /// Allows only the characters of `class`.
    #[must_use]
    pub fn chars(mut self, class: CharClass) -> Self {
        self.chars = Some(class);
        self
    }

    /// Allows at most `length` grapheme clusters.
    #[must_use]
    pub fn max_length(mut self, length: usize) -> Self {
        self.max_length = Some(length);
        self
    }

    /// Allows only text for which `rule` is true, such as the start of a
    /// number. The rule sees the whole text after the edit.
    #[must_use]
    pub fn rule(mut self, rule: impl Fn(&str) -> bool + 'static) -> Self {
        self.rule = Some(Rc::new(rule));
        self
    }

    /// The mask, if the filter has one.
    pub fn mask(&self) -> Option<&InputMask> {
        self.mask.as_ref()
    }

    /// True when the filter allows every edit.
    pub fn is_open(&self) -> bool {
        self.chars.is_none()
            && self.max_length.is_none()
            && self.rule.is_none()
            && self.mask.is_none()
    }

    /// What to do with an edit that puts `insert` in place of the
    /// grapheme `range` of `current`, or `None` to reject it. The edit is
    /// one unit: when a character of `insert` is not allowed, the whole
    /// edit is rejected. A deletion is always allowed, except that a mask
    /// can change it.
    pub fn accept(&self, current: &str, range: Range<usize>, insert: &str) -> Option<Accepted> {
        let bounds = grapheme_boundaries(current);
        let count = bounds.len() - 1;
        let range = range.start.min(count)..range.end.min(count).max(range.start.min(count));
        if let Some(mask) = &self.mask {
            return mask_edit(mask, current, &bounds, range, insert);
        }
        if insert.is_empty() {
            return Some(Accepted::Insert(String::new()));
        }
        if let Some(class) = &self.chars
            && !insert.chars().all(|ch| class.allows(ch))
        {
            return None;
        }
        let mut insert = insert.to_owned();
        if let Some(max) = self.max_length {
            let room = max.saturating_sub(count - range.len());
            let inserted = grapheme_boundaries(&insert);
            if room == 0 {
                return None;
            }
            if inserted.len() - 1 > room {
                insert.truncate(inserted[room]);
            }
        }
        if let Some(rule) = &self.rule {
            let text = splice(current, &bounds, &range, &insert);
            if !rule(&text) {
                return None;
            }
        }
        Some(Accepted::Insert(insert))
    }
}

/// `current` with its grapheme `range` replaced by `insert`.
fn splice(current: &str, bounds: &[usize], range: &Range<usize>, insert: &str) -> String {
    let mut text = String::with_capacity(current.len() + insert.len());
    text.push_str(&current[..bounds[range.start]]);
    text.push_str(insert);
    text.push_str(&current[bounds[range.end]..]);
    text
}

/// The edit of a masked field. The caret keeps its place among the
/// characters that the user typed, and a deletion of only literals
/// deletes the typed character before them.
fn mask_edit(
    mask: &InputMask,
    current: &str,
    bounds: &[usize],
    range: Range<usize>,
    insert: &str,
) -> Option<Accepted> {
    let candidate = splice(current, bounds, &range, insert);
    let caret = range.start + grapheme_boundaries(insert).len() - 1;
    let caret_byte = grapheme_boundaries(&candidate)
        .get(caret)
        .copied()
        .unwrap_or(candidate.len());
    let after = mask.apply(&candidate);
    let mut typed = mask.apply(&candidate[..caret_byte]).value.chars().count();
    let mut value: Vec<char> = after.value.chars().collect();
    if insert.is_empty() {
        let without = mask.apply(current).value;
        if after.value == without && !range.is_empty() && typed > 0 {
            // Only literals went: delete the typed character before them.
            value.remove(typed - 1);
            typed -= 1;
        }
    } else {
        let base = splice(current, bounds, &range, "");
        if after.value == mask.apply(&base).value {
            return None;
        }
    }
    let value: String = value.into_iter().collect();
    let text = mask.apply(&value).display;
    let prefix: String = value.chars().take(typed).collect();
    let caret = if typed >= value.chars().count() {
        grapheme_boundaries(&text).len() - 1
    } else {
        grapheme_boundaries(&mask.apply(&prefix).display).len() - 1
    };
    Some(Accepted::Value { text, caret })
}
