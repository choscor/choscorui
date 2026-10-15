//! Window lifecycle on a real display server. The tests run only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`.
#![cfg(target_os = "linux")]

use choscorui_core::{Color, DisplayList, Point, Rect, Size, WindowOptions};
use choscorui_linux::{Application, Painter, WindowEvent};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

/// Dispatches events until `done` is true, for at most five seconds.
fn pump_until(application: &Application, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
    done()
}

#[test]
fn a_window_shows_paints_retitles_and_closes() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let window = application
        .create_window(WindowOptions::new("Smoke", 320.0, 200.0))
        .unwrap();
    assert_eq!(window.painter(), Painter::Cairo);
    window.show();
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(10.0, 10.0), Size::new(50.0, 50.0)),
        Color::rgb8(40, 100, 200),
    );
    window.set_display_list(list);
    assert!(pump_until(&application, || window.paint_count() > 0));
    assert!(window.is_visible());
    assert!(window.content_size().width > 0.0);
    let events = window.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WindowEvent::Resized { .. }))
    );
    window.set_title("Renamed").unwrap();
    assert_eq!(window.title(), "Renamed");
    assert!(window.set_title("bad\0title").is_err());
    assert_eq!(application.window_count(), 1);
    window.close();
    assert_eq!(application.window_count(), 0);
    application
        .dispatch_next_event(Duration::from_millis(10))
        .unwrap();
}

#[test]
fn posted_work_and_timers_run_on_the_ui_thread() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let poster = application.poster();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        poster.post(move || sender.send(1).unwrap()).unwrap();
    })
    .join()
    .unwrap();
    let fired = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = std::rc::Rc::clone(&fired);
    let _timer = application
        .schedule_repeating(Duration::from_millis(10), move || {
            counter.set(counter.get() + 1);
        })
        .unwrap();
    assert!(pump_until(&application, || fired.get() >= 3
        && receiver.try_recv().is_ok()));
    // A panic in posted work stops at the boundary and is counted.
    application
        .poster()
        .post(|| panic!("posted panic"))
        .unwrap();
    assert!(pump_until(&application, || application
        .posted_panic_count()
        == 1));
    // After the application drops, posting fails.
    let poster = application.poster();
    drop(application);
    assert!(poster.post(|| {}).is_err());
}
