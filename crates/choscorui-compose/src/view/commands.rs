//! Command menus: the menu bar of an application, declared with the
//! [`MenuChoice`] rows that menus in views use. The application lowers
//! them to the core menu model, with a generated id for each item, and
//! runs the item's action when the user chooses its id. The lowering
//! reads the live labels and states, so a change of what it read makes
//! the menu bar again.

use super::env::Env;
use super::menus::{MenuChoice, Row};
use super::state::{IntoLive, Live};
use super::text::TextContent;
use super::widgets::Items;
use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec,
};
use std::rc::Rc;

/// One menu of the menu bar, with `MenuChoice` rows: actions, check and
/// radio rows, submenus, and separators. A row's
/// [`MenuChoice::keyboard_shortcut`] chooses it from the keyboard.
#[derive(Clone)]
pub struct CommandMenu {
    title: Live<TextContent>,
    items: Rc<dyn Items<MenuChoice>>,
}

impl CommandMenu {
    /// A menu titled `title` with the rows that `items` gives. The first
    /// menu is the application menu on macOS.
    pub fn new<M>(title: impl IntoLive<TextContent, M>, items: impl Items<MenuChoice>) -> Self {
        Self {
            title: title.into_live(),
            items: Rc::new(items),
        }
    }
}

/// Command menus lowered to the core menu model. Each action and check
/// row has a generated id, in menu order from 1.
pub struct LoweredMenus {
    model: MenuBarModel,
    actions: Vec<Rc<dyn Fn()>>,
}

impl LoweredMenus {
    /// Lowers `menus` in `env`. Inside a live property or effect, the
    /// reads of labels and states are recorded.
    pub fn lower(menus: &[CommandMenu], env: &Env) -> Result<Self, MenuError> {
        let mut actions = Vec::new();
        let specs = menus
            .iter()
            .map(|menu| {
                let entries = lower_rows(&menu.items.items(env), env, &mut actions);
                MenuSpec::new(menu.title.get().resolve(env), entries)
            })
            .collect();
        Ok(Self {
            model: MenuBarModel::new(specs)?,
            actions,
        })
    }

    /// The menu bar model.
    pub fn model(&self) -> &MenuBarModel {
        &self.model
    }

    /// The action of the item with `id`, if any.
    pub fn action(&self, id: MenuCommandId) -> Option<Rc<dyn Fn()>> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.actions.get(index).cloned()
    }
}

fn lower_rows(rows: &[MenuChoice], env: &Env, actions: &mut Vec<Rc<dyn Fn()>>) -> Vec<MenuEntry> {
    rows.iter()
        .map(|choice| {
            let (label, checked, action) = match &choice.row {
                Row::Action(label, action) => (label, false, action),
                Row::Check(label, _, checked, action) => (label, checked(), action),
                Row::Submenu(label, items) => {
                    return MenuEntry::submenu(
                        label.get().resolve(env),
                        lower_rows(items, env, actions),
                    );
                }
                Row::Separator => return MenuEntry::Separator,
            };
            actions.push(action.clone());
            let shortcut = choice.keys.map(|(key, modifiers)| MenuShortcut {
                key: key.to_string(),
                modifiers: MenuShortcutModifiers {
                    shift: modifiers.shift,
                    control: modifiers.control,
                    option: modifiers.option,
                    command: modifiers.command,
                },
            });
            MenuEntry::command(
                MenuCommandId(actions.len() as u64),
                MenuItemState {
                    title: label.get().resolve(env),
                    enabled: !choice.disabled.get(),
                    checked,
                    shortcut,
                },
            )
        })
        .collect()
}
