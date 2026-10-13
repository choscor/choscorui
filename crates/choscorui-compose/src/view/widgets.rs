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
    ButtonVariant, CalendarMode, ChartKind, ChartSeries, CommandId, CommandItem, ControlKind,
    ControlMessage, ControlSize, ControlValue, ListEntry, MenuItem, NavigationLink, SheetSide,
    SidebarItem, ToggleItem,
};
use choscorui_core::{CivilDate, Icon, ItemModel, SelectionMode, SemanticRole};
use std::rc::Rc;

/// The command id that view buttons report; views run closures instead.
const NO_COMMAND: CommandId = CommandId(0);

/// A list that a control shows: a fixed `Vec`, or a closure that makes
/// it from the environment.
pub trait Items<T>: 'static {
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

fn as_dates(value: &ControlValue) -> Option<Vec<CivilDate>> {
    match value {
        ControlValue::Dates(dates) => Some(dates.clone()),
        _ => None,
    }
}

/// A push button that runs `action`: `Button::new(t!("save"), move || …)`.
/// Without a variant or size of its own, it takes the environment's
/// `button_variant` and `control_size`.
pub struct Button {
    label: Label,
    action: Rc<dyn Fn()>,
    variant: Option<Live<ButtonVariant>>,
    size: Option<Live<ControlSize>>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
}

impl Button {
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
pub type Checkbox = Configured<controls::Checkbox>;

impl Configured<controls::Checkbox> {
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
pub type Switch = Configured<controls::Switch>;

impl Configured<controls::Switch> {
    pub fn new<M>(label: impl IntoLive<TextContent, M>, on: Binding<bool>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Switch, move |env| {
            controls::Switch::new(text(&label, env), false)
        })
        .bind(on, ControlValue::Bool, as_bool)
    }
}

/// A button that stays on or off and edits `on`.
pub type Toggle = Configured<controls::Toggle>;

impl Configured<controls::Toggle> {
    pub fn new<M>(label: impl IntoLive<TextContent, M>, on: Binding<bool>) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Toggle, move |env| {
            controls::Toggle::new(text(&label, env), false)
        })
        .bind(on, ControlValue::Bool, as_bool)
    }
}

/// A row of toggles.
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
pub type ButtonGroup = Configured<controls::HostedButtonGroup>;

impl Configured<controls::HostedButtonGroup> {
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
pub type Kbd = Configured<controls::Kbd>;

impl Configured<controls::Kbd> {
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
pub type Select = Configured<controls::Select>;

impl Configured<controls::Select> {
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
pub type Input = Configured<controls::Input>;

impl Configured<controls::Input> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }

    /// An input that hides what the user types.
    pub fn password<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Input, move |env| {
            controls::Input::password(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A multiline text input that edits `value`.
pub type Textarea = Configured<controls::Textarea>;

impl Configured<controls::Textarea> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, value: Binding<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Textarea, move |env| {
            controls::Textarea::new(text(&name, env), "")
        })
        .bind(value, ControlValue::Text, as_text)
    }
}

/// A one-time code input of `length` characters that edits `value`.
pub type InputOtp = Configured<controls::InputOtp>;

impl Configured<controls::InputOtp> {
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
pub type FieldText = Configured<controls::FieldText>;

impl Configured<controls::FieldText> {
    pub fn new<M>(part: controls::FieldPart, text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Field, move |env| {
            controls::FieldText::new(part, self::text(&content, env))
        })
    }
}

/// An area that opens `items` at the pointer on a secondary click. Use
/// `.on_command` for the chosen action.
pub type ContextMenu = Configured<controls::ContextMenu>;

impl Configured<controls::ContextMenu> {
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
pub type Menubar = Configured<controls::Menubar>;

impl Configured<controls::Menubar> {
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
pub type Popover = Configured<controls::Popover>;

impl Configured<controls::Popover> {
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
pub type Dialog = Configured<controls::Dialog>;

impl Configured<controls::Dialog> {
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
pub type Command = Configured<controls::Command>;

impl Configured<controls::Command> {
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
pub type Breadcrumb = Configured<controls::Breadcrumb>;

impl Configured<controls::Breadcrumb> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, items: impl Items<String>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Breadcrumb, move |env| {
            controls::Breadcrumb::new(text(&name, env), items.items(env))
        })
    }
}

/// Page links with previous and next buttons; `page` edits the current
/// page.
pub type Pagination = Configured<controls::Pagination>;

impl Configured<controls::Pagination> {
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
pub type NavigationMenu = Configured<controls::NavigationMenu>;

impl Configured<controls::NavigationMenu> {
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
pub type Sidebar = Configured<controls::Sidebar>;

impl Configured<controls::Sidebar> {
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
pub type Card = Configured<controls::Card>;

impl Configured<controls::Card> {
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
pub type Badge = Configured<controls::Badge>;

impl Configured<controls::Badge> {
    pub fn new<M>(text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Badge, move |env| {
            controls::Badge::new(self::text(&content, env))
        })
    }
}

/// A round user image with a text fallback.
pub type Avatar = Configured<controls::Avatar>;

impl Configured<controls::Avatar> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, fallback: impl Into<String>) -> Self {
        let name = label(name);
        let fallback = fallback.into();
        Configured::custom(ControlKind::Avatar, move |env| {
            controls::Avatar::new(text(&name, env), fallback.clone())
        })
    }
}

/// Slides that move one at a time; `current` edits the shown slide.
pub type Carousel = Configured<controls::Carousel>;

impl Configured<controls::Carousel> {
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
pub type DataTable = Configured<controls::DataTable>;

impl Configured<controls::DataTable> {
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
pub type List = Configured<controls::ItemView>;

impl Configured<controls::ItemView> {
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
pub type Chart = Configured<controls::Chart>;

impl Configured<controls::Chart> {
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
pub type Alert = Configured<controls::Alert>;

impl Configured<controls::Alert> {
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
pub type Spinner = Configured<controls::Spinner>;

impl Configured<controls::Spinner> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Spinner, move |env| {
            controls::Spinner::new(text(&name, env))
        })
    }
}

/// A pulsing placeholder block of `width` by `height` points.
pub type Skeleton = Configured<controls::Skeleton>;

impl Configured<controls::Skeleton> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, width: f64, height: f64) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Skeleton, move |env| {
            controls::Skeleton::new(text(&name, env), width, height)
        })
    }
}

/// A centered empty state. Its buttons report `.on_item`.
pub type EmptyState = Configured<controls::Empty>;

impl Configured<controls::Empty> {
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
pub type AspectRatio = Configured<controls::AspectRatio>;

impl Configured<controls::AspectRatio> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>, ratio: f64) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::AspectRatio, move |env| {
            controls::AspectRatio::new(text(&name, env), ratio)
        })
    }
}

/// A section that shows or hides its items; `open` edits whether it
/// shows them.
pub type Collapsible = Configured<controls::Collapsible>;

impl Configured<controls::Collapsible> {
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
pub type Accordion = Configured<controls::Accordion>;

impl Configured<controls::Accordion> {
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
pub type Marker = Configured<controls::Marker>;

impl Configured<controls::Marker> {
    pub fn new<M>(text: impl IntoLive<TextContent, M>) -> Self {
        let content = label(text);
        Configured::custom(ControlKind::Marker, move |env| {
            controls::Marker::new(self::text(&content, env))
        })
    }
}

/// A file with media, a detail line, and a progress state.
pub type Attachment = Configured<controls::Attachment>;

impl Configured<controls::Attachment> {
    pub fn new<M>(name: impl IntoLive<TextContent, M>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Attachment, move |env| {
            controls::Attachment::new(text(&name, env))
        })
    }
}
