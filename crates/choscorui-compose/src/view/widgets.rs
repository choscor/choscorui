//! View constructors for controls. Each takes its content, a `Binding`
//! where the control edits a value, and an action where it fires one.
//! Behavior, keyboard handling, and semantics stay in the controls.
//!
//! Text arguments take literal text or a catalog string (`t!`); they
//! follow the locale. List arguments take a `Vec` or a closure that makes
//! the list from the environment, for translated items.

use super::View;
use super::configured::Configured;
use super::control::ControlView;
use super::env::Env;
use super::host::Cx;
use super::limits::Limits;
use super::state::{Binding, IntoLive, Live};
use super::text::TextContent;
use choscorui_controls as controls;
use choscorui_controls::{
    ButtonVariant, CalendarMode, CharClass, ChartKind, ChartSeries, CommandId, CommandItem,
    ControlKind, ControlMessage, ControlSize, ControlValue, InputType, ListEntry, MenuItem,
    NavigationLink, SheetSide, SidebarItem, ToggleItem,
};
use choscorui_core::{CivilDate, Icon, ItemModel, SelectionMode, SemanticRole};
use std::rc::Rc;

/// The command id that view buttons report; views run closures instead.
const NO_COMMAND: CommandId = CommandId(0);

/// A list that a control shows: a fixed `Vec`, or a closure that makes
/// it from the environment.
pub trait Items<T>: 'static {
    /// The items for the environment `env`.
    fn items(&self, env: &Env) -> Vec<T>;
}

impl<T: Clone + 'static> Items<T> for Vec<T> {
    fn items(&self, _env: &Env) -> Vec<T> {
        self.clone()
    }
}

impl<T, F: Fn(&Env) -> Vec<T> + 'static> Items<T> for F {
    fn items(&self, env: &Env) -> Vec<T> {
        self(env)
    }
}

/// A text argument of a control view.
pub(super) type Label = Live<TextContent>;

pub(super) fn label<M>(text: impl IntoLive<TextContent, M>) -> Label {
    text.into_live()
}

/// The text of `label` in `env`.
pub(super) fn text(label: &Label, env: &Env) -> String {
    label.get().resolve(env)
}

/// The control argument when there is one, otherwise the environment's.
fn pick<T: Clone + 'static>(own: &Option<Live<T>>, env: Option<T>) -> Option<T> {
    own.as_ref().map(Live::get).or(env)
}

fn as_bool(value: &ControlValue) -> Option<bool> {
    match value {
        ControlValue::Bool(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn as_text(value: &ControlValue) -> Option<String> {
    match value {
        ControlValue::Text(value) => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn as_index(value: &ControlValue) -> Option<Option<usize>> {
    match value {
        ControlValue::Index(index) => Some(*index),
        ControlValue::Indices(indices) => Some(indices.first().copied()),
        _ => None,
    }
}

pub(super) fn as_position(value: &ControlValue) -> Option<usize> {
    match value {
        ControlValue::Index(Some(index)) => Some(*index),
        _ => None,
    }
}

pub(super) fn to_position(index: usize) -> ControlValue {
    ControlValue::Index(Some(index))
}

pub(super) fn as_indices(value: &ControlValue) -> Option<Vec<usize>> {
    match value {
        ControlValue::Indices(indices) => Some(indices.clone()),
        ControlValue::Index(index) => Some(index.iter().copied().collect()),
        _ => None,
    }
}

fn one_of(index: Option<usize>) -> ControlValue {
    ControlValue::Indices(index.into_iter().collect())
}

fn as_range(value: &ControlValue) -> Option<(f64, f64)> {
    match value {
        ControlValue::Range(low, high) => Some((*low, *high)),
        _ => None,
    }
}

fn to_range((low, high): (f64, f64)) -> ControlValue {
    ControlValue::Range(low, high)
}

fn as_date(value: &ControlValue) -> Option<Option<CivilDate>> {
    match value {
        ControlValue::Date(date) => Some(*date),
        _ => None,
    }
}

pub(super) fn as_dates(value: &ControlValue) -> Option<Vec<CivilDate>> {
    match value {
        ControlValue::Dates(dates) => Some(dates.clone()),
        _ => None,
    }
}

/// A push button that runs `action`: `Button::new(t!("save"), move || …)`.
/// Without a variant or size of its own, it takes the environment's
/// `button_variant` and `control_size`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::ButtonVariant;
/// let save = Button::new("Save", || println!("saved"))
///     .variant(ButtonVariant::Outline);
/// ```
pub struct Button {
    label: Label,
    action: Rc<dyn Fn()>,
    variant: Option<Live<ButtonVariant>>,
    size: Option<Live<ControlSize>>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
}

impl Button {
    /// A button with `label` that runs `action` when the user activates it.
    pub fn new<M>(label: impl IntoLive<TextContent, M>, action: impl Fn() + 'static) -> Self {
        Self {
            label: label.into_live(),
            action: Rc::new(action),
            variant: None,
            size: None,
            leading: None,
            trailing: None,
        }
    }

    /// The look of the button, instead of the environment's.
    #[must_use]
    pub fn variant<M>(mut self, variant: impl IntoLive<ButtonVariant, M>) -> Self {
        self.variant = Some(variant.into_live());
        self
    }

    /// The size of the button, instead of the environment's.
    #[must_use]
    pub fn size<M>(mut self, size: impl IntoLive<ControlSize, M>) -> Self {
        self.size = Some(size.into_live());
        self
    }

    /// An icon before the label, or the only content of an icon size.
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// An icon after the label.
    #[must_use]
    pub fn trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }
}

impl View for Button {
    fn mount(self, cx: &mut Cx) {
        let Self {
            label,
            action,
            variant,
            size,
            leading,
            trailing,
        } = self;
        ControlView::build(move |env: &Env| {
            let variant = pick(&variant, env.button_variant()).unwrap_or_default();
            let size = pick(&size, env.control_size()).unwrap_or_default();
            let mut button = controls::Button::new(text(&label, env), NO_COMMAND)
                .with_variant(variant)
                .with_size(size);
            if let Some(icon) = leading {
                button = button.with_icon(icon);
            }
            if let Some(icon) = trailing {
                button = button.with_trailing_icon(icon);
            }
            Box::new(button)
        })
        .kind(ControlKind::Button)
        .on_message(move |message| {
            if *message == ControlMessage::Activated {
                action();
            }
        })
        .mount(cx);
    }
}

/// A checkbox that edits `checked`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Terms;
///
/// impl View for Terms {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let accepted = cx.state(false);
///         Checkbox::new("Accept the terms", accepted.binding())
///     }
/// }
/// ```
pub type Checkbox = Configured<controls::Checkbox>;

impl Configured<controls::Checkbox> {
    /// A checkbox with `label` that edits `checked`.
    pub fn new<M>(label: impl IntoLive<TextContent, M>, checked: Binding<bool>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Checkbox, move |env| {
            controls::Checkbox::new(text(&label, env), false)
        })
        .bind(checked, ControlValue::Bool, as_bool)
    }

    /// A checkbox that also shows the mixed state, for a choice that
    /// covers items with both values. `checked` is `None` while mixed. A
    /// press sets it to `Some`.
    pub fn mixed<M>(label: impl IntoLive<TextContent, M>, checked: Binding<Option<bool>>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Checkbox, move |env| {
            controls::Checkbox::new(text(&label, env), false)
        })
        .bind(checked, ControlValue::Mixed, |value| {
            as_bool(value).map(Some)
        })
    }
}

/// An on or off switch that edits `on`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Network;
///
/// impl View for Network {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let on = cx.state(true);
///         Switch::new("Wi-Fi", on.binding())
///     }
/// }
/// ```
pub type Switch = Configured<controls::Switch>;

impl Configured<controls::Switch> {
    /// A switch with `label` that edits `on`.
    pub fn new<M>(label: impl IntoLive<TextContent, M>, on: Binding<bool>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Switch, move |env| {
            controls::Switch::new(text(&label, env), false)
        })
        .bind(on, ControlValue::Bool, as_bool)
    }
}

/// A button that stays on or off and edits `on`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Format;
///
/// impl View for Format {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let bold = cx.state(false);
///         Toggle::new("Bold", bold.binding())
///     }
/// }
/// ```
pub type Toggle = Configured<controls::Toggle>;

impl Configured<controls::Toggle> {
    /// A toggle button with `label` that edits `on`.
    pub fn new<M>(label: impl IntoLive<TextContent, M>, on: Binding<bool>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Toggle, move |env| {
            controls::Toggle::new(text(&label, env), false)
        })
        .bind(on, ControlValue::Bool, as_bool)
    }
}

/// A row of toggles.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::ToggleItem;
/// struct Alignment;
///
/// impl View for Alignment {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selected = cx.state(Some(0));
///         let items = vec![ToggleItem::new("Left"), ToggleItem::new("Center"), ToggleItem::new("Right")];
///         ToggleGroup::single("Alignment", items, selected.binding())
///     }
/// }
/// ```
pub type ToggleGroup = Configured<controls::ToggleGroup>;

impl Configured<controls::ToggleGroup> {
    /// Toggles of which at most one is on: `selected` edits its index.
    pub fn single<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<ToggleItem>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ToggleGroup, move |env| {
            controls::ToggleGroup::new(text(&name, env), items.items(env), false)
        })
        .bind(selected, one_of, as_index)
    }

    /// Toggles that are on or off on their own: `selected` edits the
    /// indices that are on.
    pub fn multiple<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<ToggleItem>,
        selected: Binding<Vec<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ToggleGroup, move |env| {
            controls::ToggleGroup::new(text(&name, env), items.items(env), true)
        })
        .bind(selected, ControlValue::Indices, as_indices)
    }
}

/// Joined members of a [`controls::ButtonGroup`]: toggles, buttons,
/// separators, and text parts. `labels` names each member, and `checked`
/// edits the indices of the checked toggle members. A button member
/// reports its index through `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls as controls;
/// # use choscorui_controls::{ButtonGroupMember, CommandId};
/// struct Styles;
///
/// impl View for Styles {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let checked = cx.state(Vec::new());
///         let members = vec![
///             ButtonGroupMember::Toggle(CommandId(1)),
///             ButtonGroupMember::Toggle(CommandId(2)),
///         ];
///         let group = controls::ButtonGroup::from_members(members, false);
///         ButtonGroup::new(group, vec!["Bold".to_owned(), "Italic".to_owned()], checked.binding())
///     }
/// }
/// ```
pub type ButtonGroup = Configured<controls::HostedButtonGroup>;

impl Configured<controls::HostedButtonGroup> {
    /// A view of `group`, with `labels` for its members and `checked` for its toggles.
    pub fn new(
        group: controls::ButtonGroup,
        labels: impl Items<String>,
        checked: Binding<Vec<usize>>,
    ) -> Self {
        Configured::custom(ControlKind::ButtonGroup, move |env| {
            group.clone().hosted(labels.items(env))
        })
        .bind(checked, ControlValue::Indices, as_indices)
    }
}

/// Key caps for a shortcut.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let shortcut = Kbd::new("⌘K");
/// ```
pub type Kbd = Configured<controls::Kbd>;

impl Configured<controls::Kbd> {
    /// Key caps that show `key`.
    pub fn new<M>(key: impl IntoLive<TextContent, M>) -> Self {
        let key = label(key);
        Configured::custom(ControlKind::Kbd, move |env| {
            controls::Kbd::new(text(&key, env))
        })
    }

    /// Key caps for a combination of keys, for example `⌘` and `K`.
    pub fn keys(keys: impl Items<String>) -> Self {
        Configured::custom(ControlKind::Kbd, move |env| {
            controls::Kbd::group(keys.items(env))
        })
    }
}

/// A trigger with a popup list; `selected` edits the chosen option.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::ListEntry;
/// struct Fruit;
///
/// impl View for Fruit {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selected = cx.state(None);
///         let entries = vec![ListEntry::option("Apple"), ListEntry::option("Pear")];
///         Select::new("Fruit", entries, selected.binding())
///     }
/// }
/// ```
pub type Select = Configured<controls::Select>;

impl Configured<controls::Select> {
    /// A select named `name` with `entries`; `selected` edits the chosen entry.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        entries: impl Items<ListEntry>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Select, move |env| {
            controls::Select::new(text(&name, env), entries.items(env))
        })
        .bind(selected, ControlValue::Index, as_index)
    }

    /// A select with the native-select look: a plain input box with a
    /// chevron. It behaves like [`Select::new`].
    pub fn native<M>(
        name: impl IntoLive<TextContent, M>,
        entries: impl Items<ListEntry>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::NativeSelect, move |env| {
            controls::Select::native(text(&name, env), entries.items(env))
        })
        .bind(selected, ControlValue::Index, as_index)
    }
}

/// A single-line text input that edits `value`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Profile;
///
/// impl View for Profile {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let name = cx.state(String::new());
///         Input::new("Name", name.binding())
///     }
/// }
/// ```
pub type Input = Configured<controls::Input>;

impl Configured<controls::Input> {
    /// An input named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }

    /// An input that hides what the user types. Cut and copy do nothing
    /// in it.
    pub fn password<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::password(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }

    /// An input for an email address. Return and blur mark text that is
    /// no email address invalid.
    pub fn email<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        Self::of_type(name, InputType::Email, value)
    }

    /// An input for a web address, such as `https://example.com`. Return
    /// and blur mark other text invalid.
    pub fn url<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        Self::of_type(name, InputType::Url, value)
    }

    /// An input for a phone number. It takes digits, spaces, and
    /// `+ - ( ) .` only.
    pub fn tel<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        Self::of_type(name, InputType::Tel, value)
    }

    /// An input for a search query.
    pub fn search<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        Self::of_type(name, InputType::Search, value)
    }

    /// An input for a number. It takes digits, one leading minus, and one
    /// `.` only.
    pub fn number<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        Self::of_type(name, InputType::Number, value)
    }

    fn of_type<M>(
        name: impl IntoLive<TextContent, M>,
        kind: InputType,
        value: Binding<String>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::new(text(&name, env), "").with_type(kind)
        })
        .bind(value, ControlValue::Text, as_text)
    }

    /// Allows at most `length` grapheme clusters. A paste longer than the
    /// room left is cut to the room, and a key with no room beeps.
    #[must_use]
    pub fn max_length(self, length: usize) -> Self {
        self.with(move |input| input.max_length(length))
    }

    /// Allows only the characters of `class`. A key or a paste with
    /// another character changes nothing and beeps.
    #[must_use]
    pub fn filter(self, class: CharClass) -> Self {
        self.with(move |input| input.filter(class.clone()))
    }
}

/// A multiline text input that edits `value`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Feedback;
///
/// impl View for Feedback {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let notes = cx.state(String::new());
///         Textarea::new("Notes", notes.binding())
///     }
/// }
/// ```
pub type Textarea = Configured<controls::Textarea>;

impl Configured<controls::Textarea> {
    /// A multiline input named `name` that edits `value`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Textarea, move |env| {
            controls::Textarea::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A one-time code input of `length` characters that edits `value`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Verify;
///
/// impl View for Verify {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let code = cx.state(String::new());
///         InputOtp::new("Code", 6, code.binding())
///     }
/// }
/// ```
pub type InputOtp = Configured<controls::InputOtp>;

impl Configured<controls::InputOtp> {
    /// A code input named `name` of `length` characters that edits `value`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        length: usize,
        value: Binding<String>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::InputOtp, move |env| {
            controls::InputOtp::new(text(&name, env), length)
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// Low and high values along one track; `values` edits both. It covers
/// `0.0..=100.0` with a step of 1 until `.range` and `.step` change them.
/// Values outside the bounds show the nearest bound and report a
/// [`ViewError::Clamped`](super::ViewError::Clamped).
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, RangeSlider, View};
///
/// struct Price;
///
/// impl View for Price {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let price = cx.state((20.0, 80.0));
///         RangeSlider::new("Price", price.binding()).range(0.0..=200.0).step(5.0)
///     }
/// }
/// ```
pub type RangeSlider = Configured<controls::RangeSlider>;

impl Configured<controls::RangeSlider> {
    /// A range slider named `name` that edits `values`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, values: Binding<(f64, f64)>) -> Self {
        let name = label(name);
        let limits = super::builtins::percent();
        let read = limits.clone();
        Configured::custom(ControlKind::Slider, move |env| {
            let Limits {
                minimum,
                maximum,
                step,
                ..
            } = read.get();
            controls::RangeSlider::new(text(&name, env), minimum, maximum, step, minimum, maximum)
        })
        .limited(limits)
        .bind(values, to_range, as_range)
    }

    /// The bounds of both values. Reversed bounds become `start..=start`,
    /// and bounds that are not finite stay as they are. Both report a
    /// [`ViewError::Clamped`](super::ViewError::Clamped).
    #[must_use]
    pub fn range(self, bounds: std::ops::RangeInclusive<f64>) -> Self {
        self.set_bounds(*bounds.start(), *bounds.end())
    }

    /// The snap step of the values. A step that is not positive becomes
    /// 1 and reports a [`ViewError::Clamped`](super::ViewError::Clamped).
    #[must_use]
    pub fn step(self, step: f64) -> Self {
        self.set_step(step)
    }
}

/// A button with a calendar popover.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::CivilDate;
/// struct Booking;
///
/// impl View for Booking {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let month = CivilDate::new(2026, 10, 1).expect("a valid date");
///         let date = cx.state(None);
///         DatePicker::new("Date", month, date.binding())
///     }
/// }
/// ```
pub type DatePicker = Configured<controls::DatePicker>;

impl Configured<controls::DatePicker> {
    /// One date; `date` edits it. The calendar first shows `month`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        month: CivilDate,
        date: Binding<Option<CivilDate>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::DatePicker, move |env| {
            controls::DatePicker::new(text(&name, env), env.locale(), month)
        })
        .bind(date, ControlValue::Date, as_date)
    }

    /// Several dates, or a range with `CalendarMode::Range`; `dates`
    /// edits them.
    pub fn dates<M>(
        name: impl IntoLive<TextContent, M>,
        month: CivilDate,
        mode: CalendarMode,
        dates: Binding<Vec<CivilDate>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::DatePicker, move |env| {
            controls::DatePicker::new(text(&name, env), env.locale(), month).with_mode(mode)
        })
        .bind(dates, ControlValue::Dates, as_dates)
    }
}

/// A legend, label, description, or error line of a form field.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::FieldPart;
/// let help = FieldText::new(FieldPart::Description, "Use 8 or more characters.");
/// ```
pub type FieldText = Configured<controls::FieldText>;

impl Configured<controls::FieldText> {
    /// The `part` of a field that shows `text`.
    pub fn new<M>(part: controls::FieldPart, text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Field, move |env| {
            controls::FieldText::new(part, self::text(&content, env))
        })
    }
}

/// An area that opens `items` at the pointer on a secondary click. Use
/// `.on_command` for the chosen action.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{CommandId, MenuItem};
/// let items = vec![MenuItem::action("Copy", CommandId(1)), MenuItem::action("Paste", CommandId(2))];
/// let area = ContextMenu::new("Canvas", "Right-click here", items)
///     .on_command(|command| println!("chose {command:?}"));
/// ```
pub type ContextMenu = Configured<controls::ContextMenu>;

impl Configured<controls::ContextMenu> {
    /// An area named `name` with the hint `hint` that opens `items`.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        hint: impl IntoLive<TextContent, N>,
        items: impl Items<MenuItem>,
    ) -> Self {
        let (name, hint) = (label(name), label(hint));
        Configured::custom(ControlKind::Menu, move |env| {
            controls::ContextMenu::new(text(&name, env), text(&hint, env), items.items(env))
        })
    }
}

/// A row of menu titles. Use `.on_command` for the chosen action.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{CommandId, MenuItem};
/// let file = vec![MenuItem::action("Open", CommandId(1)), MenuItem::action("Save", CommandId(2))];
/// let bar = Menubar::new("Main menu", vec![("File".to_owned(), file)])
///     .on_command(|command| println!("chose {command:?}"));
/// ```
pub type Menubar = Configured<controls::Menubar>;

impl Configured<controls::Menubar> {
    /// A menu bar named `name` with one menu for each (title, items) pair.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        menus: impl Items<(String, Vec<MenuItem>)>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Menubar, move |env| {
            controls::Menubar::new(text(&name, env), menus.items(env))
        })
    }
}

/// A trigger button with a floating panel; `open` edits whether it shows.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Help;
///
/// impl View for Help {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(false);
///         Popover::new("Info", "Shortcuts", "Press ⌘K to search.", open.binding())
///     }
/// }
/// ```
pub type Popover = Configured<controls::Popover>;

impl Configured<controls::Popover> {
    /// A popover with a `trigger` button, a `title`, and a `description`; `open` edits whether it shows.
    pub fn new<M, N, O>(
        trigger: impl IntoLive<TextContent, M>,
        title: impl IntoLive<TextContent, N>,
        description: impl IntoLive<TextContent, O>,
        open: Binding<bool>,
    ) -> Self {
        let (trigger, title, description) = (label(trigger), label(title), label(description));
        Configured::custom(ControlKind::Popover, move |env| {
            controls::Popover::new(
                text(&trigger, env),
                text(&title, env),
                text(&description, env),
            )
        })
        .open(open)
    }

    /// A panel that opens while the pointer rests on the trigger.
    pub fn hover_card<M, N, O>(
        trigger: impl IntoLive<TextContent, M>,
        title: impl IntoLive<TextContent, N>,
        description: impl IntoLive<TextContent, O>,
    ) -> Self {
        let (trigger, title, description) = (label(trigger), label(title), label(description));
        Configured::custom(ControlKind::HoverCard, move |env| {
            controls::Popover::hover_card(
                text(&trigger, env),
                text(&title, env),
                text(&description, env),
            )
        })
    }
}

/// A trigger button with a modal panel; `open` edits whether it shows.
/// Its action buttons report `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Delete;
///
/// impl View for Delete {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(false);
///         Dialog::new("Delete", "Delete the file?", "You cannot undo this.", open.binding())
///     }
/// }
/// ```
pub type Dialog = Configured<controls::Dialog>;

impl Configured<controls::Dialog> {
    /// A dialog with a `trigger` button, a `title`, and a `description`; `open` edits whether it shows.
    pub fn new<M, N, O>(
        trigger: impl IntoLive<TextContent, M>,
        title: impl IntoLive<TextContent, N>,
        description: impl IntoLive<TextContent, O>,
        open: Binding<bool>,
    ) -> Self {
        let (trigger, title, description) = (label(trigger), label(title), label(description));
        Configured::custom(ControlKind::Dialog, move |env| {
            controls::Dialog::new(
                text(&trigger, env),
                text(&title, env),
                text(&description, env),
            )
        })
        .open(open)
    }

    /// A panel that slides in from `side`.
    pub fn sheet<M, N, O>(
        side: SheetSide,
        trigger: impl IntoLive<TextContent, M>,
        title: impl IntoLive<TextContent, N>,
        description: impl IntoLive<TextContent, O>,
        open: Binding<bool>,
    ) -> Self {
        let (trigger, title, description) = (label(trigger), label(title), label(description));
        Configured::custom(ControlKind::Sheet, move |env| {
            controls::Dialog::sheet(
                side,
                text(&trigger, env),
                text(&title, env),
                text(&description, env),
            )
        })
        .open(open)
    }

    /// A panel that slides in from `side`, with a handle.
    pub fn drawer<M, N, O>(
        side: SheetSide,
        trigger: impl IntoLive<TextContent, M>,
        title: impl IntoLive<TextContent, N>,
        description: impl IntoLive<TextContent, O>,
        open: Binding<bool>,
    ) -> Self {
        let (trigger, title, description) = (label(trigger), label(title), label(description));
        Configured::custom(ControlKind::Drawer, move |env| {
            controls::Dialog::drawer(
                side,
                text(&trigger, env),
                text(&title, env),
                text(&description, env),
            )
        })
        .open(open)
    }
}

/// A search input over grouped commands. The chosen row reports
/// `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::CommandItem;
/// let groups = vec![("Files".to_owned(), vec![CommandItem::new("Open"), CommandItem::new("Save")])];
/// let palette = Command::new("Commands", "Type a command", groups)
///     .on_item(|row| println!("row {row}"));
/// ```
pub type Command = Configured<controls::Command>;

impl Configured<controls::Command> {
    /// A command palette named `name` with `placeholder` text and `groups` of commands.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        placeholder: impl IntoLive<TextContent, N>,
        groups: impl Items<(String, Vec<CommandItem>)>,
    ) -> Self {
        let (name, placeholder) = (label(name), label(placeholder));
        Configured::custom(ControlKind::Command, move |env| {
            controls::Command::new(text(&name, env), text(&placeholder, env), groups.items(env))
        })
    }

    /// A button labeled `trigger` that opens the command search in a
    /// modal dialog.
    pub fn dialog<M, N, O>(
        trigger: impl IntoLive<TextContent, M>,
        name: impl IntoLive<TextContent, N>,
        placeholder: impl IntoLive<TextContent, O>,
        groups: impl Items<(String, Vec<CommandItem>)>,
    ) -> Self {
        let (trigger, name, placeholder) = (label(trigger), label(name), label(placeholder));
        Configured::custom(ControlKind::Command, move |env| {
            controls::Command::dialog(
                text(&trigger, env),
                text(&name, env),
                text(&placeholder, env),
                groups.items(env),
            )
        })
    }
}

/// A trail of links to the current page. A link reports `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let trail = Breadcrumb::new("Path", vec!["Home".to_owned(), "Docs".to_owned()])
///     .on_item(|index| println!("go to {index}"));
/// ```
pub type Breadcrumb = Configured<controls::Breadcrumb>;

impl Configured<controls::Breadcrumb> {
    /// A breadcrumb named `name` with one link for each item.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, items: impl Items<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Breadcrumb, move |env| {
            controls::Breadcrumb::new(text(&name, env), items.items(env))
        })
    }
}

/// Page links with previous and next buttons; `page` edits the current
/// page.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Results;
///
/// impl View for Results {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let page = cx.state(0);
///         Pagination::new("Pages", "Previous", "Next", 10, page.binding())
///     }
/// }
/// ```
pub type Pagination = Configured<controls::Pagination>;

impl Configured<controls::Pagination> {
    /// A pagination named `name` with `pages` pages; `page` edits the current page.
    pub fn new<M, N, O>(
        name: impl IntoLive<TextContent, M>,
        previous: impl IntoLive<TextContent, N>,
        next: impl IntoLive<TextContent, O>,
        pages: usize,
        page: Binding<usize>,
    ) -> Self {
        let (name, previous, next) = (label(name), label(previous), label(next));
        Configured::custom(ControlKind::Pagination, move |env| {
            controls::Pagination::new(
                text(&name, env),
                text(&previous, env),
                text(&next, env),
                pages,
                0,
            )
        })
        .bind(page, to_position, as_position)
    }
}

/// Site links whose triggers open panels. A link reports `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::NavigationLink;
/// let links = vec![NavigationLink::new("Guide", "Start here")];
/// let menu = NavigationMenu::new("Site", vec![("Docs".to_owned(), links)])
///     .on_item(|index| println!("link {index}"));
/// ```
pub type NavigationMenu = Configured<controls::NavigationMenu>;

impl Configured<controls::NavigationMenu> {
    /// A navigation menu named `name` with one trigger for each (title, links) pair.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<(String, Vec<NavigationLink>)>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::NavigationMenu, move |env| {
            controls::NavigationMenu::new(text(&name, env), items.items(env))
        })
    }
}

/// Grouped navigation items; `selected` edits the chosen item.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::SidebarItem;
/// # use choscorui_core::Icon;
/// struct Navigation;
///
/// impl View for Navigation {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selected = cx.state(Some(0));
///         let items = vec![SidebarItem::new("Files", Icon::File), SidebarItem::new("Mail", Icon::Mail)];
///         Sidebar::new("Navigation", "Toggle sidebar", vec![("Main".to_owned(), items)], selected.binding())
///     }
/// }
/// ```
pub type Sidebar = Configured<controls::Sidebar>;

impl Configured<controls::Sidebar> {
    /// A sidebar named `name` with a `toggle` button and `groups` of items.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        toggle: impl IntoLive<TextContent, N>,
        groups: impl Items<(String, Vec<SidebarItem>)>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let (name, toggle) = (label(name), label(toggle));
        Configured::custom(ControlKind::Sidebar, move |env| {
            controls::Sidebar::new(text(&name, env), text(&toggle, env), groups.items(env))
        })
        .bind(selected, ControlValue::Index, as_index)
    }
}

/// A surface with a title and description. Its buttons report `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let card = Card::new("Storage", "12 GB of 50 GB used");
/// ```
pub type Card = Configured<controls::Card>;

impl Configured<controls::Card> {
    /// A card with `title` and `description`.
    pub fn new<M, N>(
        title: impl IntoLive<TextContent, M>,
        description: impl IntoLive<TextContent, N>,
    ) -> Self {
        let (title, description) = (label(title), label(description));
        Configured::custom(ControlKind::Card, move |env| {
            controls::Card::new(text(&title, env), text(&description, env))
        })
    }
}

/// A short status label.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let badge = Badge::new("New");
/// ```
pub type Badge = Configured<controls::Badge>;

impl Configured<controls::Badge> {
    /// A badge that shows `text`.
    pub fn new<M>(text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Badge, move |env| {
            controls::Badge::new(self::text(&content, env))
        })
    }
}

/// A round user image with a text fallback.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let avatar = Avatar::new("Ada Lovelace", "AL");
/// ```
pub type Avatar = Configured<controls::Avatar>;

impl Configured<controls::Avatar> {
    /// An avatar for `name` that shows the `fallback` text.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, fallback: impl Into<String>) -> Self {
        let name = label(name);
        let fallback = fallback.into();
        Configured::custom(ControlKind::Avatar, move |env| {
            controls::Avatar::new(text(&name, env), fallback.clone())
        })
    }
}

/// Slides that move one at a time; `current` edits the shown slide.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Gallery;
///
/// impl View for Gallery {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let current = cx.state(0);
///         let slides = vec!["One".to_owned(), "Two".to_owned(), "Three".to_owned()];
///         Carousel::new("Photos", "Previous", "Next", slides, current.binding())
///     }
/// }
/// ```
pub type Carousel = Configured<controls::Carousel>;

impl Configured<controls::Carousel> {
    /// A carousel named `name` of `slides`; `current` edits the shown slide.
    pub fn new<M, N, O>(
        name: impl IntoLive<TextContent, M>,
        previous: impl IntoLive<TextContent, N>,
        next: impl IntoLive<TextContent, O>,
        slides: impl Items<String>,
        current: Binding<usize>,
    ) -> Self {
        let (name, previous, next) = (label(name), label(previous), label(next));
        Configured::custom(ControlKind::Carousel, move |env| {
            controls::Carousel::new(
                text(&name, env),
                text(&previous, env),
                text(&next, env),
                slides.items(env),
            )
        })
        .bind(current, to_position, as_position)
    }
}

/// Sorted, filtered, and paged rows of the model that `model` makes;
/// `selected` edits the selected rows.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{ItemModel, StringListModel};
/// struct People;
///
/// impl View for People {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selected = cx.state(Vec::new());
///         let model = || Box::new(StringListModel::from_strings(["Ada", "Grace"])) as Box<dyn ItemModel>;
///         DataTable::new("People", model, "Previous", "Next", selected.binding())
///     }
/// }
/// ```
pub type DataTable = Configured<controls::DataTable>;

impl Configured<controls::DataTable> {
    /// A data table named `name` of the model that `model` makes.
    pub fn new<M, N, O>(
        name: impl IntoLive<TextContent, M>,
        model: impl Fn() -> Box<dyn ItemModel> + 'static,
        previous: impl IntoLive<TextContent, N>,
        next: impl IntoLive<TextContent, O>,
        selected: Binding<Vec<usize>>,
    ) -> Self {
        let (name, previous, next) = (label(name), label(previous), label(next));
        Configured::custom(ControlKind::Table, move |env| {
            controls::DataTable::new(
                text(&name, env),
                model(),
                text(&previous, env),
                text(&next, env),
            )
        })
        .bind(selected, ControlValue::Indices, as_indices)
    }
}

/// The rows of the model that `model` makes, one line of text each;
/// `selection` edits the positions of the selected rows. The list is
/// virtualized: it paints only the rows in view, so its cost does not
/// grow with the model. The row height comes from the `ListView` recipe.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{ItemModel, StringListModel};
/// struct Fruits;
///
/// impl View for Fruits {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selection = cx.state(Vec::new());
///         let model = || Box::new(StringListModel::from_strings(["Apple", "Pear"])) as Box<dyn ItemModel>;
///         List::new("Fruits", model, selection.binding())
///     }
/// }
/// ```
pub type List = Configured<controls::ItemView>;

impl Configured<controls::ItemView> {
    /// A list named `name` of the model that `model` makes.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        model: impl Fn() -> Box<dyn ItemModel> + 'static,
        selection: Binding<Vec<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ListView, move |env| {
            controls::ItemView::new(text(&name, env), SemanticRole::List, 1.0)
                .with_model(model(), Box::new(controls::TextItemDelegate::default()))
                .with_styled_rows()
        })
        .bind(selection, ControlValue::Indices, as_indices)
    }

    /// How many rows the user can select; the default is
    /// `SelectionMode::Extended`.
    #[must_use]
    pub fn selection_mode(self, mode: SelectionMode) -> Self {
        self.with(move |mut view| {
            view.set_selection_mode(mode);
            view
        })
    }
}

/// A chart of application data.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{ChartKind, ChartSeries};
/// let months = vec!["Jan".to_owned(), "Feb".to_owned()];
/// let sales = vec![ChartSeries::new("Sales", vec![120.0, 180.0])];
/// let chart = Chart::new("Sales", ChartKind::Bar, months, sales);
/// ```
pub type Chart = Configured<controls::Chart>;

impl Configured<controls::Chart> {
    /// A chart named `name` of `kind` with `categories` and `series`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        kind: ChartKind,
        categories: impl Items<String>,
        series: impl Items<ChartSeries>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Chart, move |env| {
            controls::Chart::new(
                text(&name, env),
                kind,
                categories.items(env),
                series.items(env),
            )
        })
    }
}

/// A callout with a title and description. Its buttons report `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let alert = Alert::new("Saved", "Your changes are saved.");
/// ```
pub type Alert = Configured<controls::Alert>;

impl Configured<controls::Alert> {
    /// An alert with `title` and `description`.
    pub fn new<M, N>(
        title: impl IntoLive<TextContent, M>,
        description: impl IntoLive<TextContent, N>,
    ) -> Self {
        let (title, description) = (label(title), label(description));
        Configured::custom(ControlKind::Alert, move |env| {
            controls::Alert::new(text(&title, env), text(&description, env))
        })
    }
}

/// A spinning loader.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let loading = Spinner::new("Loading");
/// ```
pub type Spinner = Configured<controls::Spinner>;

impl Configured<controls::Spinner> {
    /// A spinner named `name`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Spinner, move |env| {
            controls::Spinner::new(text(&name, env))
        })
    }
}

/// A pulsing placeholder block of `width` by `height` points.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let placeholder = Skeleton::new("Loading the title", 200.0, 16.0);
/// ```
pub type Skeleton = Configured<controls::Skeleton>;

impl Configured<controls::Skeleton> {
    /// A skeleton block named `name` of `width` by `height` points.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, width: f64, height: f64) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Skeleton, move |env| {
            controls::Skeleton::new(text(&name, env), width, height)
        })
    }
}

/// A centered empty state. Its buttons report `.on_item`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let empty = EmptyState::new("No files", "Add a file to start.");
/// ```
pub type EmptyState = Configured<controls::Empty>;

impl Configured<controls::Empty> {
    /// An empty state with `title` and `description`.
    pub fn new<M, N>(
        title: impl IntoLive<TextContent, M>,
        description: impl IntoLive<TextContent, N>,
    ) -> Self {
        let (title, description) = (label(title), label(description));
        Configured::custom(ControlKind::Empty, move |env| {
            controls::Empty::new(text(&title, env), text(&description, env))
        })
    }
}

/// A box with a fixed ratio.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let frame = AspectRatio::new("Video", 16.0 / 9.0);
/// ```
pub type AspectRatio = Configured<controls::AspectRatio>;

impl Configured<controls::AspectRatio> {
    /// A box named `name` with the width-to-height `ratio`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, ratio: f64) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::AspectRatio, move |env| {
            controls::AspectRatio::new(text(&name, env), ratio)
        })
    }
}

/// A section that shows or hides its items; `open` edits whether it
/// shows them.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Details;
///
/// impl View for Details {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(false);
///         let items = vec!["Size: 2 MB".to_owned()];
///         Collapsible::new("Details", "Show details", "1 item", items, open.binding())
///     }
/// }
/// ```
pub type Collapsible = Configured<controls::Collapsible>;

impl Configured<controls::Collapsible> {
    /// A collapsible section with `title`, a `toggle` button, a `summary`, and `items`.
    pub fn new<M, N, O>(
        title: impl IntoLive<TextContent, M>,
        toggle: impl IntoLive<TextContent, N>,
        summary: impl IntoLive<TextContent, O>,
        items: impl Items<String>,
        open: Binding<bool>,
    ) -> Self {
        let (title, toggle, summary) = (label(title), label(toggle), label(summary));
        Configured::custom(ControlKind::Collapsible, move |env| {
            controls::Collapsible::new(
                text(&title, env),
                text(&toggle, env),
                text(&summary, env),
                items.items(env),
            )
        })
        .open(open)
    }
}

/// Stacked sections of (title, content); `open` edits the open sections.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Faq;
///
/// impl View for Faq {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(Vec::new());
///         let items = vec![("Shipping".to_owned(), "Two to five days.".to_owned())];
///         Accordion::new("Questions", items, open.binding())
///     }
/// }
/// ```
pub type Accordion = Configured<controls::Accordion>;

impl Configured<controls::Accordion> {
    /// An accordion named `name` of (title, content) sections.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        items: impl Items<(String, String)>,
        open: Binding<Vec<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Accordion, move |env| {
            controls::Accordion::new(text(&name, env), items.items(env))
        })
        .bind(open, ControlValue::Indices, as_indices)
    }
}

/// A muted line between chat messages.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let marker = Marker::new("Today");
/// ```
pub type Marker = Configured<controls::Marker>;

impl Configured<controls::Marker> {
    /// A marker that shows `text`.
    pub fn new<M>(text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Marker, move |env| {
            controls::Marker::new(self::text(&content, env))
        })
    }
}

/// A file with media, a detail line, and a progress state.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let file = Attachment::new("report.pdf");
/// ```
pub type Attachment = Configured<controls::Attachment>;

impl Configured<controls::Attachment> {
    /// An attachment named `name`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Attachment, move |env| {
            controls::Attachment::new(text(&name, env))
        })
    }
}
