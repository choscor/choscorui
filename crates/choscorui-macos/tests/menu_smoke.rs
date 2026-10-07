use choscorui_macos::{
    Application, MenuBarModel, MenuCommandId, MenuEntry, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec,
};

fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }

    let app = Application::new().expect("AppKit must initialize");
    let command = MenuCommandId(41);
    let model = MenuBarModel::new(vec![MenuSpec::new(
        "File",
        vec![MenuEntry::command(
            command,
            MenuItemState {
                title: "Open…".into(),
                enabled: true,
                checked: false,
                shortcut: Some(MenuShortcut {
                    key: "o".into(),
                    modifiers: MenuShortcutModifiers {
                        command: true,
                        ..MenuShortcutModifiers::default()
                    },
                }),
            },
        )],
    )])
    .unwrap();
    let mut menu = app
        .install_main_menu(model)
        .expect("main menu must install");

    assert!(
        menu.perform_action(command)
            .expect("native target/action must run")
    );
    assert_eq!(menu.take_activations(), vec![command]);
    assert!(menu.take_activations().is_empty());

    menu.set_state(
        command,
        MenuItemState {
            title: "Open Recent".into(),
            enabled: false,
            checked: true,
            shortcut: None,
        },
    )
    .expect("native state update must succeed");
    let state = menu.model().state(command).unwrap();
    assert_eq!(state.title, "Open Recent");
    assert!(!state.enabled);
    assert!(state.checked);
    assert!(!menu.perform_action(command).unwrap());
    assert!(menu.take_activations().is_empty());
    assert_eq!(menu.callback_panic_count(), 0);
}
