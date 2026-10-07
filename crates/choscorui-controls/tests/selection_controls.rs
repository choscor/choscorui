use choscorui_controls::{
    RadioGroup, RadioInput, RadioOption, SelectionError, Tab, TabInput, TabSet,
};
use choscorui_core::{
    Dirty, Key, KeyEvent, Point, Rect, SemanticAction, SemanticRelation, SemanticRole, Size, Tree,
};

fn node(tree: &mut Tree, y: f64) -> choscorui_core::NodeId {
    tree.attach(
        tree.root(),
        Rect::new(Point::new(10.0, y), Size::new(180.0, 30.0)),
    )
    .unwrap()
}

#[test]
fn radio_group_is_exclusive_and_navigates_enabled_members() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 300.0)));
    let first = node(&mut tree, 10.0);
    let disabled = node(&mut tree, 50.0);
    let third = node(&mut tree, 90.0);
    let mut group = RadioGroup::new(
        "Delivery",
        vec![
            RadioOption::new(first, "Standard"),
            RadioOption::new(disabled, "Express").disabled(),
            RadioOption::new(third, "Pickup"),
        ],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    tree.set_focus(Some(first)).unwrap();

    assert_eq!(
        group
            .handle(
                &mut tree,
                first,
                RadioInput::Key(KeyEvent::Down {
                    key: Key::ArrowDown,
                    repeat: false,
                }),
            )
            .unwrap(),
        Some(third)
    );
    assert_eq!(group.selected(), third);
    assert_eq!(tree.focus(), Some(third));
    assert_eq!(
        group
            .handle(&mut tree, first, RadioInput::Activate)
            .unwrap(),
        Some(first)
    );
    assert_eq!(group.selected(), first);

    let first_semantics = group.semantics(first).unwrap();
    let third_semantics = group.semantics(third).unwrap();
    assert_eq!(first_semantics.role, SemanticRole::RadioButton);
    assert_eq!(first_semantics.state.checked, Some(true));
    assert_eq!(third_semantics.state.checked, Some(false));
    assert_eq!(
        first_semantics.actions,
        vec![SemanticAction::Activate, SemanticAction::Focus]
    );
    let group_semantics = group.group_semantics();
    assert_eq!(group_semantics.role, SemanticRole::RadioGroup);
    assert_eq!(group_semantics.value.as_deref(), Some("Standard"));
}

#[test]
fn radio_selection_only_invalidates_paint_and_semantics() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 200.0)));
    let first = node(&mut tree, 10.0);
    let second = node(&mut tree, 50.0);
    let mut group = RadioGroup::new(
        "Mode",
        vec![RadioOption::new(first, "A"), RadioOption::new(second, "B")],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    for id in [tree.root(), first, second] {
        tree.take_dirty(id).unwrap();
    }
    group
        .handle(&mut tree, second, RadioInput::Activate)
        .unwrap();
    for id in [first, second] {
        let dirty = tree.take_dirty(id).unwrap();
        assert!(dirty.contains(Dirty::PAINT));
        assert!(dirty.contains(Dirty::SEMANTICS));
        assert!(!dirty.contains(Dirty::LAYOUT));
    }
}

#[test]
fn tabs_select_panels_and_expose_bidirectional_relationships() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0)));
    let first_tab = node(&mut tree, 10.0);
    let second_tab = node(&mut tree, 10.0);
    let third_tab = node(&mut tree, 10.0);
    let first_panel = node(&mut tree, 60.0);
    let second_panel = node(&mut tree, 60.0);
    let third_panel = node(&mut tree, 60.0);
    let mut tabs = TabSet::new(
        "Settings",
        vec![
            Tab::new(first_tab, first_panel, "General"),
            Tab::new(second_tab, second_panel, "Advanced").disabled(),
            Tab::new(third_tab, third_panel, "About"),
        ],
        first_tab,
    )
    .unwrap();
    tabs.install(&mut tree).unwrap();
    tree.set_focus(Some(first_tab)).unwrap();

    assert_eq!(
        tabs.handle(
            &mut tree,
            first_tab,
            TabInput::Key(KeyEvent::Down {
                key: Key::ArrowRight,
                repeat: false,
            }),
        )
        .unwrap(),
        Some(third_tab)
    );
    assert_eq!(tabs.selected_tab(), third_tab);
    assert_eq!(tabs.selected_panel(), third_panel);
    assert_eq!(tree.focus(), Some(third_tab));

    let tab = tabs.tab_semantics(third_tab).unwrap();
    assert_eq!(tab.role, SemanticRole::Tab);
    assert_eq!(tab.state.selected, Some(true));
    assert!(
        tab.relations
            .contains(&SemanticRelation::Controls(third_panel))
    );
    let panel = tabs.panel_semantics(third_panel).unwrap();
    assert_eq!(panel.role, SemanticRole::TabPanel);
    assert!(!panel.state.hidden);
    assert!(
        panel
            .relations
            .contains(&SemanticRelation::LabelledBy(third_tab))
    );
    assert!(tabs.panel_semantics(first_panel).unwrap().state.hidden);
    let list = tabs.group_semantics();
    assert_eq!(list.role, SemanticRole::TabList);
    assert_eq!(list.value.as_deref(), Some("About"));
}

#[test]
fn selection_models_reject_bad_identity_without_partial_installation() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0)));
    let id = node(&mut tree, 10.0);
    assert!(matches!(
        RadioGroup::new("Bad", vec![RadioOption::new(id, "A")], tree.root()),
        Err(SelectionError::UnknownSelection)
    ));
    let missing_panel = {
        let value = node(&mut tree, 50.0);
        tree.detach(value).unwrap();
        value
    };
    let tabs = TabSet::new("Bad", vec![Tab::new(id, missing_panel, "A")], id).unwrap();
    assert_eq!(
        tabs.install(&mut tree),
        Err(SelectionError::MissingNode(missing_panel))
    );
}

#[test]
fn detachment_cancels_a_selection_transaction() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0)));
    let first = node(&mut tree, 10.0);
    let second = node(&mut tree, 50.0);
    let mut group = RadioGroup::new(
        "Mode",
        vec![RadioOption::new(first, "A"), RadioOption::new(second, "B")],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    tree.detach(first).unwrap();

    assert_eq!(
        group.handle(&mut tree, second, RadioInput::Activate),
        Err(SelectionError::MissingNode(first))
    );
    assert_eq!(group.selected(), first);
}
