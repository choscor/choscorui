//! Toaster and the toast queue through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ShortcutModifiers, TOAST_DURATION, Toast, ToastPosition, ToastVariant, Toaster,
    Toasts,
};
use choscorui_core::{Key, KeyEvent, LayoutDirection, Point, Rect};
use hosted::{Harness, texts};

fn advance(h: &mut Harness, time: f64) -> bool {
    h.set_time(time);
    let fired = h.host.advance(&mut h.tree, time);
    let bounds = h.bounds();
    h.place(bounds);
    fired
}

fn stack(h: &Harness) -> Option<Rect> {
    h.host.get(h.node).unwrap().overlay()
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

#[test]
fn pushed_toasts_show_at_the_next_advance_and_close_on_time() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    assert_eq!(stack(&h), None);
    toasts.push(Toast::new("Event has been created").with_description("Sunday at 9:00"));
    assert_eq!(h.host.next_deadline(&h.tree), Some(f64::NEG_INFINITY));
    assert!(advance(&mut h, 0.0));
    let rect = stack(&h).expect("shown");
    // 384 pt wide, 16 pt from the bottom-end corner of the 800 by 600 window.
    assert_eq!(rect.origin.x, 800.0 - 16.0 - 384.0);
    assert_eq!(rect.max_y(), 600.0 - 16.0);
    h.set_time(1.0);
    let painted = texts(&h.paint());
    assert!(painted.contains(&"Event has been created".to_string()));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("Event has been created\nSunday at 9:00")
    );
    assert_eq!(h.host.next_deadline(&h.tree), Some(TOAST_DURATION));
    assert!(advance(&mut h, TOAST_DURATION));
    let closing = h.host.next_deadline(&h.tree).expect("the exit animation");
    assert!(closing > TOAST_DURATION);
    advance(&mut h, closing);
    assert_eq!(stack(&h), None);
    assert_eq!(h.host.next_deadline(&h.tree), None);
}

#[test]
fn hover_expands_the_stack_and_pauses_the_timers() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    for title in ["One", "Two", "Three", "Four"] {
        toasts.push(Toast::new(title));
    }
    advance(&mut h, 0.0);
    let collapsed = stack(&h).unwrap();
    h.set_time(1.0);
    assert_eq!(
        texts(&h.paint()),
        ["Four"],
        "only the front toast shows text"
    );
    h.hover(Point::new(collapsed.center().x, collapsed.max_y() - 5.0));
    let bounds = h.bounds();
    h.place(bounds);
    let expanded = stack(&h).unwrap();
    assert!(
        expanded.size.height > collapsed.size.height * 3.0,
        "all four"
    );
    assert_eq!(h.host.next_deadline(&h.tree), None, "paused");
    h.set_time(3.0);
    h.hover(Point::new(10.0, 10.0));
    assert_eq!(
        h.host.next_deadline(&h.tree),
        Some(3.0 + TOAST_DURATION - 1.0),
        "the timers resume with what was left"
    );
}

#[test]
fn keys_reach_the_action_and_close_buttons() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    let id = toasts.push(Toast::new("Message deleted").with_action("Undo"));
    advance(&mut h, 0.0);
    h.focus();
    assert_eq!(h.semantics().description, "Undo");
    assert!(tab(&mut h));
    assert_eq!(h.semantics().description, "Close");
    assert!(!tab(&mut h), "Tab leaves after the last button");
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    assert!(tab(&mut h));
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(id.0 as usize)]);
    assert!(h.host.next_deadline(&h.tree).is_some(), "the toast closes");
}

#[test]
fn a_loading_toast_stays_until_it_is_updated() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    let id = toasts.push(Toast::new("Saving...").with_variant(ToastVariant::Loading));
    advance(&mut h, 0.0);
    assert_eq!(h.host.next_deadline(&h.tree), None);
    assert!(h.host.animating(&h.tree, 10.0), "the loader turns");
    toasts.update(id, Toast::new("Saved").with_variant(ToastVariant::Success));
    advance(&mut h, 10.0);
    assert_eq!(h.semantics().value.as_deref(), Some("Saved"));
    assert_eq!(h.host.next_deadline(&h.tree), Some(10.0 + TOAST_DURATION));
    toasts.dismiss(id);
    advance(&mut h, 11.0);
    assert!(h.host.next_deadline(&h.tree).unwrap() > 11.0, "closing");
}

#[test]
fn the_position_follows_the_reading_direction() {
    let toasts = Toasts::new();
    let mut h = Harness::with_direction(
        Toaster::new("Notifications", toasts.clone(), "Close")
            .with_position(ToastPosition::TopStart),
        LayoutDirection::Rtl,
    );
    toasts.push(Toast::new("Copied"));
    advance(&mut h, 0.0);
    let rect = stack(&h).unwrap();
    assert_eq!(rect.origin.y, 16.0);
    assert_eq!(rect.max_x(), 800.0 - 16.0, "the start is at the right");
}
