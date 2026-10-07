use choscorui_controls::{
    Button, ButtonInput, CommandId, CommandRegistry, CommandState, SemanticAction,
};
use choscorui_core::{Point, Rect, Size, Tree};

#[test]
fn command_state_is_shared_and_disabled_commands_cannot_execute() {
    let mut commands = CommandRegistry::new();
    let id = CommandId(42);
    commands
        .insert(
            id,
            CommandState {
                enabled: true,
                checked: false,
            },
        )
        .unwrap();
    assert!(!commands.state(id).unwrap().checked);
    assert_eq!(commands.activate(id).unwrap(), Some(id));
    commands.set_checked(id, true).unwrap();
    assert!(commands.state(id).unwrap().checked);
    commands.set_enabled(id, false).unwrap();
    assert_eq!(commands.activate(id).unwrap(), None);
    assert!(!commands.state(id).unwrap().enabled);
}

#[test]
fn duplicate_and_unknown_identifiers_fail_explicitly() {
    let mut commands = CommandRegistry::new();
    let id = CommandId(7);
    commands.insert(id, CommandState::default()).unwrap();
    assert!(commands.insert(id, CommandState::default()).is_err());
    assert!(commands.activate(CommandId(8)).is_err());
    assert!(commands.set_checked(CommandId(8), true).is_err());
}

#[test]
fn button_projects_command_enabled_state_into_input_and_semantics() {
    let mut commands = CommandRegistry::new();
    let id = CommandId(3);
    commands.insert(id, CommandState::default()).unwrap();
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0)));
    let node = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(50.0, 30.0)),
        )
        .unwrap();
    let mut button = Button::new("Run", id);
    commands.set_enabled(id, false).unwrap();
    button.sync_command(&mut tree, node, &commands).unwrap();
    assert!(!button.semantics(&tree, node).enabled);
    assert_eq!(
        button
            .handle(
                &mut tree,
                node,
                ButtonInput::Semantic(SemanticAction::Activate)
            )
            .unwrap(),
        None
    );
    commands.set_enabled(id, true).unwrap();
    button.sync_command(&mut tree, node, &commands).unwrap();
    assert_eq!(
        button
            .handle(
                &mut tree,
                node,
                ButtonInput::Semantic(SemanticAction::Activate)
            )
            .unwrap(),
        Some(id)
    );
}

#[test]
fn command_bound_button_reads_current_state_without_manual_sync() {
    let mut commands = CommandRegistry::new();
    let id = CommandId(9);
    commands.insert(id, CommandState::default()).unwrap();
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0)));
    let node = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(50.0, 30.0)),
        )
        .unwrap();
    let mut button = Button::new("Run", id);
    commands.set_enabled(id, false).unwrap();
    assert!(
        !button
            .semantics_with_commands(&tree, node, &commands)
            .unwrap()
            .enabled
    );
    assert!(
        !button
            .visual_state_with_commands(&tree, node, &commands)
            .unwrap()
            .enabled
    );
    assert_eq!(
        button
            .handle_with_commands(
                &mut tree,
                node,
                ButtonInput::Semantic(SemanticAction::Activate),
                &commands
            )
            .unwrap(),
        None
    );
    commands.set_enabled(id, true).unwrap();
    assert_eq!(
        button
            .semantics_with_commands(&tree, node, &commands)
            .unwrap()
            .actions,
        &[SemanticAction::Activate]
    );
    assert_eq!(
        button
            .handle_with_commands(
                &mut tree,
                node,
                ButtonInput::Semantic(SemanticAction::Activate),
                &commands
            )
            .unwrap(),
        Some(id)
    );
}
