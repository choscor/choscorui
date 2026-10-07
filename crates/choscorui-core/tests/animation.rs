use choscorui_core::{
    AnimationClock, AnimationKey, AnimationSpec, AnimationValues, Dirty, Easing, MotionTokens,
    Point, Rect, Size, Tree,
};
use std::time::Duration;

fn setup() -> (Tree, choscorui_core::NodeId) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));
    let node = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        )
        .unwrap();
    (tree, node)
}

#[test]
fn animation_advances_marks_only_paint_and_schedules_until_finished() {
    let (mut tree, node) = setup();
    tree.take_dirty(node).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    let mut clock = AnimationClock::new();
    let start = clock
        .animate(
            &mut tree,
            node,
            AnimationKey(1),
            AnimationValues {
                from: 0.0,
                to: 10.0,
            },
            AnimationSpec {
                duration: Duration::from_millis(100),
                easing: Easing::Linear,
            },
            MotionTokens {
                duration_ms: 100,
                reduced: false,
            },
        )
        .unwrap();
    assert!(!start.finished);
    let frame = clock.advance(&mut tree, Duration::from_millis(50)).unwrap();
    assert_eq!(frame.updates.len(), 1);
    assert!((frame.updates[0].value - 5.0).abs() < 0.001);
    assert!(frame.needs_frame);
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(!dirty.contains(Dirty::LAYOUT));
    let done = clock
        .advance(&mut tree, Duration::from_millis(100))
        .unwrap();
    assert_eq!(done.updates[0].value, 10.0);
    assert!(done.updates[0].finished);
    assert!(!done.needs_frame);
}

#[test]
fn interruption_starts_from_current_value_and_reduced_motion_finishes_immediately() {
    let (mut tree, node) = setup();
    let mut clock = AnimationClock::new();
    let spec = AnimationSpec {
        duration: Duration::from_millis(100),
        easing: Easing::Linear,
    };
    clock
        .animate(
            &mut tree,
            node,
            AnimationKey(2),
            AnimationValues {
                from: 0.0,
                to: 10.0,
            },
            spec,
            MotionTokens {
                duration_ms: 100,
                reduced: false,
            },
        )
        .unwrap();
    clock.advance(&mut tree, Duration::from_millis(40)).unwrap();
    let interrupted = clock
        .animate(
            &mut tree,
            node,
            AnimationKey(2),
            AnimationValues {
                from: 99.0,
                to: 20.0,
            },
            spec,
            MotionTokens {
                duration_ms: 100,
                reduced: false,
            },
        )
        .unwrap();
    assert!((interrupted.value - 4.0).abs() < 0.001);
    let immediate = clock
        .animate(
            &mut tree,
            node,
            AnimationKey(2),
            AnimationValues {
                from: 0.0,
                to: 30.0,
            },
            spec,
            MotionTokens {
                duration_ms: 100,
                reduced: true,
            },
        )
        .unwrap();
    assert_eq!(immediate.value, 30.0);
    assert!(immediate.finished);
    assert!(!clock.needs_frame());
}

#[test]
fn removing_node_cancels_animation_and_clock_rejects_backward_time() {
    let (mut tree, node) = setup();
    let mut clock = AnimationClock::new();
    clock
        .animate(
            &mut tree,
            node,
            AnimationKey(3),
            AnimationValues { from: 0.0, to: 1.0 },
            AnimationSpec {
                duration: Duration::from_secs(1),
                easing: Easing::EaseInOut,
            },
            MotionTokens {
                duration_ms: 1000,
                reduced: false,
            },
        )
        .unwrap();
    let eased = clock
        .advance(&mut tree, Duration::from_millis(250))
        .unwrap();
    assert!((eased.updates[0].value - 0.15625).abs() < 0.00001);
    tree.detach(node).unwrap();
    let frame = clock
        .advance(&mut tree, Duration::from_millis(300))
        .unwrap();
    assert!(frame.updates.is_empty());
    assert!(!frame.needs_frame);
    assert!(
        clock
            .advance(&mut tree, Duration::from_millis(250))
            .is_err()
    );
}
