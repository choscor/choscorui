use choscorui_core::{
    Point, Rect, SemanticAction, SemanticChange, SemanticNode, SemanticParts, SemanticRole,
    SemanticState, SemanticTree, Size, Tree,
};
use choscorui_macos::{AccessibilityNotification, AccessibilityProjection};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

fn semantic(role: SemanticRole, name: &str, actions: Vec<SemanticAction>) -> SemanticNode {
    SemanticNode {
        role,
        name: name.into(),
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions,
        text: None,
    }
}

#[test]
fn reconciliation_preserves_native_identity_and_retires_removed_nodes() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let button = tree
        .attach(tree.root(), rect(10.0, 10.0, 80.0, 30.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(
            button,
            semantic(SemanticRole::Button, "Run", vec![SemanticAction::Activate]),
            &tree,
        )
        .unwrap();
    let mut projection = AccessibilityProjection::new();
    let first = projection.reconcile(&semantics.snapshot(&tree), &[SemanticChange::Tree]);
    let identity = first.elements[0].identity;
    assert_eq!(
        first.notifications,
        vec![AccessibilityNotification::LayoutChanged]
    );

    let mut updated = semantic(SemanticRole::Button, "Run", vec![SemanticAction::Activate]);
    updated.value = Some("Ready".into());
    semantics.set(button, updated, &tree).unwrap();
    let second = projection.reconcile(&semantics.snapshot(&tree), &[SemanticChange::Value(button)]);
    assert_eq!(second.elements[0].identity, identity);
    assert_eq!(
        second.notifications,
        vec![AccessibilityNotification::ValueChanged(identity)]
    );

    let mut localized = semantic(SemanticRole::Button, "Chạy", vec![SemanticAction::Activate]);
    localized.value = Some("Ready".into());
    semantics.set(button, localized, &tree).unwrap();
    let localized =
        projection.reconcile(&semantics.snapshot(&tree), &[SemanticChange::Label(button)]);
    assert_eq!(localized.elements[0].identity, identity);
    assert_eq!(
        localized.notifications,
        vec![AccessibilityNotification::LabelChanged(identity)]
    );

    tree.detach(button).unwrap();
    semantics.prune(&tree);
    let removed = projection.reconcile(&semantics.snapshot(&tree), &[SemanticChange::Tree]);
    assert_eq!(removed.retired, vec![identity]);
}

#[test]
fn focus_notifications_and_actions_use_declared_semantic_capabilities() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let button = tree
        .attach(tree.root(), rect(10.0, 10.0, 80.0, 30.0))
        .unwrap();
    tree.set_focusable(button, true).unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(
            button,
            semantic(SemanticRole::Button, "Run", vec![SemanticAction::Activate]),
            &tree,
        )
        .unwrap();
    let mut projection = AccessibilityProjection::new();
    projection.reconcile(&semantics.snapshot(&tree), &[SemanticChange::Tree]);
    tree.set_focus(Some(button)).unwrap();
    let change = semantics.sync_focus(&tree).unwrap();
    let result = projection.reconcile(&semantics.snapshot(&tree), &[change]);
    let identity = result.elements[0].identity;
    assert_eq!(
        result.notifications,
        vec![AccessibilityNotification::FocusChanged(identity)]
    );
    assert!(
        projection
            .request_action(identity, SemanticAction::Activate)
            .is_ok()
    );
    assert!(
        projection
            .request_action(identity, SemanticAction::Increment)
            .is_err()
    );
    assert_eq!(projection.take_actions()[0].node, button);
}

#[test]
fn parts_get_their_own_elements_focus_and_actions() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let tabs = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 30.0))
        .unwrap();
    tree.set_focusable(tabs, true).unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(
            tabs,
            semantic(SemanticRole::TabList, "Views", vec![]),
            &tree,
        )
        .unwrap();
    let mut parts = SemanticParts::new();
    for (index, name) in ["List", "Grid"].into_iter().enumerate() {
        let tab = parts.push(
            None,
            SemanticRole::Tab,
            name,
            rect(index as f64 * 100.0, 0.0, 100.0, 30.0),
        );
        tab.node.actions = vec![SemanticAction::Activate];
    }
    parts.set_focused(Some(1));
    semantics.set_parts(tabs, parts).unwrap();
    tree.set_focus(Some(tabs)).unwrap();
    let focus = semantics.sync_focus(&tree).unwrap();
    let mut projection = AccessibilityProjection::new();
    let update = projection.reconcile(&semantics.snapshot(&tree), &[focus]);
    assert_eq!(update.elements.len(), 3);
    let node = update.elements[0].identity;
    let grid = &update.elements[2];
    assert_eq!((grid.part, grid.parent), (Some(1), Some(node)));
    assert!(grid.focused && !update.elements[0].focused);
    assert_eq!(
        update.notifications,
        vec![AccessibilityNotification::FocusChanged(grid.identity)]
    );
    projection
        .request_action(grid.identity, SemanticAction::Activate)
        .unwrap();
    let request = &projection.take_actions()[0];
    assert_eq!((request.node, request.part), (tabs, Some(1)));
}
