//! View constructors for the hosted controls of tool bars, containers,
//! feedback, and popups: avatar groups, stacks, group boxes, tool boxes,
//! scroll bars, toasters, tool buttons, command links, alert dialogs,
//! dropdown menus, and font choosers. Like the views in `widgets`, each
//! takes its content, a `Binding` where the control edits a value, and
//! an action where it fires one.

use super::configured::Configured;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_index, as_position, label, text, to_position};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    CommandId, ControlKind, ControlValue, DialogButton, MenuItem, ScrollOrientation, ToastPosition,
    Toasts, ToolButtonMenu,
};

/// The command id that view buttons report; views run closures instead.
const NO_COMMAND: CommandId = CommandId(0);

fn as_number(value: &ControlValue) -> Option<f64> {
    match value {
        ControlValue::Number(number) if number.is_finite() => Some(*number),
        _ => None,
    }
}

fn as_bool(value: &ControlValue) -> Option<bool> {
    match value {
        ControlValue::Bool(value) => Some(*value),
        _ => None,
    }
}

/// Overlapping avatars named `name`, for example the members of a team.
/// Use `.with(|group| group.with_count("+3"))` for a count after them and
/// `.with(|group| group.with_size(size))` for their size.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{AvatarGroup, View};
/// use choscorui_controls::Avatar;
///
/// fn team() -> impl View {
///     AvatarGroup::new(
///         "Team",
///         vec![Avatar::new("Ana Lee", "AL"), Avatar::new("Bao Tran", "BT")],
///     )
/// }
/// ```
pub type AvatarGroup = Configured<controls::AvatarGroup>;

impl Configured<controls::AvatarGroup> {
    /// A group named `name` that shows `avatars` in an overlapped row.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        avatars: impl Items<controls::Avatar>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Avatar, move |env| {
            controls::AvatarGroup::new(text(&name, env), avatars.items(env))
        })
    }
}

/// The semantics of a stack of `pages` pages, which shows page `current`.
/// It paints nothing: put it in a `ZStack` with the view of the current
/// page, as a `Tabs` view sits next to its panel. No pages report a build
/// error, and a page past the end keeps the page that it shows.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, Dynamic, StackedContainer, Text, View, ZStack};
///
/// struct Wizard;
///
/// impl View for Wizard {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let step = cx.state(0_usize);
///         ZStack::new((
///             StackedContainer::new("Steps", 3, step),
///             Dynamic::new(move || Text::new(format!("Step {}", step.get() + 1))),
///         ))
///     }
/// }
/// ```
pub type StackedContainer = Configured<controls::HostedStack>;

impl Configured<controls::HostedStack> {
    /// A stack named `name` with `pages` pages that shows page `current`.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        pages: usize,
        current: impl IntoLive<usize, N>,
    ) -> Self {
        let name = label(name);
        Configured::try_custom(ControlKind::Card, move |env| {
            controls::StackedContainer::hosted(text(&name, env), pages, 0)
        })
        .value(current.into_live().map(to_position))
    }
}

/// A titled frame around related controls, named by `title`. It paints
/// the frame and the title, and takes no input: put the members over it
/// in a `ZStack`, with padding that clears the title.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Binding, GroupBox, Switch, View, ViewExt, ZStack};
/// use choscorui_core::Space;
///
/// fn shipping(express: Binding<bool>) -> impl View {
///     ZStack::new((
///         GroupBox::new("Shipping"),
///         Switch::new("Express", express).padding(Space::S8),
///     ))
/// }
/// ```
pub type GroupBox = Configured<controls::HostedGroupBox>;

impl Configured<controls::HostedGroupBox> {
    /// A group box with `title`.
    pub fn new<M>(title: impl IntoLive<TextContent, M>) -> Self {
        let title = label(title);
        Configured::custom(ControlKind::Card, move |env| {
            controls::GroupBox::hosted(text(&title, env))
        })
    }
}

/// Pages under stacked headers, with one page open; `open` edits the
/// index of the open page. Up and Down move between the headers, and
/// Space or Return opens one. The tool box paints the headers and leaves
/// the open page's area for the application.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, ToolBox, View};
///
/// struct Settings;
///
/// impl View for Settings {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(0_usize);
///         ToolBox::new(vec!["General".to_owned(), "Advanced".to_owned()], open.binding())
///     }
/// }
/// ```
pub type ToolBox = Configured<controls::ToolBox>;

impl Configured<controls::ToolBox> {
    /// A tool box with a page for each title, and `open` as the open page.
    pub fn new(titles: impl Items<String>, open: Binding<usize>) -> Self {
        Configured::custom(ControlKind::Accordion, move |env| {
            controls::ToolBox::new(titles.items(env))
        })
        .bind(open, to_position, as_position)
    }
}

/// A standalone scroll bar named `name` over `content` points, of which
/// `viewport` show; `offset` edits the scroll position. The arrow keys
/// move one line, 16 points until `.line_step` changes it.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, ScrollBar, View};
/// use choscorui_controls::ScrollOrientation;
///
/// struct Log;
///
/// impl View for Log {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let top = cx.state(0.0);
///         ScrollBar::new("Log", ScrollOrientation::Vertical, 1000.0, 200.0, top.binding())
///             .line_step(20.0)
///     }
/// }
/// ```
pub type ScrollBar = Configured<controls::ScrollBar>;

impl Configured<controls::ScrollBar> {
    /// A scroll bar named `name` for `content` in a `viewport`, with its position in `offset`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        orientation: ScrollOrientation,
        content: f64,
        viewport: f64,
        offset: Binding<f64>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ScrollArea, move |env| {
            let mut bar = controls::ScrollBar::new(text(&name, env), orientation);
            bar.set_range(content, viewport, viewport);
            bar
        })
        .bind(offset, ControlValue::Number, as_number)
    }

    /// The distance of one arrow key.
    #[must_use]
    pub fn line_step(self, step: f64) -> Self {
        self.with(move |mut bar| {
            bar.set_line_step(step);
            bar
        })
    }
}

/// The toasts of `toasts` in a stack at a corner of this view, named
/// `name`. Each toast has a close button labeled `close`. Use it to show
/// a queue in one place; the `toaster` modifier puts one above the whole
/// window instead.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Toaster, View};
/// use choscorui_controls::{Toast, ToastPosition, Toasts};
///
/// fn notifications(toasts: Toasts) -> impl View {
///     toasts.push(Toast::new("Event created"));
///     Toaster::new("Notifications", toasts, "Close").position(ToastPosition::BottomEnd)
/// }
/// ```
pub type Toaster = Configured<controls::Toaster>;

impl Configured<controls::Toaster> {
    /// A toaster named `name` that shows `toasts`, with `close` as the name of each close button.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        toasts: Toasts,
        close: impl IntoLive<TextContent, N>,
    ) -> Self {
        let (name, close) = (label(name), label(close));
        Configured::custom(ControlKind::Toast, move |env| {
            controls::Toaster::new(text(&name, env), toasts.clone(), text(&close, env))
        })
    }

    /// The corner of the stack.
    #[must_use]
    pub fn position(self, position: ToastPosition) -> Self {
        self.with(move |toaster| toaster.with_position(position))
    }
}

/// A tool bar button that runs `action`. `.checked` makes it a toggle,
/// and `.menu` adds a menu; the button then reports `OpenChanged`.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, ToolButton, View};
///
/// struct Format;
///
/// impl View for Format {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let bold = cx.state(false);
///         ToolButton::new("Bold", || {}).checked(bold.binding())
///     }
/// }
/// ```
pub type ToolButton = Configured<controls::HostedToolButton>;

impl Configured<controls::HostedToolButton> {
    /// A tool button with `label` that runs `action`.
    pub fn new<M>(label: impl IntoLive<TextContent, M>, action: impl Fn() + 'static) -> Self {
        let label = self::label(label);
        Configured::custom(ControlKind::Toggle, move |env| {
            controls::ToolButton::new(text(&label, env), NO_COMMAND).hosted()
        })
        .on_command(move |_| action())
    }

    /// Makes the button a toggle whose state `checked` edits.
    #[must_use]
    pub fn checked(self, checked: Binding<bool>) -> Self {
        self.with(|button| button.with_checkable(true))
            .bind(checked, ControlValue::Bool, as_bool)
    }

    /// Gives the button a menu: an instant menu opens on a press, and a
    /// split menu opens from its arrow.
    #[must_use]
    pub fn menu(self, menu: ToolButtonMenu) -> Self {
        self.with(move |button| button.with_menu(menu))
    }
}

/// A command link: an arrow, a `title`, and a `description` line in one
/// button, which runs `action`.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{CommandLink, View};
///
/// fn install() -> impl View {
///     CommandLink::new("Install", "Add the app to the dock", || {})
/// }
/// ```
pub type CommandLink = Configured<controls::HostedCommandLink>;

impl Configured<controls::HostedCommandLink> {
    /// A command link with `title` and `description` that runs `action`.
    pub fn new<M, N>(
        title: impl IntoLive<TextContent, M>,
        description: impl IntoLive<TextContent, N>,
        action: impl Fn() + 'static,
    ) -> Self {
        let (title, description) = (label(title), label(description));
        Configured::custom(ControlKind::Item, move |env| {
            controls::CommandLink::new(text(&title, env), text(&description, env))
                .hosted(NO_COMMAND)
        })
        .on_activate(action)
    }
}

/// A modal alert with a `title`, a `message`, and ordered `buttons`,
/// open while `presented` is true. The chosen button reports its index
/// through `.on_item`, and Escape runs `.on_dismiss`; both close the
/// alert.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{AlertDialog, Cx, View};
/// use choscorui_controls::{DialogButton, DialogButtonRole};
///
/// struct Delete;
///
/// impl View for Delete {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let open = cx.state(false);
///         let buttons = vec![
///             DialogButton::new("Cancel", DialogButtonRole::Reject),
///             DialogButton::new("Delete", DialogButtonRole::Destructive),
///         ];
///         AlertDialog::new("Delete file?", "You cannot undo this.", buttons, open.binding())
///             .on_item(|index| println!("chose {index}"))
///     }
/// }
/// ```
pub type AlertDialog = Configured<controls::HostedAlertDialog>;

impl Configured<controls::HostedAlertDialog> {
    /// An alert with `title`, `message`, and `buttons` that shows while `presented` is true.
    pub fn new<M, N>(
        title: impl IntoLive<TextContent, M>,
        message: impl IntoLive<TextContent, N>,
        buttons: impl Items<DialogButton>,
        presented: Binding<bool>,
    ) -> Self {
        let (title, message) = (label(title), label(message));
        Configured::custom(ControlKind::AlertDialog, move |env| {
            controls::AlertDialog::new(text(&title, env), text(&message, env), buttons.items(env))
                .hosted()
        })
        .open(presented)
    }

    /// Informative text under the message.
    #[must_use]
    pub fn detail<M>(self, detail: impl IntoLive<TextContent, M>) -> Self {
        let detail = label(detail);
        self.with_env(move |alert, env| alert.with_detail(text(&detail, env)))
    }
}

/// A button named `name` that opens a menu of `items`. A chosen action
/// reports its command through `.on_command`. Set the enabled and checked
/// state of an item with `.with(|menu| …)` and
/// `HostedDropdownMenu::set_command`.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{DropdownMenu, View};
/// use choscorui_controls::{CommandId, MenuItem};
///
/// fn edit() -> impl View {
///     DropdownMenu::new(
///         "Edit",
///         vec![
///             MenuItem::action("Copy", CommandId(1)),
///             MenuItem::action("Paste", CommandId(2)),
///         ],
///     )
///     .on_command(|command| println!("ran {command:?}"))
/// }
/// ```
pub type DropdownMenu = Configured<controls::HostedDropdownMenu>;

impl Configured<controls::HostedDropdownMenu> {
    /// A menu named `name` that shows `items`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, items: impl Items<MenuItem>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Menu, move |env| {
            controls::DropdownMenu::new(items.items(env)).hosted(text(&name, env))
        })
    }
}

/// A font family combobox over `families`; `selected` edits the index of
/// the chosen family. A typed family that is not in the list selects
/// nothing, shows the invalid look, and renders with `fallback`.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, FontChooser, View};
///
/// struct Font;
///
/// impl View for Font {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let family = cx.state(Some(0_usize));
///         let families = vec!["Georgia".to_owned(), "Menlo".to_owned()];
///         FontChooser::new(families, "Helvetica", family.binding())
///     }
/// }
/// ```
pub type FontChooser = Configured<controls::FontChooser>;

impl Configured<controls::FontChooser> {
    /// A font list of `families`, with `fallback` as the default family and `selected` as the selected row.
    pub fn new(
        families: impl Items<String>,
        fallback: impl Into<String>,
        selected: Binding<Option<usize>>,
    ) -> Self {
        let fallback = fallback.into();
        Configured::custom(ControlKind::Combobox, move |env| {
            let families = families.items(env);
            // The view gives each new chooser the bound value, so the
            // build does not depend on it.
            let chosen = Runtime::untracked(|| selected.try_get()).ok().flatten();
            let current = chosen
                .and_then(|index| families.get(index))
                .cloned()
                .unwrap_or_default();
            controls::FontChooser::new(families, &current, fallback.clone())
        })
        .bind(selected, ControlValue::Index, as_index)
    }
}
