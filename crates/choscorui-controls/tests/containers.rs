mod hosted;

use choscorui_controls::{
    ContainerError, ControlMessage, ControlValue, CustomControl, EstimatedText, GroupBox,
    HostedResizable, MeasureContext, PaintContext, Resizable, ResizableInput, SplitOrientation,
    StackPage, StackedContainer, Styles,
};
use choscorui_core::{
    ColorRole, Constraints, DesignTokens, Dirty, DisplayCommand, DisplayList, Key, KeyEvent,
    LayoutDirection, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticRole, Size,
    Tree,
};
use hosted::Harness;

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
    let mut splitter = Resizable::new(
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
                ResizableInput::Key(KeyEvent::Down {
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
                ResizableInput::Semantic(SemanticAction::Decrement)
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
    let mut splitter = Resizable::new(
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
                ResizableInput::Key(KeyEvent::Down {
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
        vec![choscorui_core::SemanticRelation::LabelledBy(title)],
        "the members are children, not related nodes"
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

#[test]
fn group_box_paints_a_small_card_with_its_title() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 200.0));
    let group = tree
        .attach(tree.root(), rect(0.0, 0.0, 400.0, 124.0))
        .unwrap();
    let member = tree.attach(group, rect(16.0, 60.0, 100.0, 36.0)).unwrap();
    let box_control = GroupBox::new("Account", group, None, vec![member]).unwrap();
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let bounds = rect(0.0, 0.0, 400.0, 124.0);
    // 16 pt padding, a 21 pt title line, and a 16 pt gap.
    let content = box_control.content_rect(bounds, &styles, &tokens);
    assert_eq!(content, rect(16.0, 53.0, 368.0, 55.0));
    let mut list = DisplayList::new();
    box_control.paint(&mut PaintContext::new(
        &mut list,
        bounds,
        &tokens,
        &styles,
        &EstimatedText,
    ));
    let card = tokens.color(ColorRole::Card);
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRoundedRect { color, .. } if *color == card
    )));
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::DrawText { text, .. } if text == "Account"
    )));
}

fn down(position: Point) -> PointerEvent {
    PointerEvent::Down {
        position,
        button: PointerButton::Primary,
    }
}

fn up(position: Point) -> PointerEvent {
    PointerEvent::Up {
        position,
        button: PointerButton::Primary,
    }
}

fn moved(position: Point) -> PointerEvent {
    PointerEvent::Move { position }
}

#[test]
fn splitter_handle_drags_within_the_pane_minimums() {
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
    let mut splitter = Resizable::new(
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
    let mut input = |tree: &mut Tree, event| {
        splitter
            .handle(tree, ResizableInput::Pointer(event))
            .unwrap()
    };

    // A press on a pane is not a drag.
    assert_eq!(input(&mut tree, down(Point::new(50.0, 100.0))), None);
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(input(&mut tree, moved(Point::new(80.0, 100.0))), None);

    // The handle keeps its offset under the pointer.
    assert_eq!(input(&mut tree, down(Point::new(244.0, 100.0))), None);
    assert_eq!(tree.focus(), Some(handle));
    assert_eq!(tree.pointer_capture(), Some(handle));
    assert_eq!(
        input(&mut tree, moved(Point::new(304.0, 100.0))),
        Some(300.0)
    );
    assert_eq!(tree.bounds(first), Some(rect(0.0, 0.0, 300.0, 200.0)));
    assert_eq!(tree.bounds(handle), Some(rect(300.0, 0.0, 8.0, 200.0)));
    assert_eq!(
        input(&mut tree, moved(Point::new(-50.0, 100.0))),
        Some(100.0)
    );
    assert_eq!(
        input(&mut tree, moved(Point::new(900.0, 100.0))),
        Some(372.0)
    );
    assert_eq!(input(&mut tree, up(Point::new(900.0, 100.0))), None);
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(input(&mut tree, moved(Point::new(200.0, 100.0))), None);

    // A cancel ends a drag and keeps the position.
    input(&mut tree, down(Point::new(376.0, 100.0)));
    assert_eq!(
        input(&mut tree, moved(Point::new(304.0, 100.0))),
        Some(300.0)
    );
    assert_eq!(splitter.handle(&mut tree, ResizableInput::Cancel), Ok(None));
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(splitter.position(), 300.0);

    // The keys still move the handle, and the semantics do not change.
    assert_eq!(
        splitter
            .handle(
                &mut tree,
                ResizableInput::Key(KeyEvent::Down {
                    key: Key::ArrowLeft,
                    repeat: false,
                }),
            )
            .unwrap(),
        Some(290.0)
    );
    let semantics = splitter.semantics(&tree);
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.value.as_deref(), Some("290"));
}

fn hosted_sidebar() -> HostedResizable {
    Resizable::hosted(
        "Sidebar",
        SplitOrientation::Horizontal,
        50.0,
        50.0,
        4.0,
        100.0,
    )
    .unwrap()
}

fn drag(h: &mut Harness, from: Point, to: &[Point]) {
    h.host.dispatch_pointer(&mut h.tree, down(from));
    for point in to {
        h.host.dispatch_pointer(&mut h.tree, moved(*point));
    }
    let end = to.last().copied().unwrap_or(from);
    h.host.dispatch_pointer(&mut h.tree, up(end));
}

fn changes(values: &[f64]) -> Vec<ControlMessage> {
    values
        .iter()
        .map(|value| ControlMessage::Changed(ControlValue::Number(*value)))
        .collect()
}

#[test]
fn a_hosted_resizable_drags_its_handle_and_clamps_to_the_minimums() {
    let mut h = Harness::new(hosted_sidebar());
    h.place(rect(0.0, 0.0, 300.0, 100.0));
    let at = |x| Point::new(x, 50.0);

    // A press on a pane does not drag.
    drag(&mut h, at(20.0), &[at(60.0)]);
    assert!(h.messages().is_empty());

    // The handle is 100..104, and the pointer keeps its 2 pt offset.
    drag(&mut h, at(102.0), &[at(152.0), at(-40.0), at(400.0)]);
    assert_eq!(h.messages(), changes(&[150.0, 50.0, 246.0]));
    assert_eq!(h.tree.pointer_capture(), None);
    h.hover(at(120.0));
    assert!(h.messages().is_empty());

    // A loss of focus cancels a drag and keeps the position.
    h.host.dispatch_pointer(&mut h.tree, down(at(248.0)));
    assert_eq!(h.tree.pointer_capture(), Some(h.node));
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(h.tree.pointer_capture(), None);
    h.hover(at(150.0));
    assert!(h.messages().is_empty());

    // The keys still work, and the semantics are those of a slider.
    h.focus();
    h.key(Key::ArrowLeft);
    assert_eq!(h.messages(), changes(&[236.0]));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.name, "Sidebar");
    assert_eq!(semantics.value.as_deref(), Some("236"));
    assert_eq!(
        semantics.actions,
        vec![
            SemanticAction::Increment,
            SemanticAction::Decrement,
            SemanticAction::Focus
        ]
    );
}

#[test]
fn a_hosted_resizable_mirrors_its_drag_right_to_left() {
    let mut h = Harness::with_direction(hosted_sidebar(), LayoutDirection::Rtl);
    h.place(rect(0.0, 0.0, 300.0, 100.0));
    // The first pane is on the right, and its handle is 196..200.
    drag(&mut h, Point::new(198.0, 50.0), &[Point::new(148.0, 50.0)]);
    assert_eq!(h.messages(), changes(&[150.0]));
}

#[test]
fn a_hosted_resizable_drags_a_vertical_handle() {
    let resizable =
        Resizable::hosted("Rows", SplitOrientation::Vertical, 40.0, 40.0, 4.0, 60.0).unwrap();
    let mut h = Harness::new(resizable);
    h.place(rect(0.0, 0.0, 100.0, 200.0));
    drag(&mut h, Point::new(50.0, 62.0), &[Point::new(50.0, 92.0)]);
    assert_eq!(h.messages(), changes(&[90.0]));
}

#[test]
fn a_hosted_panel_group_moves_each_handle_between_its_neighbors() {
    let group = || {
        Resizable::hosted_group(
            "Panels",
            SplitOrientation::Horizontal,
            vec![50.0, 50.0, 50.0],
            4.0,
            vec![100.0, 100.0],
        )
        .unwrap()
    };
    let mut h = Harness::new(group());
    h.place(rect(0.0, 0.0, 400.0, 100.0));
    let at = |x| Point::new(x, 50.0);

    // The second handle is 204..208. It moves only the panes beside it.
    drag(&mut h, at(206.0), &[at(256.0), at(900.0)]);
    assert_eq!(h.messages(), changes(&[150.0, 242.0]));
    // The first handle is 100..104, and the middle pane has 242 pt.
    drag(&mut h, at(102.0), &[at(400.0)]);
    assert_eq!(h.messages(), changes(&[292.0]));
    assert_eq!(h.semantics().value.as_deref(), Some("292"));

    // Tab moves between the handles inside the node.
    let mut h = Harness::new(group());
    h.place(rect(0.0, 0.0, 400.0, 100.0));
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::Tab);
    h.key(Key::ArrowRight);
    assert_eq!(h.messages(), changes(&[110.0, 100.0]));
    assert_eq!(h.semantics().value.as_deref(), Some("100"));

    let mut control = group();
    arrange(&mut control, rect(0.0, 0.0, 400.0, 100.0));
    assert_eq!(control.sizes(), vec![100.0, 100.0, 192.0]);
    let (panes, handles) = control.rects();
    assert_eq!(panes[2], rect(208.0, 0.0, 192.0, 100.0));
    assert_eq!(
        handles,
        vec![rect(100.0, 0.0, 4.0, 100.0), rect(204.0, 0.0, 4.0, 100.0)]
    );

    for (minimums, sizes) in [(vec![50.0], vec![]), (vec![50.0, 50.0], vec![60.0, 60.0])] {
        assert!(matches!(
            Resizable::hosted_group("Bad", SplitOrientation::Vertical, minimums, 4.0, sizes),
            Err(ContainerError::InvalidGeometry)
        ));
    }
    assert!(matches!(
        Resizable::hosted_group(
            "Bad",
            SplitOrientation::Vertical,
            vec![50.0, 50.0, 50.0],
            4.0,
            vec![60.0, 20.0],
        ),
        Err(ContainerError::InvalidPosition)
    ));
}

#[test]
fn a_panel_group_names_the_moved_handle_and_takes_all_sizes() {
    let mut control = Resizable::hosted_group(
        "Panels",
        SplitOrientation::Horizontal,
        vec![50.0, 50.0, 50.0],
        4.0,
        vec![100.0, 100.0],
    )
    .unwrap();
    assert_eq!(control.minimum_extent(), 158.0);
    // Pane rectangles in other bounds, before any arrange, mirrored.
    let (panes, handles) = control.rects_in(rect(0.0, 0.0, 400.0, 100.0), true);
    assert_eq!(panes[0], rect(300.0, 0.0, 100.0, 100.0));
    assert_eq!(handles[1], rect(192.0, 0.0, 4.0, 100.0));
    assert_eq!(panes[2], rect(0.0, 0.0, 192.0, 100.0));

    assert!(control.set_sizes(&[60.0, 120.0]));
    for bad in [vec![60.0], vec![40.0, 120.0], vec![60.0, f64::NAN]] {
        assert!(!control.set_sizes(&bad), "{bad:?}");
    }
    arrange(&mut control, rect(0.0, 0.0, 400.0, 100.0));
    assert_eq!(control.sizes(), vec![60.0, 120.0, 212.0]);

    // After a drag of the second handle, `active` names it.
    let mut h = Harness::new(control);
    h.place(rect(0.0, 0.0, 400.0, 100.0));
    let at = |x| Point::new(x, 50.0);
    drag(&mut h, at(186.0), &[at(206.0)]);
    assert_eq!(h.messages(), changes(&[140.0]));
    // The value is the pane before the active handle: the middle one.
    assert_eq!(h.semantics().value.as_deref(), Some("140"));
}

#[test]
fn a_panel_group_with_handle_nodes_leaves_the_handles_to_them() {
    let group = || {
        Resizable::hosted_group(
            "Panels",
            SplitOrientation::Horizontal,
            vec![50.0, 50.0, 50.0],
            4.0,
            vec![100.0, 100.0],
        )
        .unwrap()
        .with_handle_nodes(true)
    };
    // The group node is a plain group: no focus, no actions, no input.
    let mut h = Harness::new(group());
    h.place(rect(0.0, 0.0, 400.0, 100.0));
    h.focus();
    h.key(Key::ArrowRight);
    drag(&mut h, Point::new(206.0, 50.0), &[Point::new(256.0, 50.0)]);
    assert_eq!(h.messages(), changes(&[]));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Group);
    assert!(semantics.actions.is_empty());
    assert_eq!(semantics.value, None);

    let mut control = group();
    assert!(!control.focusable());
    assert_eq!(control.handle_count(), 2);
    arrange(&mut control, rect(0.0, 0.0, 400.0, 100.0));
    // Each handle gives the pane before it and that pane's range.
    let first = control.handle_semantics(0);
    assert_eq!(first.role, SemanticRole::Slider);
    assert_eq!(first.value.as_deref(), Some("100"));
    assert_eq!(first.description, "50–150");
    assert_eq!(control.handle_semantics(1).description, "50–242");
    assert_eq!(control.handle_semantics(2).value, None);
}

/// Arranges `control` in `bounds` as a host would.
fn arrange(control: &mut HostedResizable, bounds: Rect) {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let cx = MeasureContext {
        constraints: Constraints::tight(bounds.size),
        viewport: bounds,
        direction: LayoutDirection::Ltr,
        tokens: &tokens,
        styles: &styles,
        text: &EstimatedText,
    };
    control.arrange(&cx, bounds);
}
