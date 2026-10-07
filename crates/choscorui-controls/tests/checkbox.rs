use choscorui_controls::{Checkbox, CheckboxInput, CheckboxStyle, SemanticAction};
use choscorui_core::{
    Color, Dirty, DisplayCommand, DisplayList, Key, KeyEvent, Point, PointerButton, PointerEvent,
    Rect, Size, Tree,
};
use choscorui_core::{SemanticAction as CoreSemanticAction, SemanticRole};

fn setup() -> (Tree, choscorui_core::NodeId, Checkbox) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(80.0, 40.0)),
        )
        .unwrap();
    (tree, id, Checkbox::new("Remember", false))
}

#[test]
fn pointer_release_inside_toggles_and_outside_cancels() {
    let (mut tree, id, mut checkbox) = setup();
    let inside = Point::new(20.0, 20.0);
    let outside = Point::new(150.0, 20.0);
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: inside,
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(tree.pointer_capture(), Some(id));
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Pointer(PointerEvent::Up {
                    position: outside,
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        None
    );
    assert!(!checkbox.value());
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: inside,
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Pointer(PointerEvent::Up {
                    position: inside,
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        Some(true)
    );
    assert_eq!(tree.pointer_capture(), None);
    assert!(checkbox.semantics(&tree, id).checked);
}

#[test]
fn space_and_semantic_set_value_share_state() {
    let (mut tree, id, mut checkbox) = setup();
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Down {
                    key: Key::Space,
                    repeat: false
                })
            )
            .unwrap(),
        None
    );
    assert!(checkbox.visual_state(&tree, id).pressed);
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Up { key: Key::Space })
            )
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(true))
            )
            .unwrap(),
        None
    );
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(false))
            )
            .unwrap(),
        Some(false)
    );
    assert_eq!(checkbox.semantics(&tree, id).name, "Remember");
    let shared = checkbox.semantics(&tree, id).shared();
    assert_eq!(shared.role, SemanticRole::Checkbox);
    assert_eq!(shared.state.checked, Some(false));
    assert_eq!(
        shared.actions,
        vec![
            CoreSemanticAction::Activate,
            CoreSemanticAction::SetValue,
            CoreSemanticAction::Focus
        ]
    );
}

#[test]
fn disabled_checkbox_releases_capture_and_blocks_changes() {
    let (mut tree, id, mut checkbox) = setup();
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: Point::new(20.0, 20.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    checkbox.set_enabled(&mut tree, id, false).unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(true))
            )
            .unwrap(),
        None
    );
    assert!(!checkbox.semantics(&tree, id).enabled);
}

#[test]
fn value_change_invalidates_paint_and_semantics_without_layout() {
    let (mut tree, id, mut checkbox) = setup();
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Semantic(SemanticAction::SetValue(true)),
        )
        .unwrap();
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
}

#[test]
fn style_swap_preserves_checked_value_and_keyboard_behavior() {
    let (mut tree, id, mut checkbox) = setup();
    tree.set_focus(Some(id)).unwrap();
    let bounds = tree.bounds(id).unwrap();
    let first = CheckboxStyle {
        box_color: Color::rgba8(10, 20, 30, 255),
        hover_color: Color::rgba8(15, 25, 35, 255),
        pressed_color: Color::rgba8(5, 15, 25, 255),
        check_color: Color::rgba8(255, 255, 255, 255),
        disabled_color: Color::rgba8(80, 80, 80, 255),
        focus_color: Color::rgba8(0, 200, 0, 255),
    };
    let second = CheckboxStyle {
        box_color: Color::rgba8(200, 30, 20, 255),
        hover_color: Color::rgba8(230, 60, 50, 255),
        ..first
    };
    let mut before = DisplayList::new();
    first.paint(&mut before, bounds, checkbox.visual_state(&tree, id));
    let mut after = DisplayList::new();
    second.paint(&mut after, bounds, checkbox.visual_state(&tree, id));
    assert_ne!(before.commands(), after.commands());
    assert_eq!(
        after.commands()[0],
        DisplayCommand::FillRect {
            rect: bounds,
            color: second.box_color
        }
    );
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Move {
                position: Point::new(20.0, 20.0),
            }),
        )
        .unwrap();
    let mut hovered = DisplayList::new();
    second.paint(&mut hovered, bounds, checkbox.visual_state(&tree, id));
    assert_eq!(
        hovered.commands()[0],
        DisplayCommand::FillRect {
            rect: bounds,
            color: second.hover_color
        }
    );
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Key(KeyEvent::Down {
                key: Key::Space,
                repeat: false,
            }),
        )
        .unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Up { key: Key::Space })
            )
            .unwrap(),
        Some(true)
    );
    assert!(checkbox.value());
}
