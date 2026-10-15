//! Window lifecycle on a real display server. The tests run only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`.
#![cfg(target_os = "linux")]

use choscorui_core::{
    Color, CursorShape, DisplayList, Image, Point, Rect, Size, Support, WindowGeometry,
    WindowOptions,
};
use choscorui_linux::{Application, BackendKind, Error, Painter, WindowEvent, capabilities};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

/// The display server that `CHOSCORUI_BACKEND` requests.
fn requested_backend() -> Option<BackendKind> {
    match std::env::var("CHOSCORUI_BACKEND").ok()?.as_str() {
        "wayland" => Some(BackendKind::Wayland),
        "x11" => Some(BackendKind::X11),
        _ => None,
    }
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
/// `gpu_painting`. On Wayland and on Xvfb, Mesa's llvmpipe gives the
/// GLES painter.
fn check_painter(painter: Painter) {
    let gpu = capabilities().gpu_painting;
    if std::env::var("CHOSCORUI_PAINTER").is_ok_and(|value| value == "cairo") {
        assert_eq!(painter, Painter::Cairo);
        assert_eq!(gpu, Support::Unsupported);
    } else {
        assert_eq!(painter, Painter::Gles);
        assert_eq!(gpu, Support::Supported);
    }
}

#[test]
fn a_window_shows_paints_retitles_and_closes() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    if let Some(requested) = requested_backend() {
        assert_eq!(application.backend(), requested);
    }
    let window = application
        .create_window(WindowOptions::new("Smoke", 320.0, 200.0))
        .unwrap();
    check_painter(window.painter());
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
    // A change paints again after the first image is released: on X11
    // with MIT-SHM, after its completion event.
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(20.0, 20.0), Size::new(50.0, 50.0)),
        Color::rgb8(200, 100, 40),
    );
    window.set_display_list(list);
    assert!(pump_until(&application, || window.paint_count() > 1));
    window.set_title("Renamed").unwrap();
    assert_eq!(window.title(), "Renamed");
    assert!(window.set_title("bad\0title").is_err());
    // The bell request must not break the connection.
    application.beep();
    application
        .dispatch_next_event(Duration::from_millis(10))
        .unwrap();
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
    check_painter(window.painter());
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
    check_painter(window.painter());
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

#[test]
fn x11_geometry_round_trips_on_an_unmapped_window() {
    if !enabled() || requested_backend() != Some(BackendKind::X11) {
        return;
    }
    let application = Application::new().unwrap();
    assert_eq!(application.backend(), BackendKind::X11);
    // Xvfb runs no window manager. The window stays unmapped, so the
    // server applies the position and the size, and the state goes to
    // the `_NET_WM_STATE` property directly.
    let window = application
        .create_window(WindowOptions::new("Geometry", 320.0, 200.0))
        .unwrap();
    let placed = WindowGeometry {
        frame: Rect::new(Point::new(40.0, 30.0), Size::new(400.0, 300.0)),
        zoomed: true,
        full_screen: false,
    };
    window.set_geometry(&placed).unwrap();
    assert_eq!(window.geometry(), placed);
    let full_screen = WindowGeometry {
        zoomed: false,
        full_screen: true,
        ..placed
    };
    window.set_geometry(&full_screen).unwrap();
    assert_eq!(window.geometry(), full_screen);
    let mut resized = false;
    assert!(pump_until(&application, || {
        resized |= window.take_events().iter().any(|event| {
            matches!(event, WindowEvent::Resized { size, .. } if *size == Size::new(400.0, 300.0))
        });
        resized
    }));
    assert_eq!(window.content_size(), Size::new(400.0, 300.0));
    // The screen of Xvfb is 1280 x 800 at scale 1.
    assert!(
        application
            .screens()
            .contains(&Rect::new(Point::new(0.0, 0.0), Size::new(1280.0, 800.0)))
    );
    // Named and custom cursors apply; the image becomes a RENDER cursor.
    window.set_cursor(CursorShape::IBeam).unwrap();
    let image = Image::from_fn(16, 16, |x, _| [255, 0, 0, if x < 8 { 255 } else { 128 }]);
    window
        .set_cursor(CursorShape::Custom {
            image,
            hotspot: Point::new(8.0, 8.0),
        })
        .unwrap();
    window.close();
    application
        .dispatch_next_event(Duration::from_millis(10))
        .unwrap();
}

/// The name of the test that runs in child processes with a broken
/// `DISPLAY`.
const WITHOUT_DISPLAY: &str = "x11_without_a_display_fails_with_no_display";

#[test]
fn x11_without_a_display_fails_with_no_display() {
    // The environment of a process is global, so each case runs in a
    // child process: one without `DISPLAY`, and one whose `DISPLAY`
    // names no server.
    let Some(case) = std::env::var_os("CHOSCORUI_TEST_DISPLAY_CASE") else {
        for case in ["unset", "missing"] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    WITHOUT_DISPLAY,
                    "--exact",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("CHOSCORUI_TEST_DISPLAY_CASE", case)
                .env("CHOSCORUI_BACKEND", "x11")
                .env_remove("WAYLAND_DISPLAY");
            if case == "unset" {
                command.env_remove("DISPLAY");
            } else {
                command.env("DISPLAY", ":4321");
            }
            let status = command.status().unwrap();
            assert!(status.success(), "the {case} case failed: {status}");
        }
        return;
    };
    match Application::new() {
        Err(Error::NoDisplay(reason)) => assert!(!reason.is_empty(), "{case:?}"),
        Err(error) => panic!("{case:?}: unexpected error {error}"),
        Ok(_) => panic!("{case:?}: an application without a display server"),
    }
}

/// The name of the test that runs in a child process on a private Xvfb.
const SCALE: &str = "x11_scale_follows_the_physical_size_then_xft_dpi";

#[test]
fn x11_scale_follows_the_physical_size_then_xft_dpi() {
    if let Some(display) = std::env::var_os("CHOSCORUI_TEST_SCALE_DISPLAY") {
        check_scale(&display);
        return;
    }
    if !enabled() || requested_backend() != Some(BackendKind::X11) {
        return;
    }
    // A private Xvfb at 192 DPI without RandR: the physical size of the
    // screen gives the scale, and `xrdb` does not change the other tests.
    let number = (200..400)
        .map(|offset| offset + std::process::id() % 200)
        .find(|number| !std::path::Path::new(&format!("/tmp/.X{number}-lock")).exists())
        .unwrap();
    let display = format!(":{number}");
    let mut server = std::process::Command::new("Xvfb")
        .args([
            display.as_str(),
            "-screen",
            "0",
            "1280x800x24",
            "-dpi",
            "192",
            "-extension",
            "RANDR",
            "-noreset",
            "-nolisten",
            "tcp",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let socket = format!("/tmp/.X11-unix/X{number}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !std::path::Path::new(&socket).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([SCALE, "--exact", "--nocapture", "--test-threads=1"])
        .env("CHOSCORUI_TEST_SCALE_DISPLAY", &display)
        .env("DISPLAY", &display)
        .env("CHOSCORUI_BACKEND", "x11")
        .env_remove("WAYLAND_DISPLAY")
        .status();
    server.kill().unwrap();
    server.wait().unwrap();
    assert!(status.unwrap().success(), "the child on {display} failed");
}

/// Checks the scale of a window on the private Xvfb of `display`: 2 from
/// 192 DPI, then 1.5 when `Xft.dpi` becomes 144.
fn check_scale(display: &std::ffi::OsStr) {
    let application = Application::new().unwrap();
    assert_eq!(application.backend(), BackendKind::X11);
    let window = application
        .create_window(WindowOptions::new("Scale", 320.0, 200.0))
        .unwrap();
    window.show();
    assert!(pump_until(&application, || window.paint_count() > 0));
    assert_eq!(window.backing_scale_factor(), 2.0);
    assert_eq!(window.content_size(), Size::new(320.0, 200.0));
    // Each monitor is in its own scale.
    assert_eq!(
        application.screens(),
        vec![Rect::new(Point::new(0.0, 0.0), Size::new(640.0, 400.0))]
    );
    window.take_events();
    // `Xft.dpi` wins over the physical size.
    let mut xrdb = std::process::Command::new("xrdb")
        .arg("-merge")
        .env("DISPLAY", display)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut input = xrdb.stdin.take().unwrap();
        input.write_all(b"Xft.dpi: 144\n").unwrap();
    }
    assert!(xrdb.wait().unwrap().success());
    let mut events = Vec::new();
    let resized = |event: &WindowEvent| {
        matches!(event, WindowEvent::Resized { size, scale }
            if *scale == 1.5 && *size == Size::new(320.0, 200.0))
    };
    assert!(pump_until(&application, || {
        events.extend(window.take_events());
        events.iter().any(resized)
    }));
    // `ScaleChanged` comes first, and the window keeps its logical size.
    let changed = events
        .iter()
        .position(|event| matches!(event, WindowEvent::ScaleChanged { scale } if *scale == 1.5))
        .unwrap();
    assert!(changed < events.iter().position(resized).unwrap());
    assert_eq!(window.backing_scale_factor(), 1.5);
    assert_eq!(window.content_size(), Size::new(320.0, 200.0));
    window.close();
    application
        .dispatch_next_event(Duration::from_millis(10))
        .unwrap();
}
