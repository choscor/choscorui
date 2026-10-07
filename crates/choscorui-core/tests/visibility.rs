use choscorui_core::{Point, Rect, Size, Tree, TreeError};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn hidden_subtrees_do_not_receive_input_or_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let hidden = tree
        .attach(tree.root(), rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let child = tree.attach(hidden, rect(10.0, 10.0, 20.0, 20.0)).unwrap();
    let visible = tree
        .attach(tree.root(), rect(120.0, 10.0, 20.0, 20.0))
        .unwrap();
    tree.set_focusable(child, true).unwrap();
    tree.set_focusable(visible, true).unwrap();
    tree.set_focus(Some(child)).unwrap();
    tree.set_pointer_capture(Some(child)).unwrap();

    tree.set_visible(hidden, false).unwrap();

    assert_eq!(tree.visible(child), Some(false));
    assert_eq!(tree.focus(), None);
    assert_eq!(tree.pointer_capture(), None);
    assert_ne!(tree.hit_test(Point::new(15.0, 15.0)), Some(child));
    assert_eq!(tree.focus_next(false), Some(visible));
    assert_eq!(tree.set_focus(Some(child)), Err(TreeError::HiddenNode));
    assert_eq!(
        tree.set_pointer_capture(Some(child)),
        Err(TreeError::HiddenNode)
    );

    tree.set_visible(hidden, true).unwrap();
    assert_eq!(tree.visible(child), Some(true));
    tree.set_focus(Some(child)).unwrap();
    tree.set_pointer_capture(Some(child)).unwrap();
    assert_eq!(tree.hit_test(Point::new(15.0, 15.0)), Some(child));
}
