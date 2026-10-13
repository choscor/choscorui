//! Input masks and locale-aware numeric and date input contracts. Each
//! validator maps raw text to a [`ValidationState`]: `Intermediate` while
//! the user may still be typing toward a valid value, `Invalid` with a
//! typed error once no continuation can be valid, and `Valid` with a
//! parsed value. Each rule also gives the [`InputFilter`] that rejects
//! impossible text at once, and a `commit` check for Return and blur.

use super::ValidationState;
use crate::InputFilter;
use crate::validation::{DateError, NumberError, ValidationError};
use choscorui_core::{CivilDate, DateOrder, Locale};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotKind {
    Digit,
    Letter,
    Alphanumeric,
    Any,
}

/// The case that a mask gives the letters of a slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Keep,
    Upper,
    Lower,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Slot {
    Input {
        kind: SlotKind,
        required: bool,
        case: Case,
    },
    Literal(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaskError {
    Empty,
    TrailingEscape,
}

impl std::fmt::Display for MaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the input mask is empty"),
            Self::TrailingEscape => f.write_str("the input mask ends with an escape character"),
        }
    }
}

impl std::error::Error for MaskError {}

/// A fixed-format mask such as `(000) 000-0000` or `AA-9999`.
///
/// | Symbol | Accepts | Required |
/// | --- | --- | --- |
/// | `0` / `9` | digit | yes / no |
/// | `A` / `a` | letter | yes / no |
/// | `N` / `n` | letter or digit | yes / no |
/// | `X` / `x` | any character | yes / no |
/// | `>` | uppercases the letters of the slots after it | — |
/// | `<` | lowercases the letters of the slots after it | — |
/// | `\c` | the literal `c` | — |
///
/// Any other character is a literal that is inserted automatically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputMask {
    slots: Vec<Slot>,
}

/// Outcome of applying a mask to typed or pasted text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaskedText {
    /// Text to show, with literals inserted.
    pub display: String,
    /// Characters the user supplied, without literals.
    pub value: String,
    /// Every required slot is filled.
    pub complete: bool,
    /// Input characters that fit no slot and were dropped.
    pub rejected: usize,
}

impl InputMask {
    pub fn parse(pattern: &str) -> Result<Self, MaskError> {
        let mut slots = Vec::new();
        let mut chars = pattern.chars();
        let mut case = Case::Keep;
        while let Some(ch) = chars.next() {
            let input = |kind, required| Slot::Input {
                kind,
                required,
                case,
            };
            let slot = match ch {
                '0' => input(SlotKind::Digit, true),
                '9' => input(SlotKind::Digit, false),
                'A' => input(SlotKind::Letter, true),
                'a' => input(SlotKind::Letter, false),
                'N' => input(SlotKind::Alphanumeric, true),
                'n' => input(SlotKind::Alphanumeric, false),
                'X' => input(SlotKind::Any, true),
                'x' => input(SlotKind::Any, false),
                '>' => {
                    case = Case::Upper;
                    continue;
                }
                '<' => {
                    case = Case::Lower;
                    continue;
                }
                '\\' => Slot::Literal(chars.next().ok_or(MaskError::TrailingEscape)?),
                other => Slot::Literal(other),
            };
            slots.push(slot);
        }
        if slots.iter().all(|slot| matches!(slot, Slot::Literal(_))) {
            return Err(MaskError::Empty);
        }
        Ok(Self { slots })
    }

    /// Fits `raw` into the mask. Literals typed by the user are accepted
    /// where the mask expects them; optional slots are skipped when the
    /// next character only fits a later slot.
    pub fn apply(&self, raw: &str) -> MaskedText {
        let mut display = String::new();
        let mut value = String::new();
        let mut rejected = 0;
        let mut slot = 0;
        let mut pending_literals = String::new();
        for ch in raw.chars() {
            // A typed literal matching the next literal slot is consumed.
            if let Some(Slot::Literal(literal)) = self.slots.get(slot)
                && *literal == ch
            {
                display.push_str(&pending_literals);
                pending_literals.clear();
                display.push(ch);
                slot += 1;
                continue;
            }
            let mut probe = slot;
            let mut literals = String::new();
            let placed = loop {
                match self.slots.get(probe) {
                    None => break None,
                    Some(Slot::Literal(literal)) => {
                        literals.push(*literal);
                        probe += 1;
                    }
                    Some(Slot::Input { kind, required, .. }) => {
                        if accepts(*kind, ch) {
                            break Some(probe);
                        }
                        if *required {
                            break None;
                        }
                        probe += 1;
                    }
                }
            };
            match placed {
                Some(index) => {
                    display.push_str(&pending_literals);
                    pending_literals.clear();
                    display.push_str(&literals);
                    let case = match self.slots[index] {
                        Slot::Input { case, .. } => case,
                        Slot::Literal(_) => Case::Keep,
                    };
                    for ch in cased(ch, case) {
                        display.push(ch);
                        value.push(ch);
                    }
                    slot = index + 1;
                }
                None => rejected += 1,
            }
        }
        // Show literals that directly follow the last input (for example
        // the ") " after an area code) so the caret lands after them.
        while let Some(Slot::Literal(literal)) = self.slots.get(slot) {
            if value.is_empty() {
                break;
            }
            display.push(*literal);
            slot += 1;
        }
        let complete = self.slots[slot.min(self.slots.len())..]
            .iter()
            .all(|slot| !matches!(slot, Slot::Input { required: true, .. }));
        MaskedText {
            display,
            value,
            complete,
            rejected,
        }
    }

    /// Placeholder such as `(___) ___-____`.
    pub fn placeholder(&self, fill: char) -> String {
        self.slots
            .iter()
            .map(|slot| match slot {
                Slot::Literal(ch) => *ch,
                Slot::Input { .. } => fill,
            })
            .collect()
    }

    /// Valid when the text fills every required slot, Intermediate while
    /// it does not, and Invalid with [`ValidationError::Format`] when it
    /// has characters that fit no slot.
    pub fn validate(&self, raw: &str) -> ValidationState {
        let masked = self.apply(raw);
        if masked.rejected > 0 {
            ValidationState::Invalid(ValidationError::Format)
        } else if masked.complete {
            ValidationState::Valid
        } else {
            ValidationState::Intermediate
        }
    }
}

/// `ch` in the case of its slot.
fn cased(ch: char, case: Case) -> Vec<char> {
    match case {
        Case::Keep => vec![ch],
        Case::Upper => ch.to_uppercase().collect(),
        Case::Lower => ch.to_lowercase().collect(),
    }
}

fn accepts(kind: SlotKind, ch: char) -> bool {
    match kind {
        SlotKind::Digit => ch.is_ascii_digit(),
        SlotKind::Letter => ch.is_alphabetic(),
        SlotKind::Alphanumeric => ch.is_alphanumeric(),
        SlotKind::Any => !ch.is_control(),
    }
}

/// Locale-aware decimal input with optional bounds and precision.
///
/// The text has digits, the locale's group separator in the whole part,
/// at most one decimal sign when fractions are allowed, and one leading
/// minus when the minimum is negative or absent.
#[derive(Clone, Debug)]
pub struct NumberInput {
    locale: Locale,
    min: Option<f64>,
    max: Option<f64>,
    fraction_digits: usize,
}

impl NumberInput {
    pub fn new(locale: Locale, fraction_digits: usize) -> Self {
        Self {
            locale,
            min: None,
            max: None,
            fraction_digits: fraction_digits.min(15),
        }
    }
    pub fn with_range(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// True when `text` can still become a valid number of this input, so
    /// the field lets the user type it. It rejects `1-2`, `1.2.3`, `--`,
    /// letters, a minus when the minimum is not negative, and more
    /// fraction digits than the input allows.
    pub fn accepts(&self, text: &str) -> bool {
        let negative = self.min.is_none_or(|min| min < 0.0);
        self.shape(text, self.fraction_digits, negative)
    }

    /// True when `text` has the shape of a number with at most
    /// `fraction_digits` fraction digits, and a leading minus only when
    /// `negative` is true.
    fn shape(&self, text: &str, fraction_digits: usize, negative: bool) -> bool {
        let data = self.locale.data();
        let mut decimal = false;
        let mut fraction = 0;
        for (index, ch) in text.chars().enumerate() {
            if ch.is_ascii_digit() {
                fraction += usize::from(decimal);
                if fraction > fraction_digits {
                    return false;
                }
            } else if ch == '-' {
                if index != 0 || !negative {
                    return false;
                }
            } else if ch == data.decimal && fraction_digits > 0 && !decimal {
                decimal = true;
            } else if Some(ch) != data.group || decimal {
                return false;
            }
        }
        true
    }

    /// The filter that lets the user type only what [`accepts`] allows.
    ///
    /// [`accepts`]: Self::accepts
    pub fn filter(&self) -> InputFilter {
        let input = self.clone();
        InputFilter::new().rule(move |text| input.accepts(text))
    }

    /// Validation state while the user types and, when valid, the parsed
    /// value. A value below the minimum is Intermediate, because more
    /// digits can still reach it. [`commit`](Self::commit) makes it
    /// Invalid.
    pub fn validate(&self, text: &str) -> (ValidationState, Option<f64>) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return (ValidationState::Intermediate, None);
        }
        // A negative number below a minimum that is not negative is
        // below the minimum, not malformed.
        if !self.shape(trimmed, self.fraction_digits, true) {
            let error = if !self.shape(trimmed, 15, true) {
                NumberError::NotANumber
            } else if self.fraction_digits == 0 {
                NumberError::NotWhole
            } else {
                NumberError::TooManyDecimals(self.fraction_digits)
            };
            return (ValidationState::Invalid(error.into()), None);
        }
        let Ok(value) = self.locale.parse_decimal(trimmed) else {
            return (ValidationState::Intermediate, None);
        };
        if self.min.is_some_and(|min| value < min) {
            return (ValidationState::Intermediate, None);
        }
        if let Some(max) = self.max
            && value > max
        {
            let error = NumberError::AboveMaximum(max);
            return (ValidationState::Invalid(error.into()), None);
        }
        (ValidationState::Valid, Some(value))
    }

    /// Validation state when the user commits the text with Return or
    /// blur: as [`validate`](Self::validate), but a value below the
    /// minimum is Invalid.
    pub fn commit(&self, text: &str) -> (ValidationState, Option<f64>) {
        let (state, value) = self.validate(text);
        if state == ValidationState::Intermediate
            && let (Some(min), Ok(value)) = (self.min, self.locale.parse_decimal(text.trim()))
            && value < min
        {
            let error = NumberError::BelowMinimum(min);
            return (ValidationState::Invalid(error.into()), None);
        }
        (state, value)
    }

    /// Locale formatting used when committing a value back to the field.
    pub fn format(&self, value: f64) -> String {
        self.locale
            .format_decimal(value, self.fraction_digits)
            .unwrap_or_default()
    }
}

/// Locale-aware short-date input (for example `31/12/2026` in Vietnamese,
/// `12/31/2026` in US English).
#[derive(Clone, Debug)]
pub struct DateInput {
    locale: Locale,
    min: Option<CivilDate>,
    max: Option<CivilDate>,
}

impl DateInput {
    pub fn new(locale: Locale) -> Self {
        Self {
            locale,
            min: None,
            max: None,
        }
    }
    pub fn with_range(mut self, min: Option<CivilDate>, max: Option<CivilDate>) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// The fields of the short format, in the locale's order.
    fn fields(&self) -> [DateField; 3] {
        match self.locale.data().date_order {
            DateOrder::DayMonthYear => [DateField::Day, DateField::Month, DateField::Year],
            DateOrder::MonthDayYear => [DateField::Month, DateField::Day, DateField::Year],
            DateOrder::YearMonthDay => [DateField::Year, DateField::Month, DateField::Day],
        }
    }

    /// True when `text` can still become a date of the locale's short
    /// format, so the field lets the user type it. It allows digits and
    /// the locale's date separator, at most the length of each field, and
    /// rejects a field that no date has, such as month `13` or the year
    /// `1234/` where the day comes first.
    pub fn accepts(&self, text: &str) -> bool {
        let separator = self.locale.data().date_separator;
        if !text
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == separator)
        {
            return false;
        }
        let parts: Vec<&str> = text.split(separator).collect();
        if parts.len() > 3 {
            return false;
        }
        let last = parts.len() - 1;
        parts
            .iter()
            .zip(self.fields())
            .enumerate()
            .all(|(index, (part, field))| field.allows(part, index < last))
    }

    /// The filter that lets the user type only what [`accepts`] allows.
    ///
    /// [`accepts`]: Self::accepts
    pub fn filter(&self) -> InputFilter {
        let input = self.clone();
        InputFilter::new().rule(move |text| input.accepts(text))
    }

    /// Validation state and, when valid, the date. Text that can still
    /// become a date is Intermediate.
    pub fn validate(&self, text: &str) -> (ValidationState, Option<CivilDate>) {
        let trimmed = text.trim();
        let possible = self.accepts(trimmed);
        if possible && let Ok(date) = CivilDate::parse_short(trimmed, &self.locale) {
            if self.min.is_some_and(|min| date < min) || self.max.is_some_and(|max| date > max) {
                let error = ValidationError::Date(DateError::OutOfRange);
                return (ValidationState::Invalid(error), None);
            }
            return (ValidationState::Valid, Some(date));
        }
        let separator = self.locale.data().date_separator;
        let parts: Vec<&str> = trimmed.split(separator).collect();
        let complete = parts.len() == 3
            && parts
                .last()
                .is_some_and(|last| last.len() == self.fields()[2].length());
        if possible && !complete {
            (ValidationState::Intermediate, None)
        } else {
            let error = ValidationError::Date(DateError::NotADate);
            (ValidationState::Invalid(error), None)
        }
    }

    /// The date and its text in the locale's short format, when the user
    /// commits `text` with Return or blur and it is a valid date.
    pub fn commit(&self, text: &str) -> (ValidationState, Option<(CivilDate, String)>) {
        let (state, date) = self.validate(text);
        (state, date.map(|date| (date, self.format(date))))
    }

    pub fn format(&self, date: CivilDate) -> String {
        date.format_short(&self.locale)
    }

    /// Placeholder such as `dd/mm/yyyy` in the locale's field order.
    pub fn placeholder(&self) -> String {
        let data = self.locale.data();
        let s = data.date_separator;
        match data.date_order {
            choscorui_core::DateOrder::DayMonthYear => format!("dd{s}mm{s}yyyy"),
            choscorui_core::DateOrder::MonthDayYear => format!("mm{s}dd{s}yyyy"),
            choscorui_core::DateOrder::YearMonthDay => format!("yyyy{s}mm{s}dd"),
        }
    }
}

/// A field of a short date.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DateField {
    Day,
    Month,
    Year,
}

impl DateField {
    /// The number of digits of the field.
    fn length(self) -> usize {
        match self {
            Self::Day | Self::Month => 2,
            Self::Year => 4,
        }
    }

    /// True when `part` can be this field. A field that a separator ends
    /// is `done`: it needs a value that a date has.
    fn allows(self, part: &str, done: bool) -> bool {
        if part.len() > self.length() || (done && part.is_empty()) {
            return false;
        }
        let value: u32 = part.parse().unwrap_or(0);
        let range = match self {
            Self::Day => 1..=31,
            Self::Month => 1..=12,
            Self::Year => return !done || part.len() == 4,
        };
        // A full or ended field must be in range. A started one must be
        // able to reach the range with one more digit.
        if done || part.len() == self.length() {
            range.contains(&value)
        } else {
            part.is_empty() || value * 10 <= *range.end()
        }
    }
}
