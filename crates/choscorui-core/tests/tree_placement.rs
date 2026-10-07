use choscorui_core::{Dirty, Point, Rect, Size, Tree, TreeError};

fn rect() -> Rect {
    Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0))
}

#[test]
fn placement_preserves_identity_and_reorders_or_reparents() {
    let mut tree = Tree::new(rect());
    let root = tree.root();
    let first = tree.attach(root, rect()).unwrap();
    let second = tree.attach(root, rect()).unwrap();
    let leaf = tree.attach(first, rect()).unwrap();
    for id in [root, first, second, leaf] {
        tree.take_dirty(id).unwrap();
    }

    tree.place_child(root, second, 0).unwrap();
    tree.place_child(second, leaf, 0).unwrap();

    assert_eq!(tree.children(root), Some([second, first].as_slice()));
    assert_eq!(tree.children(second), Some([leaf].as_slice()));
    assert_eq!(tree.parent(leaf), Some(second));
    let dirty = tree.take_dirty(root).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
}

#[test]
fn invalid_placement_is_atomic() {
    let mut tree = Tree::new(rect());
    let root = tree.root();
    let parent = tree.attach(root, rect()).unwrap();
    let child = tree.attach(parent, rect()).unwrap();
    let original_root = tree.children(root).unwrap().to_vec();
    let original_parent = tree.children(parent).unwrap().to_vec();

    assert_eq!(tree.place_child(child, parent, 0), Err(TreeError::Cycle));
    assert_eq!(
        tree.place_child(root, child, 2),
        Err(TreeError::InvalidChildIndex)
    );
    assert_eq!(tree.children(root).unwrap(), original_root);
    assert_eq!(tree.children(parent).unwrap(), original_parent);
    assert_eq!(tree.parent(child), Some(parent));
}
