//! View constructors for the range, selection, and text controls. Like
//! the views in `widgets`, each takes its content, a `Binding` where the
//! control edits a value, and an action where it fires one.

use super::configured::Configured;
use super::limits::{Limits, SharedLimits};
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_dates, as_index, as_position, as_text, label, text, to_position};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    CalendarMode, ComboItem, ControlKind, ControlMessage, ControlValue, RangeError,
    SliderOrientation,
};
use choscorui_core::{CivilDate, ItemKey};
use std::cell::Cell;
use std::rc::Rc;

pub(super) fn as_whole(value: &ControlValue) -> Option<i32> {
    match value {
        ControlValue::Number(number) if number.is_finite() => Some(
            number
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        _ => None,
    }
}

pub(super) fn to_number(value: i32) -> ControlValue {
    ControlValue::Number(f64::from(value))
}

/// The limits of a new integer range view: `0..=100`, step 1, page 10.
pub(super) fn percent() -> SharedLimits {
    Rc::new(Cell::new(Limits::new(0.0, 100.0)))
}

/// A slider that edits `value`. It covers `0..=100` with a step of 1 and
/// a page of 10 until `.range`, `.step`, and `.page` change them. A value
/// outside the bounds shows the nearest bound and reports a
/// [`ViewError::Clamped`](super::ViewError::Clamped).
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, Slider, View};
///
/// struct Volume;
///
/// impl View for Volume {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let level = cx.state(40);
///         Slider::new("Volume", level.binding()).range(0..=100).step(1).page(10)
///     }
/// }
/// ```
pub type Slider = Configured<controls::Slider>;

impl Configured<controls::Slider> {
    /// A horizontal slider named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<i32>) -> Self {
        Self::oriented(name, value, SliderOrientation::Horizontal)
    }

    /// A slider whose thumb moves up and down.
    pub fn vertical<M>(name: impl IntoLive<TextContent, M>, value: Binding<i32>) -> Self {
        Self::oriented(name, value, SliderOrientation::Vertical)
    }

    fn oriented<M>(
        name: impl IntoLive<TextContent, M>,
        value: Binding<i32>,
        orientation: SliderOrientation,
    ) -> Self {
        let name = label(name);
        let limits = percent();
        let read = limits.clone();
        Configured::try_custom(ControlKind::Slider, move |env| {
            let range = read.get().integers(0)?;
            Ok::<_, RangeError>(controls::Slider::new(text(&name, env), range, orientation))
        })
        .limited(limits)
        .bind(value, to_number, as_whole)
    }
}

/// An integer field with steppers that edits `value`. Like [`Slider`], it
/// covers `0..=100` with a step of 1 and a page of 10 until `.range`,
/// `.step`, and `.page` change them.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, SpinBox, View};
///
/// struct Copies;
///
/// impl View for Copies {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let copies = cx.state(1);
///         SpinBox::new("Copies", copies.binding()).range(1..=99)
///     }
/// }
/// ```
pub type SpinBox = Configured<controls::SpinBox>;

impl Configured<controls::SpinBox> {
    /// A spin box named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<i32>) -> Self {
        let name = label(name);
        let limits = percent();
        let read = limits.clone();
        Configured::try_custom(ControlKind::Input, move |env| {
            let range = read.get().integers(0)?;
            Ok::<_, RangeError>(controls::SpinBox::new(text(&name, env), range))
        })
        .limited(limits)
        .bind(value, to_number, as_whole)
    }
}

/// A progress bar that shows `value` of a maximum, 100 until `.maximum`
/// changes it. A value above the maximum shows a full bar and reports a
/// [`ViewError::Clamped`](super::ViewError::Clamped).
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Progress, View};
///
/// fn upload(sent: u32) -> impl View {
///     Progress::new("Upload", sent).maximum(10)
/// }
/// ```
pub type Progress = Configured<controls::Progress>;

impl Configured<controls::Progress> {
    /// A progress bar named `name` that shows `value`.
    pub fn new<M, N>(name: impl IntoLive<TextContent, M>, value: impl IntoLive<u32, N>) -> Self {
        let name = label(name);
        let limits = percent();
        let read = limits.clone();
        Configured::try_custom(ControlKind::Progress, move |env| {
            let maximum = read.get().maximum as u32;
            controls::Progress::new(text(&name, env), 0, maximum)
        })
        .limited(limits)
        .value(
            value
                .into_live()
                .map(|value| ControlValue::Number(f64::from(value))),
        )
    }

    /// The value of a full bar. A maximum of zero becomes 1 and reports a
    /// [`ViewError::Clamped`](super::ViewError::Clamped).
    #[must_use]
    pub fn maximum(self, maximum: u32) -> Self {
        self.change_limits(|limits| {
            limits.maximum = f64::from(maximum.max(1));
            (maximum == 0).then(|| "the maximum 0 is not positive; the view uses 1".to_owned())
        })
    }

    /// A progress bar for work of unknown length.
    pub fn indeterminate<M>(name: impl IntoLive<TextContent, M>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Progress, move |env| {
            controls::Progress::indeterminate(text(&name, env))
        })
    }
}

/// A single-line text field that edits `value`. Enter runs the
/// `on_activate` action.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Search;
///
/// impl View for Search {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let query = cx.state(String::new());
///         TextField::new("Search", query.binding())
///     }
/// }
/// ```
pub type TextField = Configured<controls::TextField>;

impl Configured<controls::TextField> {
    /// A text field named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::TextField::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// Radio buttons, one for each option; `selected` edits the chosen
/// option. An empty list reports a build error.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Size;
///
/// impl View for Size {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let size = cx.state(0);
///         let options = vec!["Small".to_owned(), "Large".to_owned()];
///         RadioGroup::new("Size", options, size.binding())
///     }
/// }
/// ```
pub type RadioGroup = Configured<controls::HostedRadioGroup>;

impl Configured<controls::HostedRadioGroup> {
    /// A radio group named `name` with one button for each option.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        options: impl Items<String>,
        selected: Binding<usize>,
    ) -> Self {
        let name = label(name);
        Configured::try_custom(ControlKind::RadioGroup, move |env| {
            controls::RadioGroup::hosted(text(&name, env), options.items(env), 0)
        })
        .bind(selected, to_position, as_position)
    }
}

/// A tab list, one tab for each label; `selected` edits the chosen tab.
/// Show the chosen tab's content next to it. An empty list reports a
/// build error.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Pages;
///
/// impl View for Pages {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let tab = cx.state(0);
///         let tabs = vec!["General".to_owned(), "Advanced".to_owned()];
///         Tabs::new("Settings", tabs, tab.binding())
///     }
/// }
/// ```
pub type Tabs = Configured<controls::HostedTabs>;

impl Configured<controls::HostedTabs> {
    /// A tab list named `name` with one tab for each label.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        tabs: impl Items<String>,
        selected: Binding<usize>,
    ) -> Self {
        let name = label(name);
        Configured::try_custom(ControlKind::Tabs, move |env| {
            controls::Tabs::hosted(text(&name, env), tabs.items(env), 0)
        })
        .bind(selected, to_position, as_position)
    }
}

fn combo_items(items: Vec<String>) -> Vec<ComboItem> {
    items
        .into_iter()
        .enumerate()
        .map(|(index, text)| ComboItem {
            key: ItemKey(index as u64),
            text,
        })
        .collect()
}

/// A field with a list of suggestions; `selected` edits the chosen item.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Country;
///
/// impl View for Country {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selected = cx.state(None);
///         let countries = vec!["France".to_owned(), "Japan".to_owned()];
///         Combobox::new("Country", countries, selected.binding())
///     }
/// }
/// ```
pub type Combobox = Configured<controls::Combobox>;

impl Configured<controls::Combobox> {
    /// A combobox named `name` with `items`; `selected` edits the chosen item.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<String>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Combobox, move |env| {
            let mut combobox = controls::Combobox::new(text(&name, env), false);
            combobox.set_items(combo_items(items.items(env)));
            combobox
        })
        .bind(selected, ControlValue::Index, as_index)
    }

    /// A combobox that also takes free text; `value` edits the text.
    pub fn editable<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<String>,
        value: Binding<String>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Combobox, move |env| {
            let mut combobox = controls::Combobox::new(text(&name, env), true);
            combobox.set_items(combo_items(items.items(env)));
            combobox
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A multiline plain-text editor that edits `value`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Editor;
///
/// impl View for Editor {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let text = cx.state(String::new());
///         TextEditor::new("Document", text.binding())
///     }
/// }
/// ```
pub type TextEditor = Configured<controls::TextEditor>;

impl Configured<controls::TextEditor> {
    /// A text editor named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::TextEditor, move |env| {
            controls::TextEditor::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A month grid in the environment's locale; `selected` edits the chosen
/// day.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::CivilDate;
/// struct Agenda;
///
/// impl View for Agenda {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let today = CivilDate::new(2026, 10, 13).expect("a valid date");
///         let day = cx.state(today);
///         Calendar::new(day.binding())
///     }
/// }
/// ```
pub type Calendar = Configured<controls::Calendar>;

impl Configured<controls::Calendar> {
    /// A calendar; `selected` edits the chosen day.
    pub fn new(selected: Binding<CivilDate>) -> Self {
        Configured::try_custom(ControlKind::Calendar, move |env| {
            let day = Runtime::untracked(|| selected.try_get())?;
            Ok::<_, crate::reactive::ReactiveError>(controls::Calendar::new(env.locale(), day))
        })
        .value(move || ControlValue::Date(selected.try_get().ok()))
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Date(Some(day))) = message {
                selected.set(*day);
            }
        })
    }

    /// A calendar that chooses several days, or a range with
    /// `CalendarMode::Range`; `dates` edits them. It first shows
    /// `month`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::CalendarMode;
    /// # use choscorui_core::CivilDate;
    /// struct Trip;
    ///
    /// impl View for Trip {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let month = CivilDate::new(2026, 10, 1).expect("a valid date");
    ///         let days = cx.state(Vec::new());
    ///         Calendar::dates(month, CalendarMode::Range, days.binding())
    ///     }
    /// }
    /// ```
    pub fn dates(month: CivilDate, mode: CalendarMode, dates: Binding<Vec<CivilDate>>) -> Self {
        Configured::custom(ControlKind::Calendar, move |env| {
            controls::Calendar::new(env.locale(), month).with_mode(mode)
        })
        .bind(dates, ControlValue::Dates, as_dates)
    }
}
