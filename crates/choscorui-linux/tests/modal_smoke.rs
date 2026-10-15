//! A modal window on a real display server: while it is open, the other
//! windows drop their input. The test runs only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`,
//! one at a time (`--test-threads=1`), and only on Wayland: the virtual
//! pointer of the session is a Wayland device.
#![cfg(target_os = "linux")]

use choscorui_core::{PointerEvent, WindowOptions};
use choscorui_linux::{Application, Window, WindowEvent};
use std::process::Command;
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
        && !std::env::var("CHOSCORUI_BACKEND").is_ok_and(|backend| backend == "x11")
}

/// Sends one command to the virtual input of the headless session.
fn virtual_input(command: &str) {
    use std::io::Write;
    let path = std::env::var("CHOSCORUI_VIRTUAL_INPUT").expect("run in headless-session.sh");
    let mut fifo = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    writeln!(fifo, "{command}").unwrap();
}

/// Moves the window titled `title` to `x`, `y` in sway, once sway
/// knows the window.
fn place(application: &Application, title: &str, x: u32, y: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = Command::new("swaymsg")
            .arg(format!("[title=\"{title}\"] move position {x} {y}"))
            .output()
            .unwrap();
        if output.status.success() {
            // Let the move land before the pointer goes there.
            let settle = Instant::now() + Duration::from_millis(200);
            while Instant::now() < settle {
                application
                    .dispatch_next_event(Duration::from_millis(10))
                    .unwrap();
            }
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
}

/// Opens a window titled `title` and waits until the compositor maps it.
fn shown(application: &Application, title: &str) -> Window {
    let window = application
        .create_window(WindowOptions::new(title, 240.0, 160.0))
        .unwrap();
    window.show();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !window.is_visible() {
        assert!(Instant::now() < deadline, "{title} did not show");
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
    window
}

/// Clicks at `x`, `y`, then dispatches events for a while, and returns
/// whether `window` saw a press.
fn click_reaches(application: &Application, window: &Window, x: u32, y: u32) -> bool {
    virtual_input(&format!("move {x} {y}"));
    let settle = Instant::now() + Duration::from_millis(100);
    while Instant::now() < settle {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
    }
    window.take_events();
    virtual_input("button 272 1");
    virtual_input("button 272 0");
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut pressed = false;
    while Instant::now() < deadline {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
        pressed |= window.take_events().iter().any(|event| {
            matches!(
                event,
                WindowEvent::Pointer {
                    event: PointerEvent::Down { .. },
                    ..
                }
            )
        });
    }
    pressed
}

#[test]
fn the_other_windows_ignore_input_while_a_modal_window_is_open() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let parent = shown(&application, "Modal parent");
    place(&application, "Modal parent", 100, 100);
    assert!(click_reaches(&application, &parent, 200, 180));

    let dialog = application
        .create_window(WindowOptions::new("Modal dialog", 200.0, 120.0))
        .unwrap();
    dialog.set_modal_parent(&parent);
    let session = application.begin_modal(&dialog);
    dialog.show();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dialog.is_visible() {
        assert!(Instant::now() < deadline, "the dialog did not show");
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
    place(&application, "Modal dialog", 700, 100);
    assert!(click_reaches(&application, &dialog, 760, 160));
    assert!(!click_reaches(&application, &parent, 200, 180));
    // The parent still has its frame, and the closing request waits.
    assert!(parent.is_visible());

    drop(session);
    dialog.close();
    assert!(click_reaches(&application, &parent, 200, 180));
}
