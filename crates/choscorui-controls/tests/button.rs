use choscorui_controls::{Button, ButtonInput, CommandId, SemanticAction};
use choscorui_core::{Dirty, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect, Size, Tree};
use choscorui_core::{SemanticAction as CoreSemanticAction, SemanticRole};

fn setup() -> (Tree, choscorui_core::NodeId, Button) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(80.0, 40.0)),
        )
        .unwrap();
    (tree, id, Button::new("Run", CommandId(7)))
}

#[test]
fn pointer_capture_cancels_outside_and_activates_after_return() {
    let (mut tree, id, mut button) = setup();
    let down = PointerEvent::Down {
        position: Point::new(20.0, 20.0),
        button: PointerButton::Primary,
    };
    let outside = Point::new(150.0, 20.0);
    assert_eq!(
        button
            .handle(&mut tree, id, ButtonInput::Pointer(down))
            .unwrap(),
        None
    );
    assert_eq!(tree.focus(), Some(id));
    assert_eq!(tree.pointer_capture(), Some(id));
    button
        .handle(
            &mut tree,
            id,
            ButtonInput::Pointer(PointerEvent::Move { position: outside }),
        )
        .unwrap();
    assert!(!button.visual_state(&tree, id).pressed());
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Pointer(PointerEvent::Up {
                    position: outside,
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        None
    );
    assert_eq!(tree.pointer_capture(), None);
    button
        .handle(&mut tree, id, ButtonInput::Pointer(down))
        .unwrap();
    button
        .handle(
            &mut tree,
            id,
            ButtonInput::Pointer(PointerEvent::Move { position: outside }),
        )
        .unwrap();
    button
        .handle(
            &mut tree,
            id,
            ButtonInput::Pointer(PointerEvent::Move {
                position: Point::new(20.0, 20.0),
            }),
        )
        .unwrap();
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Pointer(PointerEvent::Up {
                    position: Point::new(20.0, 20.0),
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        Some(CommandId(7))
    );
}

#[test]
fn keyboard_and_semantic_actions_share_command() {
    let (mut tree, id, mut button) = setup();
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Key(KeyEvent::Down {
                    key: Key::Enter,
                    repeat: false
                })
            )
            .unwrap(),
        Some(CommandId(7))
    );
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Key(KeyEvent::Down {
                    key: Key::Enter,
                    repeat: true
                })
            )
            .unwrap(),
        None
    );
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Key(KeyEvent::Down {
                    key: Key::Space,
                    repeat: false
                })
            )
            .unwrap(),
        None
    );
    assert!(button.visual_state(&tree, id).pressed());
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Key(KeyEvent::Up { key: Key::Space })
            )
            .unwrap(),
        Some(CommandId(7))
    );
    tree.set_focus(None).unwrap();
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Semantic(SemanticAction::Activate)
            )
            .unwrap(),
        Some(CommandId(7))
    );
    let semantics = button.semantics(&tree, id);
    assert_eq!(semantics.name, "Run");
    assert_eq!(semantics.role, "button");
    assert!(!semantics.focused);
    assert!(semantics.enabled);
    let shared = semantics.shared();
    assert_eq!(shared.role, SemanticRole::Button);
    assert_eq!(
        shared.actions,
        vec![CoreSemanticAction::Activate, CoreSemanticAction::Focus]
    );
}

#[test]
fn disabling_cancels_capture_and_blocks_all_activation() {
    let (mut tree, id, mut button) = setup();
    button
        .handle(
            &mut tree,
            id,
            ButtonInput::Pointer(PointerEvent::Down {
                position: Point::new(20.0, 20.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    button.set_enabled(&mut tree, id, false).unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert!(!button.semantics(&tree, id).enabled);
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Semantic(SemanticAction::Activate)
            )
            .unwrap(),
        None
    );
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Key(KeyEvent::Down {
                    key: Key::Enter,
                    repeat: false
                })
            )
            .unwrap(),
        None
    );
}

#[test]
fn enabled_change_invalidates_paint_and_semantics() {
    let (mut tree, id, mut button) = setup();
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    button.set_enabled(&mut tree, id, false).unwrap();
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
}
