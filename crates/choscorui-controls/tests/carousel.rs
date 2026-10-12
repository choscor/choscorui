//! Carousel through the control host.

mod hosted;

use choscorui_controls::{Carousel, ControlMessage, ControlValue, Orientation, ShortcutModifiers};
use choscorui_core::{
    Key, KeyEvent, LayoutDirection, Point, PointerButton, PointerEvent, SemanticAction, WheelUnit,
};
use hosted::{Harness, texts};

fn slides() -> Carousel {
    Carousel::new(
        "Gallery",
        "Previous slide",
        "Next slide",
        ["1", "2", "3", "4", "5"].map(String::from).to_vec(),
    )
}

fn changed(slide: usize) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Index(Some(slide)))
}

fn pointer(h: &mut Harness, event: PointerEvent) {
    h.host.dispatch_pointer(&mut h.tree, event);
}

#[test]
fn arrow_keys_and_buttons_move_one_slide() {
    let mut h = Harness::new(slides());
    // 320 pt of slides between two 32 pt buttons with 16 pt gaps.
    assert_eq!(h.measure().width, 416.0);
    assert_eq!(h.measure().height, 320.0);
    assert_eq!(texts(&h.paint()), ["1"]);
    h.focus();
    assert_eq!(
        h.semantics().description,
        "Next slide",
        "Previous is disabled"
    );
    h.key(Key::ArrowLeft);
    assert!(h.messages().is_empty(), "the first slide stops");
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.messages(), [changed(1), changed(2)]);
    assert_eq!(h.semantics().value.as_deref(), Some("3"));
    h.set_time(1.0);
    assert_eq!(texts(&h.paint()), ["3"], "the animation ended");
    let tab_back = |h: &mut Harness| {
        h.modifiers(ShortcutModifiers {
            shift: true,
            ..ShortcutModifiers::default()
        });
        let moved = h.host.dispatch_key(
            &mut h.tree,
            KeyEvent::Down {
                key: Key::Tab,
                repeat: false,
            },
        );
        h.modifiers(ShortcutModifiers::default());
        moved
    };
    assert!(tab_back(&mut h));
    assert_eq!(h.semantics().description, "Previous slide");
    assert!(!tab_back(&mut h), "Shift+Tab leaves the carousel");
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 16.0, bounds.center().y));
    assert_eq!(h.messages(), [changed(1)]);
    assert!(h.perform(SemanticAction::Increment, None));
    assert!(h.perform(SemanticAction::Decrement, None));
    assert_eq!(h.messages(), [changed(2), changed(1)]);
}

#[test]
fn a_drag_snaps_to_the_nearest_slide() {
    let mut h = Harness::new(slides());
    let center = h.bounds().center();
    let at = |dx: f64| Point::new(center.x + dx, center.y);
    let primary = PointerButton::Primary;
    pointer(
        &mut h,
        PointerEvent::Down {
            position: at(0.0),
            button: primary,
        },
    );
    pointer(
        &mut h,
        PointerEvent::Move {
            position: at(-200.0),
        },
    );
    assert_eq!(texts(&h.paint()), ["1", "2"], "both slides show mid-drag");
    pointer(
        &mut h,
        PointerEvent::Up {
            position: at(-200.0),
            button: primary,
        },
    );
    assert_eq!(
        h.messages(),
        [changed(1)],
        "past half a slide snaps forward"
    );
    pointer(
        &mut h,
        PointerEvent::Down {
            position: at(0.0),
            button: primary,
        },
    );
    pointer(
        &mut h,
        PointerEvent::Move {
            position: at(100.0),
        },
    );
    pointer(
        &mut h,
        PointerEvent::Up {
            position: at(100.0),
            button: primary,
        },
    );
    assert!(h.messages().is_empty(), "a short drag snaps back");
}

#[test]
fn a_wheel_scrolls_and_snaps_after_a_pause() {
    let mut h = Harness::new(slides());
    let center = h.bounds().center();
    pointer(
        &mut h,
        PointerEvent::Wheel {
            position: center,
            delta: Point::new(500.0, 0.0),
            unit: WheelUnit::Points,
        },
    );
    let deadline = h.host.next_deadline(&h.tree).expect("a snap waits");
    assert!(h.host.advance(&mut h.tree, deadline));
    assert_eq!(h.messages(), [changed(1)]);
    assert_eq!(h.host.next_deadline(&h.tree), None);
}

#[test]
fn vertical_and_right_to_left_carousels_follow_their_axis() {
    let mut vertical = Harness::new(
        slides()
            .with_orientation(Orientation::Vertical)
            .with_per_view(2),
    );
    vertical.focus();
    vertical.key(Key::ArrowRight);
    vertical.key(Key::ArrowDown);
    assert_eq!(vertical.messages(), [changed(1)]);
    for _ in 0..5 {
        vertical.key(Key::ArrowDown);
    }
    assert_eq!(
        vertical.semantics().value.as_deref(),
        Some("4"),
        "two slides show, so the fourth is the last first slide"
    );

    let mut rtl = Harness::with_direction(slides(), LayoutDirection::Rtl);
    rtl.focus();
    rtl.key(Key::ArrowLeft);
    assert_eq!(rtl.messages(), [changed(1)]);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.max_x() - 16.0, bounds.center().y));
    assert_eq!(rtl.messages(), [changed(0)], "Previous is at the right end");
}
