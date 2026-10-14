use choscorui_core::{
    Axis, CrossAlign, Insets, LayoutDirection, LinearLayout, Point, Rect, SemanticAction,
    SemanticChange, SemanticNode, SemanticParts, SemanticRelation, SemanticRole, SemanticState,
    SemanticText, SemanticTree, Size, Tree,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn rtl_snapshot_keeps_logical_reading_order_after_mirrored_layout() {
    let mut tree = Tree::new(rect(0.0, 0.0, 240.0, 50.0));
    let first = tree
        .attach(tree.root(), rect(0.0, 0.0, 60.0, 30.0))
        .unwrap();
    let second = tree
        .attach(tree.root(), rect(0.0, 0.0, 60.0, 30.0))
        .unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Rtl,
        Insets::all(0.0),
        8.0,
        CrossAlign::Start,
    )
    .unwrap();
    row.add_fixed(first, 60.0, 30.0).unwrap();
    row.add_fixed(second, 60.0, 30.0).unwrap();
    row.apply(&mut tree, rect(0.0, 0.0, 240.0, 50.0)).unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(first, node(SemanticRole::Button, "الأول"), &tree)
        .unwrap();
    semantics
        .set(second, node(SemanticRole::Button, "الثاني"), &tree)
        .unwrap();

    let snapshot = semantics.snapshot(&tree);
    assert_eq!(
        snapshot.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![first, second]
    );
    assert!(snapshot[0].bounds.origin.x > snapshot[1].bounds.origin.x);
}

#[test]
fn value_selection_state_and_bounds_are_live_in_snapshots() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 100.0));
    let field = tree
        .attach(tree.root(), rect(10.0, 10.0, 100.0, 30.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    let mut field_node = node(SemanticRole::TextField, "Tên");
    field_node.value = Some("Lan".into());
    field_node.text = Some(SemanticText {
        character_count_utf16: 3,
        selection_utf16: 0..0,
        multiline: false,
    });
    semantics.set(field, field_node.clone(), &tree).unwrap();

    field_node.value = Some("Linh".into());
    field_node.state.invalid = true;
    field_node.text = Some(SemanticText {
        character_count_utf16: 4,
        selection_utf16: 1..3,
        multiline: false,
    });
    assert_eq!(
        semantics.set(field, field_node, &tree).unwrap(),
        SemanticChange::ValueAndState(field)
    );
    tree.set_bounds(field, rect(20.0, 15.0, 160.0, 30.0))
        .unwrap();

    let current = semantics.snapshot(&tree).remove(0);
    assert_eq!(current.node.value.as_deref(), Some("Linh"));
    assert!(current.node.state.invalid);
    assert_eq!(current.node.text.unwrap().selection_utf16, 1..3);
    assert_eq!(current.bounds, rect(20.0, 15.0, 160.0, 30.0));
}

fn node(role: SemanticRole, name: &str) -> SemanticNode {
    SemanticNode {
        role,
        name: name.into(),
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions: Vec::new(),
        text: None,
    }
}

#[test]
fn snapshot_preserves_retained_order_identity_and_layout_bounds() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
    let group = tree
        .attach(tree.root(), rect(10.0, 20.0, 200.0, 100.0))
        .unwrap();
    let first = tree.attach(group, rect(12.0, 24.0, 80.0, 30.0)).unwrap();
    let second = tree.attach(group, rect(12.0, 60.0, 80.0, 30.0)).unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(group, node(SemanticRole::Group, "Options"), &tree)
        .unwrap();
    semantics
        .set(first, node(SemanticRole::Button, "Save"), &tree)
        .unwrap();
    semantics
        .set(second, node(SemanticRole::Checkbox, "Remember"), &tree)
        .unwrap();

    let snapshot = semantics.snapshot(&tree);
    assert_eq!(
        snapshot.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![group, first, second]
    );
    assert_eq!(snapshot[1].bounds, rect(12.0, 24.0, 80.0, 30.0));
    assert_eq!(snapshot[1].parent, Some(group));
}

#[test]
fn updates_report_precise_changes_and_reject_dangling_relationships() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
    let label = tree
        .attach(tree.root(), rect(0.0, 0.0, 80.0, 20.0))
        .unwrap();
    let field = tree
        .attach(tree.root(), rect(90.0, 0.0, 150.0, 30.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(label, node(SemanticRole::StaticText, "Name"), &tree)
        .unwrap();
    let mut field_node = node(SemanticRole::TextField, "Name");
    field_node.value = Some("Lan".into());
    field_node.actions.push(SemanticAction::SetValue);
    field_node
        .relations
        .push(SemanticRelation::LabelledBy(label));
    assert_eq!(
        semantics.set(field, field_node.clone(), &tree).unwrap(),
        SemanticChange::Tree
    );
    assert_eq!(
        semantics.set(field, field_node.clone(), &tree).unwrap(),
        SemanticChange::None
    );
    field_node.value = Some("Linh".into());
    assert_eq!(
        semantics.set(field, field_node, &tree).unwrap(),
        SemanticChange::Value(field)
    );

    tree.detach(label).unwrap();
    assert!(semantics.validate(&tree).is_err());
    let removed = semantics.prune(&tree);
    assert_eq!(removed, vec![label]);
}

#[test]
fn focus_and_action_dispatch_require_declared_capabilities() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let button = tree
        .attach(tree.root(), rect(0.0, 0.0, 80.0, 30.0))
        .unwrap();
    tree.set_focusable(button, true).unwrap();
    let mut semantic_node = node(SemanticRole::Button, "Run");
    semantic_node.actions.push(SemanticAction::Activate);
    let mut semantics = SemanticTree::new();
    semantics.set(button, semantic_node, &tree).unwrap();

    assert!(semantics.supports(button, SemanticAction::Activate));
    assert!(!semantics.supports(button, SemanticAction::Increment));
    tree.set_focus(Some(button)).unwrap();
    assert_eq!(
        semantics.sync_focus(&tree),
        Some(SemanticChange::Focus {
            old: None,
            new: Some(button),
            part: None,
        })
    );
    assert_eq!(semantics.sync_focus(&tree), None);
}

#[test]
fn simultaneous_value_and_state_update_is_not_a_tree_replacement() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let checkbox = tree
        .attach(tree.root(), rect(0.0, 0.0, 80.0, 30.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    let mut checkbox_node = node(SemanticRole::Checkbox, "Remember");
    checkbox_node.value = Some("false".into());
    checkbox_node.state.checked = Some(false);
    semantics
        .set(checkbox, checkbox_node.clone(), &tree)
        .unwrap();
    checkbox_node.value = Some("true".into());
    checkbox_node.state.checked = Some(true);
    assert_eq!(
        semantics.set(checkbox, checkbox_node, &tree).unwrap(),
        SemanticChange::ValueAndState(checkbox)
    );
}

#[test]
fn localized_name_update_preserves_identity_and_reports_label_change() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let button = tree
        .attach(tree.root(), rect(0.0, 0.0, 80.0, 30.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    let button_node = node(SemanticRole::Button, "Run");
    semantics.set(button, button_node.clone(), &tree).unwrap();
    let mut localized = button_node;
    localized.name = "Chạy".into();
    assert_eq!(
        semantics.set(button, localized, &tree).unwrap(),
        SemanticChange::Label(button)
    );
}

#[test]
fn virtualized_collections_report_total_size_and_absolute_item_index() {
    use choscorui_core::{SemanticCollection, SemanticError};
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 200.0));
    let list = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    let row = tree.attach(list, rect(0.0, 0.0, 200.0, 20.0)).unwrap();
    let mut semantics = SemanticTree::new();
    let collection = Some(SemanticCollection {
        item_count: 10_000,
        column_count: Some(3),
    });
    assert_eq!(
        semantics.set_collection(list, collection),
        Err(SemanticError::MissingNode)
    );
    semantics
        .set(list, node(SemanticRole::List, "Messages"), &tree)
        .unwrap();
    semantics
        .set(row, node(SemanticRole::ListItem, "Row 4,001"), &tree)
        .unwrap();
    assert_eq!(
        semantics.set_collection(list, collection).unwrap(),
        SemanticChange::Tree
    );
    assert_eq!(
        semantics.set_collection(list, collection).unwrap(),
        SemanticChange::None
    );
    assert_eq!(
        semantics.set_item_index(row, Some(4_000)).unwrap(),
        SemanticChange::Tree
    );
    let snapshot = semantics.snapshot(&tree);
    assert_eq!(snapshot[0].collection, collection);
    assert_eq!(snapshot[0].item_index, None);
    assert_eq!(snapshot[1].item_index, Some(4_000));
    assert_eq!(snapshot[1].parent, Some(list));

    // Scrolling recycles the realized row for another absolute index.
    semantics.set_item_index(row, Some(4_020)).unwrap();
    assert_eq!(semantics.snapshot(&tree)[1].item_index, Some(4_020));

    tree.detach(row).unwrap();
    assert_eq!(semantics.prune(&tree), vec![row]);
    let row = tree.attach(list, rect(0.0, 0.0, 200.0, 20.0)).unwrap();
    semantics
        .set(row, node(SemanticRole::ListItem, "Row 1"), &tree)
        .unwrap();
    assert_eq!(
        semantics.snapshot(&tree)[1].item_index,
        None,
        "pruned index"
    );
}

#[test]
fn parts_publish_child_entries_and_the_focused_part() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let menu = tree
        .attach(tree.root(), rect(0.0, 0.0, 120.0, 60.0))
        .unwrap();
    tree.set_focusable(menu, true).unwrap();
    let mut semantics = SemanticTree::new();
    semantics
        .set(menu, node(SemanticRole::Menu, "File"), &tree)
        .unwrap();
    let build = |checked: bool, focused: usize| {
        let mut parts = SemanticParts::new();
        parts.push(
            None,
            SemanticRole::MenuItem,
            "Open",
            rect(0.0, 0.0, 120.0, 20.0),
        );
        let save = parts.push(
            None,
            SemanticRole::MenuItem,
            "Save",
            rect(0.0, 20.0, 120.0, 20.0),
        );
        save.node.state.checked = Some(checked);
        parts.push(
            Some(1),
            SemanticRole::StaticText,
            "⌘S",
            rect(90.0, 20.0, 30.0, 20.0),
        );
        parts.set_focused(Some(focused));
        parts
    };
    assert_eq!(
        semantics.set_parts(menu, build(false, 0)).unwrap(),
        vec![SemanticChange::Tree]
    );
    tree.set_focus(Some(menu)).unwrap();
    assert_eq!(
        semantics.sync_focus(&tree),
        Some(SemanticChange::Focus {
            old: None,
            new: Some(menu),
            part: Some(0),
        })
    );
    let snapshot = semantics.snapshot(&tree);
    let entries: Vec<_> = snapshot
        .iter()
        .map(|entry| (entry.part, entry.parent_part, entry.focused))
        .collect();
    assert_eq!(
        entries,
        vec![
            (None, None, false),
            (Some(0), None, true),
            (Some(1), None, false),
            (Some(2), Some(1), false),
        ],
        "the focused part is focused, not the node"
    );
    assert_eq!(
        semantics.set_parts(menu, build(true, 1)).unwrap(),
        vec![SemanticChange::Part {
            node: menu,
            part: 1
        }]
    );
    assert_eq!(
        semantics.sync_focus(&tree),
        Some(SemanticChange::Focus {
            old: Some(menu),
            new: Some(menu),
            part: Some(1),
        })
    );
    let mut fewer = SemanticParts::new();
    fewer.push(
        None,
        SemanticRole::MenuItem,
        "Open",
        rect(0.0, 0.0, 120.0, 20.0),
    );
    assert_eq!(
        semantics.set_parts(menu, fewer).unwrap(),
        vec![SemanticChange::Tree]
    );
    assert_eq!(semantics.parts(menu).unwrap().len(), 1);
}

#[test]
fn collection_parts_keep_their_key_when_the_visible_range_moves() {
    let mut parts = SemanticParts::new();
    parts
        .push(
            None,
            SemanticRole::Row,
            "Row 40",
            rect(0.0, 0.0, 10.0, 10.0),
        )
        .item(40);
    let mut moved = SemanticParts::new();
    moved
        .push(
            None,
            SemanticRole::Row,
            "Row 39",
            rect(0.0, 0.0, 10.0, 10.0),
        )
        .item(39);
    moved
        .push(
            None,
            SemanticRole::Row,
            "Row 40",
            rect(0.0, 10.0, 10.0, 10.0),
        )
        .item(40);
    assert_eq!(parts.parts()[0].key, moved.parts()[1].key);
    assert_eq!(moved.parts()[1].item_index, Some(40));
}
