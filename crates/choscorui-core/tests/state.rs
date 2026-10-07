use choscorui_core::{Dirty, Point, Rect, Size, Tree};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn geometry_change_invalidates_layout_and_paint_up_to_root() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let parent = tree.attach(root, rect(0.0, 0.0, 50.0, 50.0)).unwrap();
    let child = tree.attach(parent, rect(0.0, 0.0, 10.0, 10.0)).unwrap();
    tree.take_dirty(root).unwrap();
    tree.take_dirty(parent).unwrap();
    tree.take_dirty(child).unwrap();

    tree.set_bounds(child, rect(20.0, 20.0, 10.0, 10.0))
        .unwrap();
    assert_eq!(tree.bounds(child), Some(rect(20.0, 20.0, 10.0, 10.0)));
    assert_eq!(tree.hit_test(Point::new(25.0, 25.0)), Some(child));
    assert!(tree.take_dirty(child).unwrap().contains(Dirty::LAYOUT));
    assert!(tree.take_dirty(parent).unwrap().contains(Dirty::LAYOUT));
    assert!(tree.take_dirty(root).unwrap().contains(Dirty::LAYOUT));
    assert_eq!(tree.take_dirty(root).unwrap(), Dirty::NONE);
}

#[test]
fn no_op_geometry_change_does_not_request_frame() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    tree.take_dirty(root).unwrap();
    tree.set_bounds(root, rect(0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(tree.take_dirty(root).unwrap(), Dirty::NONE);
}

#[test]
fn removal_clears_focus_and_capture_for_descendants() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let parent = tree.attach(root, rect(0.0, 0.0, 50.0, 50.0)).unwrap();
    let child = tree.attach(parent, rect(0.0, 0.0, 10.0, 10.0)).unwrap();
    tree.set_focus(Some(child)).unwrap();
    tree.set_pointer_capture(Some(child)).unwrap();
    tree.take_dirty(root).unwrap();
    tree.detach(parent).unwrap();
    assert_eq!(tree.focus(), None);
    assert_eq!(tree.pointer_capture(), None);
    assert!(tree.take_dirty(root).unwrap().contains(Dirty::SEMANTICS));
}

#[test]
fn attaching_and_detaching_request_paint_and_semantics_on_ancestors() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    tree.take_dirty(root).unwrap();
    let child = tree.attach(root, rect(0.0, 0.0, 10.0, 10.0)).unwrap();
    let dirty = tree.take_dirty(root).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));

    tree.detach(child).unwrap();
    let dirty = tree.take_dirty(root).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
}
