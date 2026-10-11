use choscorui_core::{
    EventPhase, EventReply, InputModality, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
    Size, Tree,
};

fn tree() -> Tree {
    Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0)))
}

#[test]
fn tab_order_tracks_tree_order_and_wraps() {
    let mut tree = tree();
    let a = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    let container = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(30.0, 0.0), Size::new(50.0, 20.0)),
        )
        .unwrap();
    let b = tree
        .attach(
            container,
            Rect::new(Point::new(30.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    let c = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(90.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    tree.set_focusable(a, true).unwrap();
    tree.set_focusable(b, true).unwrap();
    tree.set_focusable(c, true).unwrap();
    assert_eq!(tree.focus_next(false), Some(a));
    assert_eq!(tree.focus_next(false), Some(b));
    assert_eq!(tree.focus_next(false), Some(c));
    assert_eq!(tree.focus_next(false), Some(a));
    assert_eq!(tree.focus_next(true), Some(c));
    tree.set_focusable(c, false).unwrap();
    assert_eq!(tree.focus_next(true), Some(b));
    tree.detach(b).unwrap();
    assert_eq!(tree.focus(), None);
    assert_eq!(tree.focus_next(false), Some(a));
}

#[test]
fn no_focusable_nodes_clear_focus() {
    let mut tree = tree();
    let a = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    tree.set_focus(Some(a)).unwrap();
    assert_eq!(tree.focus_next(false), None);
    assert_eq!(tree.focus(), None);
}

#[test]
fn keyboard_focus_shows_the_ring_and_a_pointer_press_does_not() {
    let mut tree = tree();
    let a = tree
        .attach(tree.root(), Rect::from_xywh(0.0, 0.0, 50.0, 20.0))
        .unwrap();
    let b = tree
        .attach(tree.root(), Rect::from_xywh(60.0, 0.0, 50.0, 20.0))
        .unwrap();
    tree.set_focusable(a, true).unwrap();
    tree.set_focusable(b, true).unwrap();

    assert_eq!(tree.focus_next(false), Some(a));
    assert!(tree.focus_visible(a));
    assert_eq!(tree.input_modality(), InputModality::Keyboard);

    tree.take_dirty(tree.root()).unwrap();
    let down = PointerEvent::Down {
        position: Point::new(70.0, 10.0),
        button: PointerButton::Primary,
    };
    tree.dispatch_pointer(down, |tree, node, phase, _| {
        if phase == EventPhase::Target {
            tree.set_focus(Some(node)).unwrap();
        }
        EventReply::Continue
    });
    assert_eq!(tree.focus(), Some(b));
    assert!(!tree.focus_visible(b));

    // A key press brings the ring back on the focused node.
    tree.dispatch_key(
        KeyEvent::Down {
            key: Key::ArrowRight,
            repeat: false,
        },
        |_, _, _, _| EventReply::Continue,
    );
    assert!(tree.focus_visible(b));

    tree.focus_by_pointer(a).unwrap();
    assert!(!tree.focus_visible(a));
    assert!(!tree.focus_visible(b));
}
