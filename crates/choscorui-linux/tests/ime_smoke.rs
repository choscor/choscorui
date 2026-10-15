//! Input methods on a real display server. The virtual input of
//! `scripts/linux/headless-session.sh` is the input method of the sway
//! seat. The tests run only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, one at a
//! time (`--test-threads=1`), and only on Wayland: X11 uses XIM.
#![cfg(target_os = "linux")]

use choscorui_core::{Point, Rect, Size, WindowOptions};
use choscorui_linux::{Application, ImeEvent, TextInputState, Window, WindowEvent};
use std::io::Write;
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
        && !std::env::var("CHOSCORUI_BACKEND").is_ok_and(|backend| backend == "x11")
}

fn fifo() -> String {
    std::env::var("CHOSCORUI_VIRTUAL_INPUT").expect("run in headless-session.sh")
}

/// Sends one command to the input method.
fn input_method(command: &str) {
    let mut fifo = std::fs::OpenOptions::new()
        .write(true)
        .open(fifo())
        .unwrap();
    writeln!(fifo, "{command}").unwrap();
}

/// The lines that the input method logged.
fn log() -> Vec<String> {
    std::fs::read_to_string(format!("{}.log", fifo()))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Dispatches events until `done` gives a value, for at most five
/// seconds.
fn pump_until<T>(application: &Application, mut done: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = done() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
    }
}

/// Waits for the next input method event of `window`.
fn next_ime(application: &Application, window: &Window, seen: &mut Vec<ImeEvent>) -> ImeEvent {
    let start = seen.len();
    pump_until(application, || {
        seen.extend(
            window
                .take_events()
                .into_iter()
                .filter_map(|event| match event {
                    WindowEvent::Ime(event) => Some(event),
                    _ => None,
                }),
        );
        seen.get(start).cloned()
    })
    .unwrap_or_else(|| panic!("no input method event after {seen:?}"))
}

fn state(active: bool, text: &str, caret: usize) -> TextInputState {
    TextInputState {
        active,
        text: text.to_owned(),
        selected_utf16: caret..caret,
        marked_utf16: None,
        caret: Rect::new(Point::new(40.0, 10.0), Size::new(1.0, 16.0)),
        captures_keys: false,
    }
}

#[test]
fn an_input_method_marks_commits_and_deletes_text() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let window = application
        .create_window(WindowOptions::new("IME", 240.0, 120.0))
        .unwrap();
    window.show();
    assert!(pump_until(&application, || window.is_visible().then_some(())).is_some());
    let before = log().len();
    window.set_text_input_state(state(true, "héllo", 5));
    let logged = pump_until(&application, || {
        let lines = log();
        lines[before.min(lines.len())..]
            .iter()
            .any(|line| line.starts_with("surrounding"))
            .then_some(lines)
    })
    .expect("the input method saw no surrounding text");
    assert!(
        logged[before..].contains(&"activate".to_owned()),
        "{logged:?}"
    );
    // The cursor and the anchor are byte offsets.
    assert!(
        logged[before..].contains(&"surrounding héllo|6|6".to_owned()),
        "{logged:?}"
    );

    let mut seen = Vec::new();
    input_method("preedit 日本");
    assert_eq!(
        next_ime(&application, &window, &mut seen),
        ImeEvent::SetMarkedText {
            text: "日本".into(),
            selected_utf16: 2..2,
            replacement_utf16: None,
        }
    );
    input_method("commit 日本語");
    assert_eq!(
        next_ime(&application, &window, &mut seen),
        ImeEvent::CommitText {
            text: "日本語".into(),
            replacement_utf16: None,
        }
    );
    // Two bytes before the cursor are "o" and "l".
    input_method("delete 2 0");
    assert_eq!(
        next_ime(&application, &window, &mut seen),
        ImeEvent::CommitText {
            text: String::new(),
            replacement_utf16: Some(3..5),
        }
    );

    let before = log().len();
    window.set_text_input_state(state(false, "", 0));
    assert!(
        pump_until(&application, || log()[before..]
            .contains(&"deactivate".to_owned())
            .then_some(()))
        .is_some(),
        "{:?}",
        log()
    );
}
