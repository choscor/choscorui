use choscorui_controls::{
    CommandId, CommandRegistry, CommandState, Shortcut, ShortcutModifiers, ShortcutScope,
};
use choscorui_core::{Key, Point, Rect, Size, Tree};

#[test]
fn focused_then_window_then_application_shortcut_wins() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));
    let focused = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    let chord = Shortcut {
        key: Key::Other(4),
        modifiers: ShortcutModifiers {
            command: true,
            ..Default::default()
        },
    };
    let mut commands = CommandRegistry::new();
    for id in [CommandId(1), CommandId(2), CommandId(3)] {
        commands.insert(id, CommandState::default()).unwrap();
    }
    commands
        .bind_shortcut(ShortcutScope::Application, chord, CommandId(1))
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Window(10), chord, CommandId(2))
        .unwrap();
    commands
        .bind_shortcut(
            ShortcutScope::Focused {
                window: 10,
                node: focused,
            },
            chord,
            CommandId(3),
        )
        .unwrap();
    assert_eq!(
        commands.resolve_shortcut(chord, 10, Some(focused)),
        Some(CommandId(3))
    );
    assert_eq!(
        commands.resolve_shortcut(chord, 10, None),
        Some(CommandId(2))
    );
    assert_eq!(
        commands.resolve_shortcut(chord, 11, Some(focused)),
        Some(CommandId(1))
    );
}

#[test]
fn duplicate_chord_in_same_scope_fails_and_disabled_winner_blocks_fallback() {
    let chord = Shortcut {
        key: Key::Other(4),
        modifiers: ShortcutModifiers {
            command: true,
            ..Default::default()
        },
    };
    let mut commands = CommandRegistry::new();
    commands
        .insert(CommandId(1), CommandState::default())
        .unwrap();
    commands
        .insert(CommandId(2), CommandState::default())
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Application, chord, CommandId(1))
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Window(10), chord, CommandId(2))
        .unwrap();
    assert!(
        commands
            .bind_shortcut(ShortcutScope::Window(10), chord, CommandId(1))
            .is_err()
    );
    assert!(
        commands
            .bind_shortcut(
                ShortcutScope::Window(10),
                Shortcut {
                    key: Key::Other(5),
                    modifiers: chord.modifiers
                },
                CommandId(99)
            )
            .is_err()
    );
    commands.set_enabled(CommandId(2), false).unwrap();
    assert_eq!(commands.resolve_shortcut(chord, 10, None), None);
    assert_eq!(
        commands.resolve_shortcut(chord, 11, None),
        Some(CommandId(1))
    );
    assert_eq!(
        commands.resolve_shortcut(
            Shortcut {
                key: Key::Other(4),
                modifiers: ShortcutModifiers::default()
            },
            11,
            None
        ),
        None
    );
}

#[test]
fn retiring_a_scope_reveals_the_less_specific_binding() {
    let chord = Shortcut {
        key: Key::Escape,
        modifiers: ShortcutModifiers::default(),
    };
    let mut commands = CommandRegistry::new();
    commands
        .insert(CommandId(1), CommandState::default())
        .unwrap();
    commands
        .insert(CommandId(2), CommandState::default())
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Application, chord, CommandId(1))
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Window(10), chord, CommandId(2))
        .unwrap();
    assert_eq!(
        commands.resolve_shortcut(chord, 10, None),
        Some(CommandId(2))
    );
    assert_eq!(commands.unbind_scope(ShortcutScope::Window(10)), 1);
    assert_eq!(
        commands.resolve_shortcut(chord, 10, None),
        Some(CommandId(1))
    );
    assert_eq!(commands.unbind_scope(ShortcutScope::Window(10)), 0);
}

#[test]
fn retiring_window_removes_its_window_and_focused_bindings() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));
    let node = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    let chord = Shortcut {
        key: Key::Escape,
        modifiers: ShortcutModifiers::default(),
    };
    let mut commands = CommandRegistry::new();
    commands
        .insert(CommandId(1), CommandState::default())
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Window(10), chord, CommandId(1))
        .unwrap();
    commands
        .bind_shortcut(
            ShortcutScope::Focused { window: 10, node },
            chord,
            CommandId(1),
        )
        .unwrap();
    commands
        .bind_shortcut(ShortcutScope::Window(11), chord, CommandId(1))
        .unwrap();
    assert_eq!(commands.unbind_window(10), 2);
    assert_eq!(commands.resolve_shortcut(chord, 10, Some(node)), None);
    assert_eq!(
        commands.resolve_shortcut(chord, 11, None),
        Some(CommandId(1))
    );
}
