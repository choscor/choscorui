use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MenuCommandId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MenuShortcutModifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuShortcut {
    pub key: String,
    pub modifiers: MenuShortcutModifiers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuItemState {
    pub title: String,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: Option<MenuShortcut>,
}

impl MenuItemState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            enabled: true,
            checked: false,
            shortcut: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MenuEntry {
    Command {
        command: MenuCommandId,
        state: MenuItemState,
    },
    Separator,
}

impl MenuEntry {
    pub fn command(command: MenuCommandId, state: MenuItemState) -> Self {
        Self::Command { command, state }
    }

    pub const fn separator() -> Self {
        Self::Separator
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuSpec {
    pub title: String,
    pub entries: Vec<MenuEntry>,
}

impl MenuSpec {
    pub fn new(title: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self {
            title: title.into(),
            entries,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuError {
    DuplicateCommand(MenuCommandId),
    UnknownCommand(MenuCommandId),
    InvalidText,
    AppKitUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuBarModel {
    menus: Vec<MenuSpec>,
    locations: HashMap<MenuCommandId, (usize, usize)>,
}

impl MenuBarModel {
    pub fn new(menus: Vec<MenuSpec>) -> Result<Self, MenuError> {
        let mut locations = HashMap::new();
        for (menu_index, menu) in menus.iter().enumerate() {
            validate_text(&menu.title)?;
            for (entry_index, entry) in menu.entries.iter().enumerate() {
                if let MenuEntry::Command { command, state } = entry {
                    validate_state(state)?;
                    if locations
                        .insert(*command, (menu_index, entry_index))
                        .is_some()
                    {
                        return Err(MenuError::DuplicateCommand(*command));
                    }
                }
            }
        }
        Ok(Self { menus, locations })
    }

    pub fn menus(&self) -> &[MenuSpec] {
        &self.menus
    }

    pub fn state(&self, command: MenuCommandId) -> Result<&MenuItemState, MenuError> {
        let &(menu, entry) = self
            .locations
            .get(&command)
            .ok_or(MenuError::UnknownCommand(command))?;
        match &self.menus[menu].entries[entry] {
            MenuEntry::Command { state, .. } => Ok(state),
            MenuEntry::Separator => unreachable!("locations only index commands"),
        }
    }

    pub fn set_state(
        &mut self,
        command: MenuCommandId,
        state: MenuItemState,
    ) -> Result<(), MenuError> {
        validate_state(&state)?;
        let &(menu, entry) = self
            .locations
            .get(&command)
            .ok_or(MenuError::UnknownCommand(command))?;
        match &mut self.menus[menu].entries[entry] {
            MenuEntry::Command { state: current, .. } => *current = state,
            MenuEntry::Separator => unreachable!("locations only index commands"),
        }
        Ok(())
    }
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
