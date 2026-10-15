//! Input methods on a real display server. The virtual input of
//! `scripts/linux/headless-session.sh` is the input method of the sway
//! seat. The tests run only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, one at a
//! time (`--test-threads=1`). The text-input tests run only on Wayland.
//! On X11, one test runs with `XMODIFIERS=@im=none`, the local input
//! method of Xlib. The XIM server test needs a real XIM server, which the
//! CI image does not have.
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

/// True for an X11 smoke run whose `XMODIFIERS` passes `accept`.
fn x11_with_modifiers(accept: impl FnOnce(&str) -> bool) -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
        && std::env::var("CHOSCORUI_BACKEND").is_ok_and(|backend| backend == "x11")
        && std::env::var("XMODIFIERS").is_ok_and(|modifiers| accept(&modifiers))
}

/// True when an XIM server runs for an X11 smoke run: `XMODIFIERS`
/// names it. The CI image has none, so this test runs only by hand, with
/// `xdotool` to type the keys.
fn xim_enabled() -> bool {
    x11_with_modifiers(|modifiers| !modifiers.is_empty() && modifiers != "@im=none")
}

/// Dispatches events until `window` gave an event that `wanted` accepts.
/// Keeps every event of the window in `seen`.
fn wait_for(
    application: &Application,
    window: &Window,
    seen: &mut Vec<WindowEvent>,
    wanted: impl Fn(&WindowEvent) -> bool,
) -> bool {
    pump_until(application, || {
        let start = seen.len();
        seen.extend(window.take_events());
        seen[start..].iter().any(&wanted).then_some(())
    })
    .is_some()
}

/// Types the key of the evdev code `code` through XTEST.
fn press(window: &Window, code: u32) {
    window.simulate_key(code, true).unwrap();
    window.simulate_key(code, false).unwrap();
}

/// Accepts the text that a key types: the keyboard commits it.
fn typed(text: &str) -> impl Fn(&WindowEvent) -> bool + '_ {
    move |event| {
        matches!(
            event,
            WindowEvent::Ime(ImeEvent::CommitText { text: typed, replacement_utf16: None })
                if typed == text
        )
    }
}

/// The evdev codes of the A and B keys.
const KEY_A: u32 = 30;
const KEY_B: u32 = 48;

/// With `XMODIFIERS=@im=none`, Xlib opens its local input method. It
/// takes no preedit callbacks, so the client closes it again and reports
/// no input method. Windows with an active text input then get the focus,
/// lose it, and close as usual. Keys type through xkbcommon, and no text
/// is marked.
#[test]
fn the_local_input_method_of_xlib_leaves_the_keys_to_xkbcommon() {
    if !x11_with_modifiers(|modifiers| modifiers == "@im=none") {
        return;
    }
    assert_eq!(
        choscorui_linux::capabilities().input_methods,
        choscorui_core::Support::Unsupported
    );
    let application = Application::new().unwrap();
    let first = application
        .create_window(WindowOptions::new("XIM none", 240.0, 120.0))
        .unwrap();
    first.show();
    let mut first_seen = Vec::new();
    assert!(wait_for(&application, &first, &mut first_seen, |event| {
        *event == WindowEvent::Focused(true)
    }));
    first.set_text_input_state(state(true, "", 0));
    press(&first, KEY_A);
    assert!(
        wait_for(&application, &first, &mut first_seen, typed("a")),
        "{first_seen:?}"
    );

    // A second window takes the focus from the first.
    let second = application
        .create_window(WindowOptions::new("XIM none 2", 240.0, 120.0))
        .unwrap();
    second.show();
    let mut second_seen = Vec::new();
    assert!(wait_for(&application, &second, &mut second_seen, |event| {
        *event == WindowEvent::Focused(true)
    }));
    assert!(wait_for(&application, &first, &mut first_seen, |event| {
        *event == WindowEvent::Focused(false)
    }));
    second.set_text_input_state(state(true, "", 0));
    press(&second, KEY_B);
    assert!(
        wait_for(&application, &second, &mut second_seen, typed("b")),
        "{second_seen:?}"
    );

    // Windows with an active text input close cleanly.
    second.close();
    first.set_text_input_state(state(false, "", 0));
    first.close();
    assert!(
        pump_until(&application, || (first.is_closed() && second.is_closed())
            .then_some(()))
        .is_some()
    );
    application
        .dispatch_next_event(Duration::from_millis(10))
        .unwrap();
    let marked = |event: &WindowEvent| {
        matches!(
            event,
            WindowEvent::Ime(ImeEvent::SetMarkedText { .. } | ImeEvent::Unmark)
        )
    };
    assert!(!first_seen.iter().any(marked), "{first_seen:?}");
    assert!(!second_seen.iter().any(marked), "{second_seen:?}");
}

/// Types `key` with `xdotool` into the window that has the focus.
fn type_key(key: &str) {
    let status = std::process::Command::new("xdotool")
        .args(["key", key])
        .status()
        .unwrap();
    assert!(status.success());
}

/// With an XIM server whose engine composes `a` (such as ibus-libpinyin),
/// a key gives marked text, and Return commits text.
#[test]
fn an_xim_server_marks_and_commits_text() {
    if !xim_enabled() {
        return;
    }
    assert_eq!(
        choscorui_linux::capabilities().input_methods,
        choscorui_core::Support::Supported
    );
    let application = Application::new().unwrap();
    let window = application
        .create_window(WindowOptions::new("XIM", 240.0, 120.0))
        .unwrap();
    window.show();
    assert!(
        pump_until(&application, || window
            .take_events()
            .contains(&WindowEvent::Focused(true))
            .then_some(()))
        .is_some()
    );
    window.set_text_input_state(state(true, "", 0));
    let settle = Instant::now() + Duration::from_millis(300);
    pump_until(&application, || (Instant::now() >= settle).then_some(()));
    let mut seen = Vec::new();
    type_key("a");
    let ImeEvent::SetMarkedText { text, .. } = next_ime(&application, &window, &mut seen) else {
        panic!("no marked text: {seen:?}");
    };
    assert!(!text.is_empty());
    type_key("Return");
    let committed = pump_until(&application, || {
        seen.extend(
            window
                .take_events()
                .into_iter()
                .filter_map(|event| match event {
                    WindowEvent::Ime(event) => Some(event),
                    _ => None,
                }),
        );
        seen.iter().find_map(|event| match event {
            ImeEvent::CommitText { text, .. } => Some(text.clone()),
            _ => None,
        })
    });
    // What Return commits depends on the engine: the marked text, or the
    // keys that made it.
    assert!(committed.is_some_and(|text| !text.is_empty()), "{seen:?}");
}
