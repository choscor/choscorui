use choscorui_core::{Dirty, Point, Rect, Size, Tree, TreeError};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn hit_testing_prefers_last_child_and_respects_parent_clip() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let back = tree.attach(root, rect(10.0, 10.0, 80.0, 80.0)).unwrap();
    let front = tree.attach(root, rect(20.0, 20.0, 100.0, 100.0)).unwrap();
    assert_eq!(tree.hit_test(Point::new(30.0, 30.0)), Some(front));
    assert_eq!(tree.hit_test(Point::new(15.0, 15.0)), Some(back));
    assert_eq!(tree.hit_test(Point::new(110.0, 30.0)), None);
}

#[test]
fn detached_identity_is_never_reused_and_descendants_disappear() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let parent = tree.attach(root, rect(0.0, 0.0, 80.0, 80.0)).unwrap();
    let child = tree.attach(parent, rect(5.0, 5.0, 20.0, 20.0)).unwrap();
    tree.detach(parent).unwrap();
    assert!(!tree.contains(parent));
    assert!(!tree.contains(child));
    assert_eq!(tree.hit_test(Point::new(10.0, 10.0)), Some(root));
    let replacement = tree.attach(root, rect(0.0, 0.0, 10.0, 10.0)).unwrap();
    assert_ne!(replacement, parent);
    assert_ne!(replacement, child);
}

#[test]
fn translating_subtree_moves_descendants_and_preserves_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(10.0, 10.0, 50.0, 40.0)).unwrap();
    let content = tree
        .attach(viewport, rect(10.0, 10.0, 200.0, 100.0))
        .unwrap();
    let button = tree.attach(content, rect(30.0, 20.0, 20.0, 20.0)).unwrap();
    tree.set_focus(Some(button)).unwrap();
    tree.take_dirty(root).unwrap();
    tree.translate_subtree(content, Point::new(-20.0, 0.0))
        .unwrap();
    assert_eq!(tree.bounds(content), Some(rect(-10.0, 10.0, 200.0, 100.0)));
    assert_eq!(tree.bounds(button), Some(rect(10.0, 20.0, 20.0, 20.0)));
    assert_eq!(tree.hit_test(Point::new(15.0, 25.0)), Some(button));
    assert_eq!(tree.hit_test(Point::new(35.0, 25.0)), Some(content));
    assert_eq!(tree.focus(), Some(button));
    assert!(tree.take_dirty(root).unwrap().contains(Dirty::SEMANTICS));
}

#[test]
fn invalid_subtree_translation_is_atomic() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(10.0, 10.0, 20.0, 20.0)).unwrap();
    let descendant = tree.attach(child, rect(15.0, 15.0, 5.0, 5.0)).unwrap();
    tree.take_dirty(root).unwrap();
    assert_eq!(
        tree.translate_subtree(child, Point::new(f64::INFINITY, 0.0)),
        Err(TreeError::InvalidGeometry)
    );
    assert_eq!(tree.bounds(child), Some(rect(10.0, 10.0, 20.0, 20.0)));
    assert_eq!(tree.bounds(descendant), Some(rect(15.0, 15.0, 5.0, 5.0)));
    assert_eq!(tree.take_dirty(root).unwrap(), Dirty::NONE);
}
