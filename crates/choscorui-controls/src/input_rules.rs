//! Input masks and locale-aware numeric and date input contracts. Each
//! validator maps raw text to a [`ValidationState`]: `Intermediate` while
//! the user may still be typing toward a valid value, `Invalid` with a
//! message once no continuation can be valid, and `Valid` with a parsed
//! value. Commit behavior stays with the field (Return commits only when
//! the state is `Valid`).

use super::ValidationState;
use choscorui_core::{CivilDate, Locale};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotKind {
    Digit,
    Letter,
    Alphanumeric,
    Any,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Slot {
    Input { kind: SlotKind, required: bool },
    Literal(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaskError {
    Empty,
    TrailingEscape,
}

/// A fixed-format mask such as `(000) 000-0000` or `AA-9999`.
///
/// | Symbol | Accepts | Required |
/// | --- | --- | --- |
/// | `0` / `9` | digit | yes / no |
/// | `A` / `a` | letter | yes / no |
/// | `N` / `n` | letter or digit | yes / no |
/// | `X` / `x` | any character | yes / no |
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
        while let Some(ch) = chars.next() {
            let slot = match ch {
                '0' => input(SlotKind::Digit, true),
                '9' => input(SlotKind::Digit, false),
                'A' => input(SlotKind::Letter, true),
                'a' => input(SlotKind::Letter, false),
                'N' => input(SlotKind::Alphanumeric, true),
                'n' => input(SlotKind::Alphanumeric, false),
                'X' => input(SlotKind::Any, true),
                'x' => input(SlotKind::Any, false),
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
                    Some(Slot::Input { kind, required }) => {
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
                    display.push(ch);
                    value.push(ch);
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

    pub fn validate(&self, raw: &str) -> ValidationState {
        let masked = self.apply(raw);
        if masked.rejected > 0 {
            ValidationState::Invalid("Contains characters this format does not allow".into())
        } else if masked.complete {
            ValidationState::Valid
        } else {
            ValidationState::Intermediate
        }
    }
}

fn input(kind: SlotKind, required: bool) -> Slot {
    Slot::Input { kind, required }
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

    /// Validation state and, when valid, the parsed value.
    pub fn validate(&self, text: &str) -> (ValidationState, Option<f64>) {
        let data = self.locale.data();
        let trimmed = text.trim();
        let partial = trimmed.is_empty()
            || trimmed.chars().all(|c| {
                c.is_ascii_digit()
                    || c == data.decimal
                    || Some(c) == data.group
                    || c == '-'
                    || c == '.'
            });
        let Ok(value) = self.locale.parse_decimal(trimmed) else {
            return if partial {
                (ValidationState::Intermediate, None)
            } else {
                (ValidationState::Invalid("Enter a number".into()), None)
            };
        };
        let digits = trimmed
            .rsplit_once(data.decimal)
            .map_or(0, |(_, fraction)| {
                fraction.chars().filter(char::is_ascii_digit).count()
            });
        if digits > self.fraction_digits {
            let message = format!("Use at most {} decimal places", self.fraction_digits);
            return (ValidationState::Invalid(message), None);
        }
        if let Some(min) = self.min
            && value < min
        {
            let bound = self.format(min);
            // Typing "1" toward a minimum of 10 is still in progress: more
            // whole digits can still reach the bound.
            let state = if value >= 0.0 && min > 0.0 && !trimmed.contains(data.decimal) {
                ValidationState::Intermediate
            } else {
                ValidationState::Invalid(format!("Must be at least {bound}"))
            };
            return (state, None);
        }
        if let Some(max) = self.max
            && value > max
        {
            return (
                ValidationState::Invalid(format!("Must be at most {}", self.format(max))),
                None,
            );
        }
        (ValidationState::Valid, Some(value))
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

    pub fn validate(&self, text: &str) -> (ValidationState, Option<CivilDate>) {
        let separator = self.locale.data().date_separator;
        let trimmed = text.trim();
        if let Ok(date) = CivilDate::parse_short(trimmed, &self.locale) {
            if self.min.is_some_and(|min| date < min) || self.max.is_some_and(|max| date > max) {
                return (
                    ValidationState::Invalid("Date is outside the allowed range".into()),
                    None,
                );
            }
            return (ValidationState::Valid, Some(date));
        }
        let fields: Vec<&str> = trimmed.split([separator, '/', '.', '-']).collect();
        let shaped_like_date = trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == separator || matches!(c, '/' | '.' | '-'))
            && fields.len() <= 3
            && fields.iter().all(|field| field.len() <= 4);
        let incomplete = fields.len() < 3 || fields.last().is_some_and(|year| year.len() < 4);
        if trimmed.is_empty() || (shaped_like_date && incomplete) {
            (ValidationState::Intermediate, None)
        } else {
            (ValidationState::Invalid("Enter a valid date".into()), None)
        }
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
