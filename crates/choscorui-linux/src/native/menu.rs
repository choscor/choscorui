//! The command menus. Linux has no global menu bar: the facade draws the
//! menus of `MainMenu` in each window, and shows a `ContextMenu` in a
//! popup. Both keep the macOS contract: a chosen command waits in a
//! queue until the application takes it.

use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuError, MenuItemState, MenuShortcut, MenuShortcutModifiers,
    MenuSpec,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::rc::Rc;

/// The model of a menu and the commands that the user chose.
struct Menus {
    model: MenuBarModel,
    activations: VecDeque<MenuCommandId>,
}

impl Menus {
    fn perform(&mut self, command: MenuCommandId) -> Result<bool, MenuError> {
        let enabled = self.model.state(command)?.enabled;
        if enabled {
            self.activations.push_back(command);
        }
        Ok(enabled)
    }
}

/// The menus of the menu bar. Clones share the model and the queue.
#[derive(Clone)]
pub struct MainMenu {
    menus: Rc<RefCell<Menus>>,
    _ui_thread: PhantomData<Rc<()>>,
}

impl MainMenu {
    pub(super) fn new(model: MenuBarModel) -> Self {
        Self {
            menus: Rc::new(RefCell::new(Menus {
                model,
                activations: VecDeque::new(),
            })),
            _ui_thread: PhantomData,
        }
    }

    /// A copy of the model with the current item states.
    pub fn model(&self) -> MenuBarModel {
        self.menus.borrow().model.clone()
    }

    /// Replaces the state of `command`.
    pub fn set_state(&self, command: MenuCommandId, state: MenuItemState) -> Result<(), MenuError> {
        self.menus.borrow_mut().model.set_state(command, state)
    }

    /// The commands that the user chose since the last call, in order.
    pub fn take_activations(&self) -> Vec<MenuCommandId> {
        self.menus.borrow_mut().activations.drain(..).collect()
    }

    /// Chooses `command` as a click does. An enabled item queues its
    /// activation and returns true; a disabled item returns false.
    pub fn perform_action(&self, command: MenuCommandId) -> Result<bool, MenuError> {
        self.menus.borrow_mut().perform(command)
    }

    /// Runs the command whose shortcut is `key` with `modifiers`, as a
    /// key press does, and returns whether an enabled item took it. The
    /// item's command then waits in [`MainMenu::take_activations`]. The
    /// modifiers use the menu terms: `command` is Ctrl. Fails for a key
    /// with a NUL character.
    pub fn perform_key_equivalent(
        &self,
        key: &str,
        modifiers: MenuShortcutModifiers,
    ) -> Result<bool, MenuError> {
        if key.contains('\0') {
            return Err(MenuError::InvalidText);
        }
        let shortcut = MenuShortcut {
            key: key.to_owned(),
            modifiers,
        };
        let mut menus = self.menus.borrow_mut();
        match menus.model.command_for_shortcut(&shortcut) {
            Some(command) => menus.perform(command),
            None => Ok(false),
        }
    }

    /// The panics that native menu callbacks caught. The menus run no
    /// native callbacks on Linux, so the count stays zero.
    pub fn callback_panic_count(&self) -> usize {
        0
    }
}

/// A menu that opens at a point. The facade shows it in a popup.
pub struct ContextMenu {
    menus: RefCell<Menus>,
    _ui_thread: PhantomData<Rc<()>>,
}

impl ContextMenu {
    /// A context menu of `spec`. Fails for text with a NUL character or
    /// a command that appears twice.
    pub fn new(spec: MenuSpec) -> Result<Self, MenuError> {
        Ok(Self {
            menus: RefCell::new(Menus {
                model: MenuBarModel::new(vec![spec])?,
                activations: VecDeque::new(),
            }),
            _ui_thread: PhantomData,
        })
    }

    /// The menu with the current item states.
    pub fn spec(&self) -> MenuSpec {
        self.menus.borrow().model.menus()[0].clone()
    }

    /// The number of entries at the top level.
    pub fn item_count(&self) -> usize {
        self.menus.borrow().model.menus()[0].entries.len()
    }

    /// Replaces the state of `command`.
    pub fn set_state(&self, command: MenuCommandId, state: MenuItemState) -> Result<(), MenuError> {
        self.menus.borrow_mut().model.set_state(command, state)
    }

    /// The commands that the user chose since the last call, in order.
    pub fn take_activations(&self) -> Vec<MenuCommandId> {
        self.menus.borrow_mut().activations.drain(..).collect()
    }

    /// Chooses `command` as a click does. An enabled item queues its
    /// activation and returns true; a disabled item returns false.
    pub fn perform_action(&self, command: MenuCommandId) -> Result<bool, MenuError> {
        self.menus.borrow_mut().perform(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choscorui_core::MenuEntry;

    fn spec() -> MenuSpec {
        MenuSpec::new(
            "File",
            vec![MenuEntry::command(
                MenuCommandId(41),
                MenuItemState {
                    shortcut: Some(MenuShortcut {
                        key: "o".into(),
                        modifiers: MenuShortcutModifiers {
                            command: true,
                            ..MenuShortcutModifiers::default()
                        },
                    }),
                    ..MenuItemState::new("Open…")
                },
            )],
        )
    }

    #[test]
    fn enabled_commands_queue_and_disabled_ones_do_not() {
        let menu = MainMenu::new(MenuBarModel::new(vec![spec()]).unwrap());
        let command = MenuCommandId(41);
        assert_eq!(menu.perform_action(command), Ok(true));
        let ctrl = MenuShortcutModifiers {
            command: true,
            ..MenuShortcutModifiers::default()
        };
        assert_eq!(menu.perform_key_equivalent("O", ctrl), Ok(true));
        assert_eq!(menu.perform_key_equivalent("p", ctrl), Ok(false));
        assert_eq!(
            menu.perform_key_equivalent("\0", ctrl),
            Err(MenuError::InvalidText)
        );
        assert_eq!(menu.take_activations(), vec![command, command]);
        assert!(menu.take_activations().is_empty());
        menu.set_state(
            command,
            MenuItemState {
                enabled: false,
                ..MenuItemState::new("Open Recent")
            },
        )
        .unwrap();
        assert_eq!(menu.perform_action(command), Ok(false));
        assert_eq!(
            menu.perform_action(MenuCommandId(9)),
            Err(MenuError::UnknownCommand(MenuCommandId(9)))
        );
        assert!(menu.take_activations().is_empty());
    }

    #[test]
    fn a_context_menu_keeps_its_own_queue() {
        let menu = ContextMenu::new(spec()).unwrap();
        assert_eq!(menu.item_count(), 1);
        assert_eq!(menu.perform_action(MenuCommandId(41)), Ok(true));
        assert_eq!(menu.take_activations(), vec![MenuCommandId(41)]);
        assert_eq!(menu.spec().title, "File");
    }
}
