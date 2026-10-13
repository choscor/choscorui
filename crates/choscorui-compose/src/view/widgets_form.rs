//! View constructors for labels, structured inputs, and value pickers:
//! labels, input groups, masked inputs, decimal spin boxes, date and time
//! editors, key sequence editors, color pickers, dials, and numeric
//! displays. Like the views in `widgets`, each takes its content, a
//! `Binding` where the control edits a value, and an action where it
//! fires one.

use super::builtins::{as_whole, percent, to_number};
use super::configured::Configured;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_text, label, text};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    ControlKind, ControlMessage, ControlValue, DateTimeKind, InputGroupAddon, InputMask,
    NumberBase, RangeError,
};
use choscorui_core::{CivilDate, CivilDateTime, CivilTime, Color};

pub(super) fn as_number(value: &ControlValue) -> Option<f64> {
    match value {
        ControlValue::Number(number) if number.is_finite() => Some(*number),
        _ => None,
    }
}

/// Static text with the label recipe, for a caption next to a control.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let caption = Label::new("Email");
/// ```
pub type Label = Configured<controls::Label>;

impl Configured<controls::Label> {
    /// A label that shows `content`.
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
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{AddonAlign, InputGroupAddon};
/// # use choscorui_core::Icon;
/// struct Search;
///
/// impl View for Search {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let query = cx.state(String::new());
///         let addons = vec![InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search)];
///         InputGroup::new("Search", addons, query.binding())
///     }
/// }
/// ```
pub type InputGroup = Configured<controls::InputGroup>;

impl Configured<controls::InputGroup> {
    /// An input group named `name` with `addons` that edits `value`.
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
    /// literals. `value` edits the text with the literals, for example
    /// `(090) 512-3456`. A key that fits no slot changes nothing and
    /// beeps. A paste keeps the characters that fit. The caret keeps its
    /// place among the typed characters, and a deletion of a literal
    /// deletes the typed character before it. Check the value with
    /// [`InputMask::validate`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::InputMask;
    /// struct Phone;
    ///
    /// impl View for Phone {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let phone = cx.state(String::new());
    ///         let mask = InputMask::parse("(000) 000-0000").expect("a valid mask");
    ///         Input::masked("Phone", mask, phone.binding())
    ///     }
    /// }
    /// ```
    pub fn masked<M>(
        name: impl IntoLive<TextContent, M>,
        mask: InputMask,
        value: Binding<String>,
    ) -> Self {
        let name = label(name);
        let placeholder = mask.placeholder('_');
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::new(text(&name, env), "")
                .with_placeholder(placeholder.clone())
                .masked(mask.clone())
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A locale-aware decimal field with steppers that edits `value` inside
/// `bounds`. It shows `fraction_digits` digits after the decimal sign,
/// and Return commits only a valid value.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Price;
///
/// impl View for Price {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let price = cx.state(9.99);
///         DecimalSpinBox::new("Price", 0.0..=1000.0, 0.01, 2, price.binding())
///     }
/// }
/// ```
pub type DecimalSpinBox = Configured<controls::DecimalSpinBox>;

impl Configured<controls::DecimalSpinBox> {
    /// A decimal field named `name` that edits `value` inside `bounds`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        bounds: std::ops::RangeInclusive<f64>,
        step: f64,
        fraction_digits: usize,
        value: Binding<f64>,
    ) -> Self {
        let name = label(name);
        let limits = (*bounds.start(), *bounds.end());
        Configured::try_custom(ControlKind::Input, move |env| {
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
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::DateTimeKind;
/// # use choscorui_core::{CivilDate, CivilDateTime, CivilTime};
/// struct Meeting;
///
/// impl View for Meeting {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let date = CivilDate::new(2026, 10, 13).expect("a valid date");
///         let time = CivilTime::new(9, 30, 0).expect("a valid time");
///         let start = cx.state(CivilDateTime { date, time });
///         DateTimeEdit::new("Start", DateTimeKind::DateTime, start.binding())
///     }
/// }
/// ```
pub type DateTimeEdit = Configured<controls::DateTimeEdit>;

impl Configured<controls::DateTimeEdit> {
    /// An editor named `name` for a `kind` value that edits `value`.
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
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Shortcut;
///
/// impl View for Shortcut {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let keys = cx.state(String::new());
///         KeySequenceEdit::new("Shortcut", 2, keys.binding())
///     }
/// }
/// ```
pub type KeySequenceEdit = Configured<controls::KeySequenceEdit>;

impl Configured<controls::KeySequenceEdit> {
    /// A shortcut field named `name` of up to `max_chords` chords that edits `sequence`.
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

    /// The text that an empty field shows, for example
    /// `t!("shortcut.placeholder")`.
    #[must_use]
    pub fn placeholder<M>(self, placeholder: impl IntoLive<TextContent, M>) -> Self {
        let placeholder = label(placeholder);
        self.with_env(move |edit, env| edit.with_placeholder(text(&placeholder, env)))
    }

    /// Shows a clear button at the end of the field while a shortcut is
    /// set.
    #[must_use]
    pub fn clear_button(self) -> Self {
        self.with(|edit| edit.with_clear_button(true))
    }
}

/// A color picker: a swatch and hue, saturation, value, and alpha
/// strips. `hex` edits the color as `#RRGGBB` text, or `#RRGGBBAA` when
/// the color is not opaque; the picker takes `#RGB` too. Accepting the
/// color runs `.on_activate`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Accent;
///
/// impl View for Accent {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let hex = cx.state("#3366FF".to_owned());
///         ColorPicker::new("Accent color", hex.binding())
///     }
/// }
/// ```
pub type ColorPicker = Configured<controls::ColorPicker>;

impl Configured<controls::ColorPicker> {
    /// A color picker named `name` that edits `hex`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, hex: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ColorPicker, move |env| {
            controls::ColorPicker::new(Color::BLACK).with_name(text(&name, env))
        })
        .bind(hex, ControlValue::Text, as_text)
    }
}

/// A rotary control that edits `value`. Like a slider, it covers
/// `0..=100` with a step of 1 and a page of 10 until `.range`, `.step`,
/// and `.page` change them. A `.wrapping(true)` dial, such as a compass,
/// turns past its maximum to its minimum.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, Dial, View};
///
/// struct Heading;
///
/// impl View for Heading {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let degrees = cx.state(90);
///         Dial::new("Heading", degrees.binding()).range(0..=359).wrapping(true)
///     }
/// }
/// ```
pub type Dial = Configured<controls::HostedDial>;

impl Configured<controls::HostedDial> {
    /// A dial named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<i32>) -> Self {
        let name = label(name);
        let limits = percent();
        let read = limits.clone();
        Configured::try_custom(ControlKind::Dial, move |env| {
            let limits = read.get();
            let range = limits.integers(0)?;
            Ok::<_, RangeError>(
                controls::Dial::new(range, limits.wrapping).hosted(text(&name, env)),
            )
        })
        .limited(limits)
        .bind(value, to_number, as_whole)
    }

    /// Turns past the maximum to the minimum, and back.
    #[must_use]
    pub fn wrapping(self, wrapping: bool) -> Self {
        self.set_wrapping(wrapping)
    }
}

/// A seven-segment display of `value` in `base`, with `digits` digits.
/// It takes no input; a value that does not fit shows dashes.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::NumberBase;
/// let counter = NumericDisplay::new("Counter", 4, NumberBase::Decimal, 42);
/// ```
pub type NumericDisplay = Configured<controls::HostedNumericDisplay>;

impl Configured<controls::HostedNumericDisplay> {
    /// A display named `name` of `value` with `digits` digits in `base`.
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
        .value(value.into_live().map(exact))
    }

    /// A display that is also a counter: a tab stop that edits `value`.
    /// ArrowUp and ArrowRight add 1, ArrowDown and ArrowLeft subtract 1,
    /// PageUp multiplies by 10, and PageDown divides by 10.
    pub fn counter<M, N>(
        name: impl IntoLive<TextContent, M>,
        digits: usize,
        base: impl IntoLive<NumberBase, N>,
        value: Binding<i64>,
    ) -> Self {
        let name = label(name);
        let base = base.into_live();
        Configured::custom(ControlKind::NumericDisplay, move |env| {
            controls::NumericDisplay::new(digits, base.get())
                .hosted(text(&name, env))
                .counter()
        })
        .bind(value, exact, |value| match value {
            ControlValue::Text(text) => text.parse().ok(),
            _ => None,
        })
    }
}

/// An `i64` as a `Text` value, which keeps each digit.
fn exact(value: i64) -> ControlValue {
    ControlValue::Text(value.to_string())
}
