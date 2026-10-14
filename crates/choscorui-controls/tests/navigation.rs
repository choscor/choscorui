use choscorui_controls::{
    CommandId, CommandRegistry, CommandState, DialogKeyRoute, DialogKeys, MnemonicTable,
    ScrollAction, ScrollArea, ScrollAreaInput, ShortcutModifiers, parse_mnemonic,
};
use choscorui_core::{
    Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect, ScrollViewport,
    SemanticAction, Size, Tree, WheelUnit,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

/// A 100×100 viewport at (0,0) over 100×1000 content with 20-point lines.
fn setup() -> (Tree, ScrollArea, NodeId) {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 300.0));
    let viewport = tree
        .attach(tree.root(), rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let content = tree
        .attach(viewport, rect(0.0, 0.0, 100.0, 1000.0))
        .unwrap();
    let item = tree.attach(content, rect(0.0, 0.0, 90.0, 20.0)).unwrap();
    tree.set_focusable(viewport, true).unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(100.0, 1000.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    (tree, ScrollArea::new(scroll), item)
}

fn key(tree: &mut Tree, area: &mut ScrollArea, key: Key) -> bool {
    area.handle(
        tree,
        ScrollAreaInput::Key(KeyEvent::Down { key, repeat: false }),
    )
    .unwrap()
}

#[test]
fn keyboard_alone_scrolls_by_line_page_and_ends() {
    let (mut tree, mut area, _) = setup();
    assert!(!key(&mut tree, &mut area, Key::ArrowDown), "needs focus");
    tree.set_focus(Some(area.node())).unwrap();
    assert!(key(&mut tree, &mut area, Key::ArrowDown));
    assert_eq!(area.viewport().offset().y, 20.0);
    key(&mut tree, &mut area, Key::PageDown);
    assert_eq!(area.viewport().offset().y, 100.0);
    key(&mut tree, &mut area, Key::Space);
    assert_eq!(area.viewport().offset().y, 180.0);
    key(&mut tree, &mut area, Key::PageUp);
    assert_eq!(area.viewport().offset().y, 100.0);
    key(&mut tree, &mut area, Key::End);
    assert_eq!(area.viewport().offset().y, 900.0);
    assert!(!key(&mut tree, &mut area, Key::ArrowDown), "clamped at end");
    key(&mut tree, &mut area, Key::Home);
    assert_eq!(area.viewport().offset().y, 0.0);
    area.handle(
        &mut tree,
        ScrollAreaInput::Semantic(SemanticAction::Increment),
    )
    .unwrap();
    assert_eq!(area.viewport().offset().y, 80.0);
    area.handle(&mut tree, ScrollAreaInput::Action(ScrollAction::Top))
        .unwrap();
    assert_eq!(area.viewport().offset().y, 0.0);
}

#[test]
fn thumb_drag_keeps_capture_outside_bounds_and_releases() {
    let (mut tree, mut area, _) = setup();
    let bar = area.vertical_scrollbar(&tree).unwrap();
    assert_eq!(bar.track, rect(90.0, 0.0, 10.0, 100.0));
    assert_eq!(bar.thumb, rect(90.0, 0.0, 10.0, 20.0));
    assert!(area.horizontal_scrollbar(&tree).is_none());

    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Down {
            position: Point::new(95.0, 5.0),
            button: PointerButton::Primary,
        }),
    )
    .unwrap();
    assert_eq!(tree.pointer_capture(), Some(area.node()));
    // Drag far below and to the right of the viewport: clamps to the end.
    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Move {
            position: Point::new(400.0, 500.0),
        }),
    )
    .unwrap();
    assert_eq!(area.viewport().offset().y, 900.0);
    assert!(area.visual_state(&tree).pressed());
    // Halfway along the 80-point travel is halfway through the content.
    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Move {
            position: Point::new(-50.0, 45.0),
        }),
    )
    .unwrap();
    assert_eq!(area.viewport().offset().y, 450.0);
    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Up {
            position: Point::new(-50.0, 45.0),
            button: PointerButton::Primary,
        }),
    )
    .unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert!(!area.visual_state(&tree).pressed());

    // Clicking the track pages toward the click.
    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Down {
            position: Point::new(95.0, 2.0),
            button: PointerButton::Primary,
        }),
    )
    .unwrap();
    assert_eq!(area.viewport().offset().y, 370.0);
}

#[test]
fn wheel_scrolls_only_over_the_area_and_cancel_releases_capture() {
    let (mut tree, mut area, item) = setup();
    let wheel = |x: f64| {
        ScrollAreaInput::Pointer(PointerEvent::Wheel {
            position: Point::new(x, 50.0),
            delta: Point::new(0.0, 2.0),
            unit: WheelUnit::Lines,
        })
    };
    assert!(!area.handle(&mut tree, wheel(200.0)).unwrap());
    assert!(area.handle(&mut tree, wheel(10.0)).unwrap());
    assert_eq!(area.viewport().offset().y, 40.0);
    assert_eq!(tree.bounds(item).unwrap().origin.y, -40.0);
    area.handle(
        &mut tree,
        ScrollAreaInput::Pointer(PointerEvent::Down {
            position: Point::new(95.0, 10.0),
            button: PointerButton::Primary,
        }),
    )
    .unwrap();
    assert_eq!(tree.pointer_capture(), Some(area.node()));
    area.handle(&mut tree, ScrollAreaInput::Cancel).unwrap();
    assert_eq!(tree.pointer_capture(), None);
    area.set_enabled(&mut tree, false).unwrap();
    assert!(!area.handle(&mut tree, wheel(10.0)).unwrap());
    let semantics = area.semantics(&tree, "Log");
    assert!(semantics.actions.is_empty());
    assert_eq!(semantics.value.as_deref(), Some("4%"));
}

#[test]
fn mnemonics_parse_escape_and_cycle_on_conflict() {
    assert_eq!(parse_mnemonic("&Save"), ("Save".into(), Some('s')));
    assert_eq!(parse_mnemonic("Save &As"), ("Save As".into(), Some('a')));
    assert_eq!(
        parse_mnemonic("Fish && &Chips"),
        ("Fish & Chips".into(), Some('c'))
    );
    assert_eq!(parse_mnemonic("Trailing&"), ("Trailing&".into(), None));
    assert_eq!(parse_mnemonic("&Đóng"), ("Đóng".into(), Some('đ')));

    let mut tree = Tree::new(rect(0.0, 0.0, 10.0, 10.0));
    let a = tree.attach(tree.root(), rect(0.0, 0.0, 1.0, 1.0)).unwrap();
    let b = tree.attach(tree.root(), rect(0.0, 0.0, 1.0, 1.0)).unwrap();
    let c = tree.attach(tree.root(), rect(0.0, 0.0, 1.0, 1.0)).unwrap();
    let mut table = MnemonicTable::new();
    assert_eq!(table.register("&Name", a), "Name");
    table.register("&Notes", b);
    table.register("&Open", c);
    let all = |_| true;
    let hit = table.resolve('O', None, all).unwrap();
    assert_eq!((hit.node, hit.activate), (c, true));
    let first = table.resolve('n', None, all).unwrap();
    assert_eq!((first.node, first.activate), (a, false));
    assert_eq!(table.resolve('n', Some(a), all).unwrap().node, b);
    assert_eq!(table.resolve('n', Some(b), all).unwrap().node, a);
    // A disabled sibling leaves a unique, activating mnemonic.
    let hit = table.resolve('n', None, |node| node != b).unwrap();
    assert_eq!((hit.node, hit.activate), (a, true));
    table.remove(c);
    assert!(table.resolve('o', None, all).is_none());
}

#[test]
fn dialog_keys_route_default_and_cancel_through_command_state() {
    let mut commands = CommandRegistry::new();
    let ok = CommandId(1);
    let cancel = CommandId(2);
    commands.insert(ok, CommandState::default()).unwrap();
    commands.insert(cancel, CommandState::default()).unwrap();
    let keys = DialogKeys {
        default: Some(ok),
        cancel: Some(cancel),
    };
    let down = |key| KeyEvent::Down { key, repeat: false };
    let none = ShortcutModifiers::default();
    let command = ShortcutModifiers {
        command: true,
        ..none
    };
    assert_eq!(
        keys.route(down(Key::Enter), none, false, &commands),
        DialogKeyRoute::Run(ok)
    );
    assert_eq!(
        keys.route(down(Key::Enter), none, true, &commands),
        DialogKeyRoute::Ignored
    );
    assert_eq!(
        keys.route(down(Key::Escape), none, false, &commands),
        DialogKeyRoute::Run(cancel)
    );
    assert_eq!(
        keys.route(down(Key::Character('.')), command, false, &commands),
        DialogKeyRoute::Run(cancel)
    );
    assert_eq!(
        keys.route(down(Key::Enter), command, false, &commands),
        DialogKeyRoute::Ignored
    );
    assert_eq!(
        keys.route(
            KeyEvent::Down {
                key: Key::Enter,
                repeat: true
            },
            none,
            false,
            &commands
        ),
        DialogKeyRoute::Ignored
    );
    commands.set_enabled(ok, false).unwrap();
    // A disabled default consumes Return, apart from a key that is not ours.
    assert_eq!(
        keys.route(down(Key::Enter), none, false, &commands),
        DialogKeyRoute::Disabled(ok)
    );
}

#[test]
fn two_bars_leave_the_corner_free() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 300.0));
    let viewport = tree
        .attach(tree.root(), rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let content = tree
        .attach(viewport, rect(0.0, 0.0, 1000.0, 1000.0))
        .unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(1000.0, 1000.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let mut area = ScrollArea::new(scroll);
    let vertical = area.vertical_scrollbar(&tree).unwrap();
    let horizontal = area.horizontal_scrollbar(&tree).unwrap();
    assert_eq!(vertical.track, rect(90.0, 0.0, 10.0, 90.0));
    assert_eq!(horizontal.track, rect(0.0, 90.0, 90.0, 10.0));
    assert!(!vertical.track.intersects(horizontal.track));
    // At the ends, the thumbs stop at the corner square.
    let end = ScrollAreaInput::Key(KeyEvent::Down {
        key: Key::End,
        repeat: false,
    });
    tree.set_focus(Some(area.node())).unwrap();
    area.handle(&mut tree, end).unwrap();
    assert_eq!(area.vertical_scrollbar(&tree).unwrap().thumb.max_y(), 90.0);
}
