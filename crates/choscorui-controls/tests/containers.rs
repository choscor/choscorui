use choscorui_controls::{
    ContainerError, GroupBox, SplitOrientation, Splitter, SplitterInput, StackPage,
    StackedContainer,
};
use choscorui_core::{Dirty, Key, KeyEvent, Point, Rect, SemanticAction, SemanticRole, Size, Tree};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn splitter_resizes_by_keyboard_and_reports_an_accessible_range() {
    let mut tree = Tree::new(rect(0.0, 0.0, 500.0, 200.0));
    let first = tree
        .attach(tree.root(), rect(0.0, 0.0, 240.0, 200.0))
        .unwrap();
    let handle = tree
        .attach(tree.root(), rect(240.0, 0.0, 8.0, 200.0))
        .unwrap();
    let second = tree
        .attach(tree.root(), rect(248.0, 0.0, 252.0, 200.0))
        .unwrap();
    let mut splitter = Splitter::new(
        "Sidebar width",
        SplitOrientation::Horizontal,
        first,
        handle,
        second,
        rect(0.0, 0.0, 500.0, 200.0),
        100.0,
        120.0,
        8.0,
        240.0,
    )
    .unwrap();
    splitter.install(&mut tree).unwrap();
    tree.set_focus(Some(handle)).unwrap();
    for id in [tree.root(), first, handle, second] {
        tree.take_dirty(id).unwrap();
    }

    assert_eq!(
        splitter
            .handle(
                &mut tree,
                SplitterInput::Key(KeyEvent::Down {
                    key: Key::ArrowRight,
                    repeat: false,
                }),
            )
            .unwrap(),
        Some(250.0)
    );
    assert_eq!(tree.bounds(first), Some(rect(0.0, 0.0, 250.0, 200.0)));
    assert_eq!(tree.bounds(handle), Some(rect(250.0, 0.0, 8.0, 200.0)));
    assert_eq!(tree.bounds(second), Some(rect(258.0, 0.0, 242.0, 200.0)));
    assert_eq!(
        splitter
            .handle(
                &mut tree,
                SplitterInput::Semantic(SemanticAction::Decrement)
            )
            .unwrap(),
        Some(240.0)
    );

    let semantics = splitter.semantics(&tree);
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.value.as_deref(), Some("240"));
    assert_eq!(
        semantics.actions,
        vec![
            SemanticAction::Increment,
            SemanticAction::Decrement,
            SemanticAction::Focus
        ]
    );
    let dirty = tree.take_dirty(first).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
}

#[test]
fn splitter_rejects_invalid_changes_atomically_and_clamps_actions() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 400.0));
    let first = tree
        .attach(tree.root(), rect(0.0, 0.0, 300.0, 180.0))
        .unwrap();
    let handle = tree
        .attach(tree.root(), rect(0.0, 180.0, 300.0, 10.0))
        .unwrap();
    let second = tree
        .attach(tree.root(), rect(0.0, 190.0, 300.0, 210.0))
        .unwrap();
    let mut splitter = Splitter::new(
        "Rows",
        SplitOrientation::Vertical,
        first,
        handle,
        second,
        rect(0.0, 0.0, 300.0, 400.0),
        80.0,
        100.0,
        10.0,
        180.0,
    )
    .unwrap();
    splitter.install(&mut tree).unwrap();
    let before = [tree.bounds(first), tree.bounds(handle), tree.bounds(second)];
    assert_eq!(
        splitter.set_position(&mut tree, 500.0),
        Err(ContainerError::InvalidPosition)
    );
    assert_eq!(splitter.position(), 180.0);
    assert_eq!(
        [tree.bounds(first), tree.bounds(handle), tree.bounds(second)],
        before
    );

    tree.set_focus(Some(handle)).unwrap();
    assert_eq!(
        splitter
            .handle(
                &mut tree,
                SplitterInput::Key(KeyEvent::Down {
                    key: Key::End,
                    repeat: false,
                }),
            )
            .unwrap(),
        Some(290.0)
    );
    assert_eq!(tree.bounds(second), Some(rect(0.0, 300.0, 300.0, 100.0)));
}

#[test]
fn group_box_validates_members_and_exposes_title_relationships() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let group = tree
        .attach(tree.root(), rect(10.0, 10.0, 300.0, 200.0))
        .unwrap();
    let title = tree.attach(group, rect(20.0, 10.0, 80.0, 20.0)).unwrap();
    let member = tree.attach(group, rect(20.0, 50.0, 100.0, 30.0)).unwrap();
    let box_control = GroupBox::new("Account", group, Some(title), vec![member]).unwrap();
    box_control.install(&tree).unwrap();
    let semantics = box_control.semantics();
    assert_eq!(semantics.role, SemanticRole::Group);
    assert_eq!(semantics.name, "Account");
    assert_eq!(
        semantics.relations,
        vec![
            choscorui_core::SemanticRelation::LabelledBy(title),
            choscorui_core::SemanticRelation::Controls(member),
        ]
    );

    let outsider = tree
        .attach(tree.root(), rect(320.0, 10.0, 30.0, 30.0))
        .unwrap();
    let invalid = GroupBox::new("Bad", group, None, vec![outsider]).unwrap();
    assert_eq!(
        invalid.install(&tree),
        Err(ContainerError::InvalidDescendant {
            node: outsider,
            container: group,
        })
    );
}

#[test]
fn stack_switches_visibility_and_restores_each_pages_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let stack = tree
        .attach(tree.root(), rect(0.0, 0.0, 400.0, 300.0))
        .unwrap();
    let first = tree.attach(stack, rect(0.0, 0.0, 400.0, 300.0)).unwrap();
    let first_a = tree.attach(first, rect(10.0, 10.0, 100.0, 30.0)).unwrap();
    let first_b = tree.attach(first, rect(10.0, 50.0, 100.0, 30.0)).unwrap();
    let second = tree.attach(stack, rect(0.0, 0.0, 400.0, 300.0)).unwrap();
    let second_focus = tree.attach(second, rect(10.0, 10.0, 100.0, 30.0)).unwrap();
    for id in [first_a, first_b, second_focus] {
        tree.set_focusable(id, true).unwrap();
    }
    let mut pages = StackedContainer::new(
        "Wizard pages",
        stack,
        vec![
            StackPage::new(first, Some(first_a)),
            StackPage::new(second, Some(second_focus)),
        ],
        first,
    )
    .unwrap();
    pages.install(&mut tree).unwrap();
    assert_eq!(tree.visible(first), Some(true));
    assert_eq!(tree.visible(second), Some(false));
    assert_eq!(tree.hit_test(Point::new(15.0, 15.0)), Some(first_a));
    tree.set_focus(Some(first_b)).unwrap();

    assert!(pages.select(&mut tree, second).unwrap());
    assert_eq!(tree.focus(), Some(second_focus));
    assert_eq!(tree.visible(first), Some(false));
    assert_eq!(tree.visible(second), Some(true));
    assert_eq!(tree.hit_test(Point::new(15.0, 15.0)), Some(second_focus));
    assert!(pages.page_semantics(first).unwrap().state.hidden);
    assert!(!pages.page_semantics(second).unwrap().state.hidden);
    assert!(pages.select(&mut tree, first).unwrap());
    assert_eq!(tree.focus(), Some(first_b));
    assert_eq!(pages.current(), first);
    assert_eq!(pages.semantics().role, SemanticRole::Group);
}

#[test]
fn stack_rejects_detached_focus_target_without_changing_page_or_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
    let stack = tree
        .attach(tree.root(), rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    let first = tree.attach(stack, rect(0.0, 0.0, 300.0, 200.0)).unwrap();
    let first_focus = tree.attach(first, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let second = tree.attach(stack, rect(0.0, 0.0, 300.0, 200.0)).unwrap();
    let missing = tree.attach(second, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let mut pages = StackedContainer::new(
        "Pages",
        stack,
        vec![
            StackPage::new(first, Some(first_focus)),
            StackPage::new(second, Some(missing)),
        ],
        first,
    )
    .unwrap();
    pages.install(&mut tree).unwrap();
    tree.set_focus(Some(first_focus)).unwrap();
    tree.detach(missing).unwrap();

    assert_eq!(
        pages.select(&mut tree, second),
        Err(ContainerError::MissingNode(missing))
    );
    assert_eq!(pages.current(), first);
    assert_eq!(tree.focus(), Some(first_focus));
}
