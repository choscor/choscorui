use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect, Size, Tree,
    WheelUnit,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn pointer_event_follows_capture_target_bubble_order() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let parent = tree.attach(root, rect(5.0, 5.0, 90.0, 90.0)).unwrap();
    let child = tree.attach(parent, rect(10.0, 10.0, 20.0, 20.0)).unwrap();
    let mut calls = Vec::new();

    tree.dispatch_pointer(
        PointerEvent::Down {
            position: Point::new(15.0, 15.0),
            button: PointerButton::Primary,
        },
        |_, id, phase, _| {
            calls.push((id, phase));
            EventReply::Continue
        },
    );

    assert_eq!(
        calls,
        vec![
            (root, EventPhase::Capture),
            (parent, EventPhase::Capture),
            (child, EventPhase::Target),
            (parent, EventPhase::Bubble),
            (root, EventPhase::Bubble),
        ]
    );
}

#[test]
fn capture_routes_outside_pointer_to_capturing_node_until_released() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(10.0, 10.0, 20.0, 20.0)).unwrap();
    tree.set_pointer_capture(Some(child)).unwrap();
    let event = PointerEvent::Move {
        position: Point::new(200.0, 200.0),
    };
    let mut targets = Vec::new();
    tree.dispatch_pointer(event, |_, id, phase, _| {
        if phase == EventPhase::Target {
            targets.push(id);
        }
        EventReply::Continue
    });
    assert_eq!(targets, vec![child]);

    tree.set_pointer_capture(None).unwrap();
    tree.dispatch_pointer(event, |_, id, phase, _| {
        if phase == EventPhase::Target {
            targets.push(id);
        }
        EventReply::Continue
    });
    assert_eq!(targets, vec![child]);
}

#[test]
fn captured_wheel_keeps_delta_units_outside_bounds() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(10.0, 10.0, 20.0, 20.0)).unwrap();
    tree.set_pointer_capture(Some(child)).unwrap();
    let mut targets = Vec::new();
    tree.dispatch_pointer(
        PointerEvent::Wheel {
            position: Point::new(120.0, 120.0),
            delta: Point::new(0.0, -3.0),
            unit: WheelUnit::Lines,
        },
        |_, id, phase, event| {
            if phase == EventPhase::Target {
                targets.push((id, event));
            }
            EventReply::Continue
        },
    );
    assert_eq!(
        targets,
        vec![(
            child,
            PointerEvent::Wheel {
                position: Point::new(120.0, 120.0),
                delta: Point::new(0.0, -3.0),
                unit: WheelUnit::Lines,
            }
        )]
    );
}

#[test]
fn detaching_target_during_capture_skips_stale_dispatch_path() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let parent = tree.attach(root, rect(0.0, 0.0, 50.0, 50.0)).unwrap();
    let child = tree.attach(parent, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let mut calls = Vec::new();
    tree.dispatch_pointer(
        PointerEvent::Down {
            position: Point::new(10.0, 10.0),
            button: PointerButton::Primary,
        },
        |tree, id, phase, _| {
            calls.push((id, phase));
            if id == parent && phase == EventPhase::Capture {
                tree.detach(parent).unwrap();
            }
            EventReply::Continue
        },
    );
    assert_eq!(
        calls,
        vec![(root, EventPhase::Capture), (parent, EventPhase::Capture)]
    );
    assert!(!tree.contains(child));
}

#[test]
fn stop_reply_prevents_remaining_handlers() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let mut calls = Vec::new();
    tree.dispatch_pointer(
        PointerEvent::Down {
            position: Point::new(10.0, 10.0),
            button: PointerButton::Primary,
        },
        |_, id, phase, _| {
            calls.push((id, phase));
            if id == child {
                EventReply::Stop
            } else {
                EventReply::Continue
            }
        },
    );
    assert_eq!(
        calls,
        vec![(root, EventPhase::Capture), (child, EventPhase::Target)]
    );
}

#[test]
fn keyboard_event_targets_focused_node_then_bubbles_to_root() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let first = tree.attach(root, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let second = tree.attach(root, rect(30.0, 0.0, 20.0, 20.0)).unwrap();
    tree.set_focus(Some(second)).unwrap();
    let mut calls = Vec::new();
    tree.dispatch_key(
        KeyEvent::Down {
            key: Key::Enter,
            repeat: false,
        },
        |_, id, phase, event| {
            calls.push((id, phase, event));
            EventReply::Continue
        },
    );
    assert_eq!(
        calls,
        vec![
            (
                root,
                EventPhase::Capture,
                KeyEvent::Down {
                    key: Key::Enter,
                    repeat: false
                }
            ),
            (
                second,
                EventPhase::Target,
                KeyEvent::Down {
                    key: Key::Enter,
                    repeat: false
                }
            ),
            (
                root,
                EventPhase::Bubble,
                KeyEvent::Down {
                    key: Key::Enter,
                    repeat: false
                }
            ),
        ]
    );
    assert!(!calls.iter().any(|(id, _, _)| *id == first));
}

#[test]
fn keyboard_event_has_no_target_after_focus_is_removed() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    tree.set_focus(Some(child)).unwrap();
    tree.detach(child).unwrap();
    let mut count = 0;
    tree.dispatch_key(KeyEvent::Up { key: Key::Tab }, |_, _, _, _| {
        count += 1;
        EventReply::Continue
    });
    assert_eq!(count, 0);
}

#[test]
fn nested_event_completes_before_outer_dispatch_resumes() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    let mut calls = Vec::new();
    tree.dispatch_pointer(
        PointerEvent::Down {
            position: Point::new(10.0, 10.0),
            button: PointerButton::Primary,
        },
        |tree, id, phase, _| {
            calls.push((id, phase));
            if id == root && phase == EventPhase::Capture {
                tree.dispatch_pointer(
                    PointerEvent::Move {
                        position: Point::new(80.0, 80.0),
                    },
                    |_, nested_id, nested_phase, _| {
                        calls.push((nested_id, nested_phase));
                        EventReply::Continue
                    },
                );
            }
            EventReply::Continue
        },
    );
    assert_eq!(
        calls,
        vec![
            (root, EventPhase::Capture),
            (root, EventPhase::Target),
            (child, EventPhase::Target),
            (root, EventPhase::Bubble),
        ]
    );
}
