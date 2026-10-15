//! Window lifecycle on a real display server. The tests run only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`.
#![cfg(target_os = "linux")]

use choscorui_core::{Color, DisplayList, Point, Rect, Size, Support, WindowOptions};
use choscorui_linux::{Application, BackendKind, Painter, WindowEvent, capabilities};
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

/// Checks the painter of a new window against `CHOSCORUI_PAINTER` and
/// `gpu_painting`. On Wayland, Mesa's llvmpipe gives the GLES painter.
fn check_painter(application: &Application, painter: Painter) {
    let gpu = capabilities().gpu_painting;
    if std::env::var("CHOSCORUI_PAINTER").is_ok_and(|value| value == "cairo") {
        assert_eq!(painter, Painter::Cairo);
        assert_eq!(gpu, Support::Unsupported);
    } else if application.backend() == BackendKind::Wayland {
        assert_eq!(painter, Painter::Gles);
        assert_eq!(gpu, Support::Supported);
    } else {
        let expected = match gpu {
            Support::Supported => Painter::Gles,
            _ => Painter::Cairo,
        };
        assert_eq!(painter, expected);
    }
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
    check_painter(&application, window.painter());
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
    // The capture renders the display list with Cairo, for both painters.
    let image = window.capture().unwrap();
    let scale = window.backing_scale_factor();
    let at = ((20.0 * scale) as usize * image.width() as usize + (20.0 * scale) as usize) * 4;
    assert_eq!(&image.pixels()[at..at + 4], &[40, 100, 200, 255]);
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
fn a_window_repaints_each_change() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let window = application
        .create_window(WindowOptions::new("Repaint", 200.0, 120.0))
        .unwrap();
    check_painter(&application, window.painter());
    window.show();
    for step in 0..6u8 {
        let mut list = DisplayList::new();
        list.fill_rect(
            Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 120.0)),
            Color::WHITE,
        );
        list.fill_rect(
            Rect::new(
                Point::new(f64::from(step) * 20.0, 10.0),
                Size::new(15.0, 15.0),
            ),
            Color::rgb8(200, 40, 40),
        );
        let before = window.paint_count();
        window.set_display_list(list);
        assert!(pump_until(&application, || window.paint_count() > before));
    }
    // No GPU failure switched the window.
    check_painter(&application, window.painter());
    window.close();
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
