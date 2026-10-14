//! Toaster and the toast queue through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ShortcutModifiers, TOAST_DURATION, Toast, ToastPosition, ToastVariant, Toaster,
    Toasts,
};
use choscorui_core::{AnnouncementPriority, Key, KeyEvent, LayoutDirection, Point, Rect};
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
fn a_new_toast_is_spoken_and_an_error_interrupts() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    toasts.push(Toast::new("Event has been created").with_description("Sunday at 9:00"));
    toasts.push(Toast::new("Upload failed").with_variant(ToastVariant::Error));
    advance(&mut h, 0.0);
    assert_eq!(
        h.host.take_announcements(),
        [
            (
                "Event has been created, Sunday at 9:00".to_owned(),
                AnnouncementPriority::Medium
            ),
            ("Upload failed".to_owned(), AnnouncementPriority::High),
        ]
    );
    advance(&mut h, 1.0);
    assert!(h.host.take_announcements().is_empty(), "spoken once");
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
    assert_eq!(h.focused_part(), "Undo");
    assert!(tab(&mut h));
    assert_eq!(h.focused_part(), "Close");
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

#[test]
fn escape_closes_the_focused_toast_and_reports_its_id() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    let id = toasts.push(Toast::new("Message deleted").with_action("Undo"));
    advance(&mut h, 0.0);
    h.focus();
    h.key(Key::Escape);
    assert_eq!(h.messages(), [ControlMessage::ItemDismissed(id.0 as usize)]);
    assert!(h.host.next_deadline(&h.tree).is_some(), "the toast closes");
}

#[test]
fn the_close_button_and_the_timer_report_the_toast_id() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    let first = toasts.push(Toast::new("One"));
    advance(&mut h, 0.0);
    advance(&mut h, TOAST_DURATION);
    assert_eq!(
        h.messages(),
        [ControlMessage::ItemDismissed(first.0 as usize)]
    );
    let second = toasts.push(Toast::new("Two").with_duration(None));
    advance(&mut h, 10.0);
    let parts = h.parts();
    let close = parts
        .parts()
        .iter()
        .find(|part| part.node.name == "Close")
        .expect("the close button");
    assert!(close.bounds.size.width >= 20.0 && close.bounds.size.height >= 20.0);
    // A press at the edge of the 20 pt target, outside the icon, closes it.
    h.click(Point::new(
        close.bounds.origin.x + 1.0,
        close.bounds.origin.y + 1.0,
    ));
    assert_eq!(
        h.messages(),
        [ControlMessage::ItemDismissed(second.0 as usize)]
    );
}

#[test]
fn a_variant_after_loading_restores_the_duration() {
    let toast = Toast::new("Saving...").with_variant(ToastVariant::Loading);
    assert_eq!(toast.duration, None);
    let done = toast.with_variant(ToastVariant::Success);
    assert_eq!(done.duration, Some(TOAST_DURATION));
    let kept = Toast::new("Pinned")
        .with_duration(None)
        .with_variant(ToastVariant::Info);
    assert_eq!(kept.duration, None, "an explicit duration stays");
}

#[test]
fn a_hidden_window_pauses_the_timers() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    toasts.push(Toast::new("Saved"));
    advance(&mut h, 0.0);
    h.set_time(1.0);
    h.host.set_window_visible(&mut h.tree, false);
    assert_eq!(h.host.next_deadline(&h.tree), None, "paused while hidden");
    assert!(!advance(&mut h, 10.0));
    assert!(stack(&h).is_some(), "the toast waits for the window");
    h.host.set_window_visible(&mut h.tree, true);
    assert_eq!(
        h.host.next_deadline(&h.tree),
        Some(10.0 + TOAST_DURATION - 1.0),
        "the timer resumes with what was left"
    );
}
