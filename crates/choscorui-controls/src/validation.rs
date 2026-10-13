//! Typed validation errors. A validator returns a [`ValidationError`],
//! and the view layer shows it with [`ValidationError::message`] in the
//! window locale, from the built-in `choscorui.validation.*` messages.

use choscorui_core::{Locale, Localizer};

/// Why number text is not valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberError {
    /// The text is not a number.
    NotANumber,
    /// The text has a fraction where only whole numbers are allowed.
    NotWhole,
    /// The text has more digits after the decimal sign than this.
    TooManyDecimals(usize),
    /// The value is less than this minimum.
    BelowMinimum(f64),
    /// The value is more than this maximum.
    AboveMaximum(f64),
    /// The value is not from `min` to `max`.
    OutOfRange { min: f64, max: f64 },
}

/// Why date text is not valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateError {
    /// The text is not a date of the locale's short format.
    NotADate,
    /// The date is outside the allowed range.
    OutOfRange,
}

/// Why a value is not valid.
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationError {
    Number(NumberError),
    Date(DateError),
    /// The text has characters that the input mask does not allow.
    Format,
    /// The text is none of the options.
    Choice,
    /// The text is not a hex color.
    Color,
    /// The text is not yes or no.
    YesNo,
    /// The text is not an email address.
    Email,
    /// The text is not a web address.
    Url,
    /// The text is not a phone number.
    Tel,
    /// The model does not let the value change.
    ReadOnly,
    /// The row of the value is gone from the model.
    MissingRow,
    /// The model refused the value.
    Rejected,
    /// A message of the application, shown as it is.
    Message(String),
}

impl ValidationError {
    /// The message that tells the user what is wrong, in the locale of
    /// `strings`.
    pub fn message(&self, strings: &Localizer) -> String {
        let locale = strings.locale();
        let text = |id: &str, args: &[(&str, &str)]| strings.message(id, args);
        match self {
            Self::Number(error) => match *error {
                NumberError::NotANumber => text("choscorui.validation.number", &[]),
                NumberError::NotWhole => text("choscorui.validation.whole_number", &[]),
                NumberError::TooManyDecimals(digits) => strings
                    .plural("choscorui.validation.decimals", digits as u64, &[])
                    .unwrap_or_else(|_| "choscorui.validation.decimals".to_owned()),
                NumberError::BelowMinimum(min) => text(
                    "choscorui.validation.minimum",
                    &[("min", &format_number(locale, min))],
                ),
                NumberError::AboveMaximum(max) => text(
                    "choscorui.validation.maximum",
                    &[("max", &format_number(locale, max))],
                ),
                NumberError::OutOfRange { min, max } => text(
                    "choscorui.validation.range",
                    &[
                        ("min", &format_number(locale, min)),
                        ("max", &format_number(locale, max)),
                    ],
                ),
            },
            Self::Date(DateError::NotADate) => text("choscorui.validation.date", &[]),
            Self::Date(DateError::OutOfRange) => text("choscorui.validation.date_range", &[]),
            Self::Format => text("choscorui.validation.format", &[]),
            Self::Choice => text("choscorui.validation.choice", &[]),
            Self::Color => text("choscorui.validation.color", &[("example", "#1E90FF")]),
            Self::YesNo => text("choscorui.validation.yes_no", &[]),
            Self::Email => text(
                "choscorui.validation.email",
                &[("example", "name@example.com")],
            ),
            Self::Url => text(
                "choscorui.validation.url",
                &[("example", "https://example.com")],
            ),
            Self::Tel => text("choscorui.validation.tel", &[]),
            Self::ReadOnly => text("choscorui.validation.read_only", &[]),
            Self::MissingRow => text("choscorui.validation.missing_row", &[]),
            Self::Rejected => text("choscorui.validation.rejected", &[]),
            Self::Message(message) => message.clone(),
        }
    }
}

impl From<NumberError> for ValidationError {
    fn from(error: NumberError) -> Self {
        Self::Number(error)
    }
}

impl From<DateError> for ValidationError {
    fn from(error: DateError) -> Self {
        Self::Date(error)
    }
}

impl From<String> for ValidationError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for ValidationError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

/// `value` in the locale, with the fewest fraction digits, up to six,
/// that show it.
pub(crate) fn format_number(locale: &Locale, value: f64) -> String {
    let digits = (0..6)
        .find(|digits| {
            let scale = 10f64.powi(*digits as i32);
            ((value * scale).round() / scale - value).abs() <= f64::EPSILON * value.abs().max(1.0)
        })
        .unwrap_or(6);
    locale
        .format_decimal(value, digits)
        .unwrap_or_else(|_| value.to_string())
}
