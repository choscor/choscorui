use choscorui_compose::{ComposeError, Element, Key, Reconciler};
use choscorui_core::{Point, Rect, Size, Tree};

fn bounds(x: f64) -> Rect {
    Rect::new(Point::new(x, 0.0), Size::new(10.0, 10.0))
}

fn element(key: &str, x: f64) -> Element {
    Element::new(Key::new(key), bounds(x))
}

#[test]
fn keyed_reconciliation_preserves_identity_across_reorder_and_reparent() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(
            &mut tree,
            &[
                element("left", 1.0).with_child(element("field", 2.0).focusable(true)),
                element("right", 3.0),
            ],
        )
        .unwrap();
    let field = composition.node_id(&Key::new("field")).unwrap();
    let right = composition.node_id(&Key::new("right")).unwrap();
    tree.set_focusable(field, true).unwrap();
    tree.set_focus(Some(field)).unwrap();

    let report = composition
        .reconcile(
            &mut tree,
            &[
                element("right", 30.0).with_child(element("field", 20.0).focusable(true)),
                element("left", 10.0),
            ],
        )
        .unwrap();

    assert_eq!(composition.node_id(&Key::new("field")), Some(field));
    assert_eq!(tree.parent(field), Some(right));
    assert_eq!(tree.focus(), Some(field));
    assert_eq!(tree.bounds(field), Some(bounds(20.0)));
    assert_eq!(report.created, 0);
    assert_eq!(report.reused, 3);
    assert!(report.moved >= 2);
}

#[test]
fn removal_prunes_subtrees_and_new_keys_never_reuse_ids() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(
            &mut tree,
            &[element("old", 1.0).with_child(element("leaf", 2.0))],
        )
        .unwrap();
    let old = composition.node_id(&Key::new("old")).unwrap();
    let report = composition
        .reconcile(&mut tree, &[element("new", 3.0)])
        .unwrap();

    let new = composition.node_id(&Key::new("new")).unwrap();
    assert_ne!(old, new);
    assert!(!tree.contains(old));
    assert_eq!(report.removed, 2);
}

#[test]
fn invalid_descriptions_leave_tree_and_key_map_unchanged() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(&mut tree, &[element("kept", 1.0)])
        .unwrap();
    let kept = composition.node_id(&Key::new("kept")).unwrap();
    let original_children = tree.children(tree.root()).unwrap().to_vec();

    let error = composition
        .reconcile(
            &mut tree,
            &[element("duplicate", 2.0), element("duplicate", 3.0)],
        )
        .unwrap_err();

    assert_eq!(error, ComposeError::DuplicateKey(Key::new("duplicate")));
    assert_eq!(composition.node_id(&Key::new("kept")), Some(kept));
    assert_eq!(tree.children(tree.root()).unwrap(), original_children);
}

#[test]
fn invalid_geometry_is_rejected_before_existing_nodes_change() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(&mut tree, &[element("kept", 1.0)])
        .unwrap();
    let kept = composition.node_id(&Key::new("kept")).unwrap();

    let error = composition
        .reconcile(
            &mut tree,
            &[Element::new(
                Key::new("bad"),
                Rect::new(Point::new(f64::NAN, 0.0), Size::new(1.0, 1.0)),
            )],
        )
        .unwrap_err();

    assert_eq!(error, ComposeError::InvalidBounds(Key::new("bad")));
    assert_eq!(tree.bounds(kept), Some(bounds(1.0)));
}
