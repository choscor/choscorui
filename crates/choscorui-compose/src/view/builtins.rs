//! View constructors for the range, selection, and text controls. Like
//! the views in `widgets`, each takes its content, a `Binding` where the
//! control edits a value, and an action where it fires one.

use super::configured::Configured;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_index, as_position, as_text, label, text, to_position};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    ComboItem, ControlKind, ControlMessage, ControlValue, IntegerRange, SliderOrientation,
};
use choscorui_core::{CivilDate, ItemKey};

fn as_whole(value: &ControlValue) -> Option<i32> {
    match value {
        ControlValue::Number(number) if number.is_finite() => Some(
            number
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        _ => None,
    }
}

fn to_number(value: i32) -> ControlValue {
    ControlValue::Number(f64::from(value))
}

/// A slider over `range` that edits `value`.
pub type Slider = Configured<controls::Slider>;

impl Configured<controls::Slider> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        range: IntegerRange,
        value: Binding<i32>,
    ) -> Self {
        Self::oriented(name, range, value, SliderOrientation::Horizontal)
    }

    /// A slider whose thumb moves up and down.
    pub fn vertical<M>(
        name: impl IntoLive<TextContent, M>,
        range: IntegerRange,
        value: Binding<i32>,
    ) -> Self {
        Self::oriented(name, range, value, SliderOrientation::Vertical)
    }

    fn oriented<M>(
        name: impl IntoLive<TextContent, M>,
        range: IntegerRange,
        value: Binding<i32>,
        orientation: SliderOrientation,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Slider, move |env| {
            controls::Slider::new(text(&name, env), range, orientation)
        })
        .bind(value, to_number, as_whole)
    }
}

/// An integer field with steppers that edits `value` inside `range`.
pub type SpinBox = Configured<controls::SpinBox>;

impl Configured<controls::SpinBox> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        range: IntegerRange,
        value: Binding<i32>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::SpinBox::new(text(&name, env), range)
        })
        .bind(value, to_number, as_whole)
    }
}

/// A progress bar that shows `value` of `maximum`. A `maximum` of zero
/// reports a build error.
pub type Progress = Configured<controls::Progress>;

impl Configured<controls::Progress> {
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        value: impl IntoLive<u32, N>,
        maximum: u32,
    ) -> Self {
        let name = label(name);
        let value = value.into_live();
        Configured::try_custom(ControlKind::Progress, move |env| {
            controls::Progress::new(text(&name, env), value.get(), maximum)
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
pub type TextField = Configured<controls::TextField>;

impl Configured<controls::TextField> {
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
pub type RadioGroup = Configured<controls::HostedRadioGroup>;

impl Configured<controls::HostedRadioGroup> {
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
pub type Tabs = Configured<controls::HostedTabs>;

impl Configured<controls::HostedTabs> {
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
pub type Combobox = Configured<controls::Combobox>;

impl Configured<controls::Combobox> {
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
pub type TextEditor = Configured<controls::TextEditor>;

impl Configured<controls::TextEditor> {
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
pub type Calendar = Configured<controls::Calendar>;

impl Configured<controls::Calendar> {
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
}
