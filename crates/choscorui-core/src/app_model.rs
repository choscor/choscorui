//! Portable descriptions of application windows and the menu bar. A
//! backend receives them and maps them to native objects; the facade
//! builds them from the application's declarations.

use crate::Size;
use std::collections::HashMap;

/// The title and the content size of a window to open.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    title: String,
    width: f64,
    height: f64,
    minimum: Size,
}

impl WindowOptions {
    /// A window titled `title` with a content area of `width` by `height`
    /// points.
    pub fn new(title: impl Into<String>, width: f64, height: f64) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            minimum: Size::ZERO,
        }
    }

    /// The smallest size that a restored frame can shrink to when it
    /// does not fit on the screens.
    #[must_use]
    pub fn minimum_size(mut self, minimum: Size) -> Self {
        self.minimum = minimum;
        self
    }

    /// The title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The content width in points.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// The content height in points.
    pub fn height(&self) -> f64 {
        self.height
    }

    /// The smallest size of a restored frame.
    pub fn minimum(&self) -> Size {
        self.minimum
    }
}

/// Identifies one command item of a menu bar.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MenuCommandId(pub u64);

/// The modifier keys of a menu shortcut.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MenuShortcutModifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

/// The key of a menu item, as one character, and its modifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuShortcut {
    pub key: String,
    pub modifiers: MenuShortcutModifiers,
}

/// What a command item shows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuItemState {
    pub title: String,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: Option<MenuShortcut>,
}

impl MenuItemState {
    /// An enabled item titled `title`, without a check mark or shortcut.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            enabled: true,
            checked: false,
            shortcut: None,
        }
    }
}

/// One entry of a menu: a command, a submenu, or a separator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MenuEntry {
    Command {
        command: MenuCommandId,
        state: MenuItemState,
    },
    /// An item that opens `entries` beside the menu.
    Submenu {
        title: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

impl MenuEntry {
    /// A command item.
    pub fn command(command: MenuCommandId, state: MenuItemState) -> Self {
        Self::Command { command, state }
    }

    /// A submenu item titled `title`.
    pub fn submenu(title: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self::Submenu {
            title: title.into(),
            entries,
        }
    }

    /// A line between groups of items.
    pub const fn separator() -> Self {
        Self::Separator
    }
}

/// One menu of the menu bar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuSpec {
    pub title: String,
    pub entries: Vec<MenuEntry>,
}

impl MenuSpec {
    /// A menu titled `title` with `entries`.
    pub fn new(title: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self {
            title: title.into(),
            entries,
        }
    }
}

/// Why a menu bar model or a native menu failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuError {
    DuplicateCommand(MenuCommandId),
    UnknownCommand(MenuCommandId),
    InvalidText,
    /// The native menu system is not available.
    Unavailable,
}

impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCommand(command) => write!(f, "menu command {command:?} appears twice"),
            Self::UnknownCommand(command) => {
                write!(f, "menu command {command:?} is not in the menu bar")
            }
            Self::InvalidText => f.write_str("menu text contains an interior NUL character"),
            Self::Unavailable => f.write_str("the native menu system is unavailable"),
        }
    }
}

impl std::error::Error for MenuError {}

/// The menus of a menu bar, with each command's place in them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuBarModel {
    menus: Vec<MenuSpec>,
    /// The menu index, then the entry index at each submenu level.
    locations: HashMap<MenuCommandId, Vec<usize>>,
}

impl MenuBarModel {
    /// Checks `menus`: the text has no NUL character and no command id
    /// appears twice.
    pub fn new(menus: Vec<MenuSpec>) -> Result<Self, MenuError> {
        let mut locations = HashMap::new();
        for (menu_index, menu) in menus.iter().enumerate() {
            validate_text(&menu.title)?;
            index_entries(&menu.entries, &mut vec![menu_index], &mut locations)?;
        }
        Ok(Self { menus, locations })
    }

    /// The menus, in menu bar order.
    pub fn menus(&self) -> &[MenuSpec] {
        &self.menus
    }

    /// The state of `command`.
    pub fn state(&self, command: MenuCommandId) -> Result<&MenuItemState, MenuError> {
        let path = self
            .locations
            .get(&command)
            .ok_or(MenuError::UnknownCommand(command))?;
        match entry_at(&self.menus[path[0]].entries, &path[1..]) {
            Some(MenuEntry::Command { state, .. }) => Ok(state),
            _ => Err(MenuError::UnknownCommand(command)),
        }
    }

    /// Replaces the state of `command`.
    pub fn set_state(
        &mut self,
        command: MenuCommandId,
        state: MenuItemState,
    ) -> Result<(), MenuError> {
        validate_state(&state)?;
        let path = self
            .locations
            .get(&command)
            .ok_or(MenuError::UnknownCommand(command))?;
        match entry_at_mut(&mut self.menus[path[0]].entries, &path[1..]) {
            Some(MenuEntry::Command { state: current, .. }) => {
                *current = state;
                Ok(())
            }
            _ => Err(MenuError::UnknownCommand(command)),
        }
    }

    /// The first command, in menu order, whose title is `title`.
    pub fn find(&self, title: &str) -> Option<MenuCommandId> {
        let mut found = None;
        for menu in &self.menus {
            found = found.or_else(|| find_in(&menu.entries, title));
        }
        found
    }
}

fn index_entries(
    entries: &[MenuEntry],
    path: &mut Vec<usize>,
    locations: &mut HashMap<MenuCommandId, Vec<usize>>,
) -> Result<(), MenuError> {
    for (index, entry) in entries.iter().enumerate() {
        path.push(index);
        match entry {
            MenuEntry::Command { command, state } => {
                validate_state(state)?;
                if locations.insert(*command, path.clone()).is_some() {
                    return Err(MenuError::DuplicateCommand(*command));
                }
            }
            MenuEntry::Submenu { title, entries } => {
                validate_text(title)?;
                index_entries(entries, path, locations)?;
            }
            MenuEntry::Separator => {}
        }
        path.pop();
    }
    Ok(())
}

fn entry_at<'a>(entries: &'a [MenuEntry], path: &[usize]) -> Option<&'a MenuEntry> {
    let (first, rest) = path.split_first()?;
    let entry = entries.get(*first)?;
    match (rest.is_empty(), entry) {
        (true, _) => Some(entry),
        (false, MenuEntry::Submenu { entries, .. }) => entry_at(entries, rest),
        (false, _) => None,
    }
}

fn entry_at_mut<'a>(entries: &'a mut [MenuEntry], path: &[usize]) -> Option<&'a mut MenuEntry> {
    let (first, rest) = path.split_first()?;
    let entry = entries.get_mut(*first)?;
    if rest.is_empty() {
        return Some(entry);
    }
    match entry {
        MenuEntry::Submenu { entries, .. } => entry_at_mut(entries, rest),
        _ => None,
    }
}

fn find_in(entries: &[MenuEntry], title: &str) -> Option<MenuCommandId> {
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Command { command, state } if state.title == title => Some(*command),
        MenuEntry::Submenu { entries, .. } => find_in(entries, title),
        _ => None,
    })
}

fn validate_state(state: &MenuItemState) -> Result<(), MenuError> {
    validate_text(&state.title)?;
    if let Some(shortcut) = &state.shortcut {
        validate_text(&shortcut.key)?;
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), MenuError> {
    if text.contains('\0') {
        Err(MenuError::InvalidText)
    } else {
        Ok(())
    }
}
