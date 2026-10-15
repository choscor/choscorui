//! The drawn dialogs of Linux on a real display server: the alert of
//! `DrawnDialogs::show_alert`, which takes keys from the virtual keyboard
//! of the session, and the drawn file dialog of the services. The tests
//! run only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, one at a time
//! (`--test-threads=1`), and only on Wayland: the virtual keyboard is a
//! Wayland device.
#![cfg(target_os = "linux")]

use choscorui::compose::view::ViewHost;
use choscorui::controls::{ControlKind, EditCommand, ShortcutModifiers};
use choscorui::core::{Key, KeyEvent, SemanticRole};
use choscorui::platform::{
    AlertButton, AlertResponse, AlertSpec, DrawnDialogs, ModalTurn, NativeAccess,
};
use choscorui::prelude::*;
use std::path::PathBuf;
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

// Linux input event codes of the keys that the tests press.
const KEY_ESC: u32 = 1;
const KEY_TAB: u32 = 15;
const KEY_ENTER: u32 = 28;
const KEY_SPACE: u32 = 57;

/// An application with one painted window.
fn parent() -> AppContext {
    let mut cx = App::new()
        .window(WindowOptions::new("Dialog parent", 320.0, 200.0), || {
            Text::new("parent")
        })
        .start()
        .expect("the app must start");
    let id = cx.windows()[0];
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx
        .native_window(id)
        .is_some_and(|window| window.paint_count() == 0)
    {
        assert!(Instant::now() < deadline, "the parent did not paint");
        cx.pump();
    }
    cx
}

/// Shows `buttons` in an alert and presses `keys` on the virtual
/// keyboard once the alert window has the focus.
fn answer(buttons: &[&str], keys: &[u32]) -> AlertResponse {
    let mut cx = parent();
    let keys = keys.to_vec();
    let mut sent = false;
    cx.set_modal_driver(Some(Box::new(move |turn: ModalTurn<'_>| {
        if turn.focused && turn.window.is_visible() && !sent {
            sent = true;
            for key in &keys {
                virtual_input(&format!("key {key} 1"));
                virtual_input(&format!("key {key} 0"));
            }
        }
    })));
    let spec = AlertSpec::new(
        "Save the changes?",
        buttons.iter().map(|title| AlertButton::new(*title)),
    )
    .unwrap()
    .with_detail("The document has edits.")
    .unwrap();
    cx.show_alert(&spec).unwrap()
}

#[test]
fn enter_runs_the_first_button_of_an_alert() {
    if !enabled() {
        return;
    }
    assert_eq!(
        answer(&["Save", "Cancel", "Don't Save"], &[KEY_ENTER]),
        AlertResponse::Button(0)
    );
}

#[test]
fn escape_runs_the_cancel_button_of_an_alert() {
    if !enabled() {
        return;
    }
    assert_eq!(
        answer(&["Save", "Cancel", "Don't Save"], &[KEY_ESC]),
        AlertResponse::Button(1)
    );
}

#[test]
fn escape_cancels_an_alert_without_a_cancel_button() {
    if !enabled() {
        return;
    }
    assert_eq!(
        answer(&["OK", "Retry"], &[KEY_ESC]),
        AlertResponse::Cancelled
    );
}

#[test]
fn space_runs_the_focused_button_of_an_alert() {
    if !enabled() {
        return;
    }
    // The focus starts on the first button in display order, Cancel;
    // Tab moves it to Save.
    assert_eq!(
        answer(&["Save", "Cancel"], &[KEY_SPACE]),
        AlertResponse::Button(1)
    );
    assert_eq!(
        answer(&["Save", "Cancel"], &[KEY_TAB, KEY_SPACE]),
        AlertResponse::Button(0)
    );
}

/// Presses `key` in `host`.
fn press(host: &mut ViewHost, key: Key) {
    let none = ShortcutModifiers::default();
    host.key(KeyEvent::Down { key, repeat: false }, none);
    host.key(KeyEvent::Up { key }, none);
    host.layout();
}

/// True when the drawn file dialog of `host` lists a row named `name`.
fn lists(host: &mut ViewHost, name: &str) -> bool {
    host.semantics().0.iter().any(|entry| {
        entry.part.is_some() && entry.node.role == SemanticRole::ListItem && entry.node.name == name
    })
}

#[test]
fn the_drawn_file_dialog_returns_the_chosen_file() {
    if !enabled() {
        return;
    }
    let directory = std::env::temp_dir().join(format!("choscorui-dialog-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for name in ["alpha.txt", "beta.txt"] {
        std::fs::write(directory.join(name), name).unwrap();
    }
    let mut cx = parent();
    let typed = directory.display().to_string();
    let mut step = 0;
    cx.set_modal_driver(Some(Box::new(move |turn: ModalTurn<'_>| {
        let host = turn.host;
        match step {
            // Type the directory into the location field.
            0 => {
                let dialog = host.semantics().0.into_iter().find(|entry| {
                    entry.part.is_none() && host.control_kind(entry.id) == Some(ControlKind::Dialog)
                });
                if let Some(dialog) = dialog {
                    host.focus(Some(dialog.id));
                    host.edit(EditCommand::SelectAll);
                    host.text(&typed);
                    press(host, Key::Enter);
                    step = 1;
                }
            }
            // Once the list shows the file, type its name and accept it.
            1 if lists(host, "beta.txt") => {
                press(host, Key::Tab);
                host.text("beta");
                press(host, Key::Enter);
                step = 2;
            }
            _ => {}
        }
    })));
    let chosen = cx
        .services()
        .choose_files(&FileDialogRequest::new(FileDialogKind::Open));
    let _ = std::fs::remove_dir_all(&directory);
    assert_eq!(chosen, Ok(vec![PathBuf::from(&directory).join("beta.txt")]));
}
