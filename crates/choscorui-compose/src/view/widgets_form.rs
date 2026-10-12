//! View constructors for labels, structured inputs, and value pickers:
//! labels, input groups, masked inputs, decimal spin boxes, date and time
//! editors, key sequence editors, color pickers, dials, and numeric
//! displays. Like the views in `widgets`, each takes its content, a
//! `Binding` where the control edits a value, and an action where it
//! fires one.

use super::builtins::{as_whole, to_number};
use super::configured::Configured;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_text, label, text};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    ControlKind, ControlMessage, ControlValue, DateTimeKind, InputGroupAddon, InputMask,
    IntegerRange, NumberBase,
};
use choscorui_core::{CivilDate, CivilDateTime, CivilTime, Color};

fn as_number(value: &ControlValue) -> Option<f64> {
    match value {
        ControlValue::Number(number) if number.is_finite() => Some(*number),
        _ => None,
    }
}

/// Static text with the label recipe, for a caption next to a control.
pub type Label = Configured<controls::Label>;

impl Configured<controls::Label> {
    pub fn new<M>(content: impl IntoLive<TextContent, M>) -> Self {
        let content = label(content);
        Configured::custom(ControlKind::Label, move |env| {
            controls::Label::hosted(text(&content, env))
        })
    }
}

/// A text input with addons: text, icons, and buttons before, after,
/// above, or below the text. `value` edits the text. A button addon
/// reports its index through `.on_item`.
pub type InputGroup = Configured<controls::InputGroup>;

impl Configured<controls::InputGroup> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        addons: impl Items<InputGroupAddon>,
        value: Binding<String>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::InputGroup, move |env| {
            let field = controls::TextField::new(text(&name, env), "");
            controls::InputGroup::new(field, addons.items(env))
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

impl Configured<controls::Input> {
    /// An input that fits what the user types into `mask` and shows its
    /// literals. `raw` edits the characters that the user typed, without
    /// the literals. A deletion at the end removes typed characters until
    /// the shown text is no longer than what is left, so a mask does not
    /// put a deleted literal back. Check the value with
    /// [`InputMask::validate`].
    pub fn masked<M>(
        name: impl IntoLive<TextContent, M>,
        mask: InputMask,
        raw: Binding<String>,
    ) -> Self {
        let name = label(name);
        let placeholder = mask.placeholder('_');
        let shown = mask.clone();
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::new(text(&name, env), "").with_placeholder(placeholder.clone())
        })
        .value(move || ControlValue::Text(shown.apply(&raw.get()).display))
        .on_message(move |message| {
            let ControlMessage::Changed(ControlValue::Text(typed)) = message else {
                return;
            };
            let Ok(mut next) = raw.try_get() else {
                return;
            };
            let before = mask.apply(&next).display;
            if typed.len() < before.len() && before.starts_with(typed.as_str()) {
                while mask.apply(&next).display.len() > typed.len() && next.pop().is_some() {}
            } else {
                next = mask.apply(typed).value;
            }
            raw.set(next);
        })
    }
}

/// A locale-aware decimal field with steppers that edits `value` inside
/// `bounds`. It shows `fraction_digits` digits after the decimal sign,
/// and Return commits only a valid value.
pub type DecimalSpinBox = Configured<controls::DecimalSpinBox>;

impl Configured<controls::DecimalSpinBox> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        bounds: std::ops::RangeInclusive<f64>,
        step: f64,
        fraction_digits: usize,
        value: Binding<f64>,
    ) -> Self {
        let name = label(name);
        let limits = (*bounds.start(), *bounds.end());
        Configured::custom(ControlKind::Input, move |env| {
            let start = limits.0;
            controls::DecimalSpinBox::new(
                text(&name, env),
                env.locale(),
                limits,
                step,
                fraction_digits,
                start,
            )
        })
        .bind(value, ControlValue::Number, as_number)
    }
}

/// The value that a date and time editor of `kind` takes and reports.
fn to_date_time(kind: DateTimeKind, value: CivilDateTime) -> ControlValue {
    let time = format!(
        "{:02}:{:02}:{:02}",
        value.time.hour(),
        value.time.minute(),
        value.time.second()
    );
    match kind {
        DateTimeKind::Date => ControlValue::Date(Some(value.date)),
        DateTimeKind::Time => ControlValue::Text(time),
        DateTimeKind::DateTime => ControlValue::Text(format!("{}T{time}", value.date.iso())),
    }
}

/// The value in a message of a date and time editor of `kind`. A date or
/// a time alone keeps the other part of `current`.
fn from_date_time(
    kind: DateTimeKind,
    current: CivilDateTime,
    value: &ControlValue,
) -> Option<CivilDateTime> {
    match (kind, value) {
        (DateTimeKind::Date, ControlValue::Date(Some(date))) => Some(CivilDateTime {
            date: *date,
            time: current.time,
        }),
        (DateTimeKind::Time, ControlValue::Text(text)) => Some(CivilDateTime {
            date: current.date,
            time: CivilTime::parse(text).ok()?,
        }),
        (DateTimeKind::DateTime, ControlValue::Text(text)) => {
            let (date, time) = text.split_once('T')?;
            Some(CivilDateTime {
                date: CivilDate::parse_iso(date).ok()?,
                time: CivilTime::parse(time).ok()?,
            })
        }
        _ => None,
    }
}

/// A date, time, or date and time editor with one field for each part,
/// in the order of the environment's locale. `value` edits the value; a
/// date or time editor keeps the other part of it. Use `.with` and
/// `DateTimeEdit::set_range` to limit the value.
pub type DateTimeEdit = Configured<controls::DateTimeEdit>;

impl Configured<controls::DateTimeEdit> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        kind: DateTimeKind,
        value: Binding<CivilDateTime>,
    ) -> Self {
        let name = label(name);
        Configured::try_custom(ControlKind::Input, move |env| {
            // The view gives each new editor the bound value, so the build
            // does not depend on it.
            let start = Runtime::untracked(|| value.try_get())?;
            Ok::<_, crate::ReactiveError>(controls::DateTimeEdit::new(
                text(&name, env),
                env.locale(),
                kind,
                start,
            ))
        })
        .value(move || {
            // An empty text fits no editor, so a disposed binding changes
            // nothing.
            value
                .try_get()
                .map_or(ControlValue::Text(String::new()), |current| {
                    to_date_time(kind, current)
                })
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(changed) = message
                && let Ok(current) = value.try_get()
                && let Some(next) = from_date_time(kind, current, changed)
            {
                value.set(next);
            }
        })
    }
}

/// A field that records a keyboard shortcut of up to `max_chords` chords.
/// `sequence` edits it as text, for example `⌘K, ⇧⌘S`; empty text is no
/// shortcut.
pub type KeySequenceEdit = Configured<controls::KeySequenceEdit>;

impl Configured<controls::KeySequenceEdit> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        max_chords: usize,
        sequence: Binding<String>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::KeySequenceEdit::new(max_chords).with_name(text(&name, env))
        })
        .bind(sequence, ControlValue::Text, as_text)
    }
}

/// A color picker: a swatch and hue, saturation, value, and alpha
/// strips. `hex` edits the color as `#RRGGBB` text, or `#RRGGBBAA` when
/// the color is not opaque; the picker takes `#RGB` too. Accepting the
/// color runs `.on_activate`.
pub type ColorPicker = Configured<controls::ColorPicker>;

impl Configured<controls::ColorPicker> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, hex: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ColorPicker, move |env| {
            controls::ColorPicker::new(Color::BLACK).with_name(text(&name, env))
        })
        .bind(hex, ControlValue::Text, as_text)
    }
}

/// A rotary control over `range` that edits `value`. A wrapping dial,
/// such as a compass, turns past its maximum to its minimum.
pub type Dial = Configured<controls::HostedDial>;

impl Configured<controls::HostedDial> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        range: IntegerRange,
        wrapping: bool,
        value: Binding<i32>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Dial, move |env| {
            controls::Dial::new(range, wrapping).hosted(text(&name, env))
        })
        .bind(value, to_number, as_whole)
    }
}

/// A seven-segment display of `value` in `base`, with `digits` digits.
/// It takes no input; a value that does not fit shows dashes.
pub type NumericDisplay = Configured<controls::HostedNumericDisplay>;

impl Configured<controls::HostedNumericDisplay> {
    pub fn new<M, N, O>(
        name: impl IntoLive<TextContent, M>,
        digits: usize,
        base: impl IntoLive<NumberBase, N>,
        value: impl IntoLive<i64, O>,
    ) -> Self {
        let name = label(name);
        let base = base.into_live();
        Configured::custom(ControlKind::NumericDisplay, move |env| {
            controls::NumericDisplay::new(digits, base.get()).hosted(text(&name, env))
        })
        .value(
            value
                .into_live()
                .map(|value| ControlValue::Number(value as f64)),
        )
    }
}
