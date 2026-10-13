use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec,
};

fn save_item() -> MenuEntry {
    MenuEntry::command(
        MenuCommandId(7),
        MenuItemState {
            title: "Save".into(),
            enabled: true,
            checked: false,
            shortcut: Some(MenuShortcut {
                key: "s".into(),
                modifiers: MenuShortcutModifiers {
                    command: true,
                    ..MenuShortcutModifiers::default()
                },
            }),
        },
    )
}

#[test]
fn menu_model_owns_state_and_updates_by_command_identity() {
    let mut model = MenuBarModel::new(vec![MenuSpec::new(
        "File",
        vec![save_item(), MenuEntry::separator()],
    )])
    .unwrap();

    model
        .set_state(
            MenuCommandId(7),
            MenuItemState {
                title: "Save As…".into(),
                enabled: false,
                checked: true,
                shortcut: None,
            },
        )
        .unwrap();

    let item = model.state(MenuCommandId(7)).unwrap();
    assert_eq!(item.title, "Save As…");
    assert!(!item.enabled);
    assert!(item.checked);
    assert_eq!(item.shortcut, None);
}

#[test]
fn menu_model_rejects_duplicate_commands_and_invalid_text() {
    assert_eq!(
        MenuBarModel::new(vec![MenuSpec::new("File", vec![save_item(), save_item()])]),
        Err(MenuError::DuplicateCommand(MenuCommandId(7)))
    );
    assert_eq!(
        MenuBarModel::new(vec![MenuSpec::new("Bad\0title", vec![])]),
        Err(MenuError::InvalidText)
    );
    let mut model = MenuBarModel::new(vec![MenuSpec::new("File", vec![save_item()])]).unwrap();
    assert_eq!(
        model.set_state(MenuCommandId(99), MenuItemState::new("Missing")),
        Err(MenuError::UnknownCommand(MenuCommandId(99)))
    );
}

#[test]
fn menu_model_preserves_menu_and_separator_order() {
    let model = MenuBarModel::new(vec![
        MenuSpec::new("File", vec![save_item(), MenuEntry::separator()]),
        MenuSpec::new(
            "View",
            vec![MenuEntry::command(
                MenuCommandId(8),
                MenuItemState::new("Sidebar"),
            )],
        ),
    ])
    .unwrap();
    assert_eq!(model.menus()[0].title, "File");
    assert!(matches!(model.menus()[0].entries[1], MenuEntry::Separator));
    assert_eq!(model.menus()[1].title, "View");
}

#[test]
fn submenus_hold_commands_that_the_model_finds_and_updates() {
    let mut model = MenuBarModel::new(vec![MenuSpec::new(
        "View",
        vec![MenuEntry::submenu(
            "Theme",
            vec![
                MenuEntry::command(MenuCommandId(1), MenuItemState::new("Dark")),
                MenuEntry::command(MenuCommandId(2), MenuItemState::new("Light")),
            ],
        )],
    )])
    .unwrap();
    assert_eq!(model.find("Light"), Some(MenuCommandId(2)));
    assert_eq!(model.find("Missing"), None);
    let checked = MenuItemState {
        checked: true,
        ..MenuItemState::new("Light")
    };
    model.set_state(MenuCommandId(2), checked).unwrap();
    assert!(model.state(MenuCommandId(2)).unwrap().checked);
    assert_eq!(
        MenuBarModel::new(vec![MenuSpec::new(
            "View",
            vec![
                save_item(),
                MenuEntry::submenu("More", vec![save_item()]),
            ],
        )]),
        Err(MenuError::DuplicateCommand(MenuCommandId(7)))
    );
    assert_eq!(
        MenuBarModel::new(vec![MenuSpec::new(
            "View",
            vec![MenuEntry::submenu("Bad\0", vec![])],
        )]),
        Err(MenuError::InvalidText)
    );
}
