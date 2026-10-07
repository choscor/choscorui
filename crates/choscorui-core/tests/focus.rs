use choscorui_core::{Point, Rect, Size, Tree};

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
