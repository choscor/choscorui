//! The menu queue and the popups of the native backend. The tests open
//! a display connection, so they run only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`,
//! one at a time (`--test-threads=1`): they share the focus and the
//! pointer.
#![cfg(target_os = "linux")]

use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuItemState, MenuShortcut, MenuShortcutModifiers,
    MenuSpec, Point, PointerEvent, Rect, Size, WindowOptions,
};
use choscorui_linux::{Application, BackendKind, Error, Window, WindowEvent, capabilities};
use std::process::Command;
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

#[test]
fn the_main_menu_queues_enabled_commands() {
    if !enabled() {
        return;
    }
    let app = Application::new().expect("the display must connect");
    let command = MenuCommandId(41);
    let ctrl = MenuShortcutModifiers {
        command: true,
        ..MenuShortcutModifiers::default()
    };
    let model = MenuBarModel::new(vec![MenuSpec::new(
        "File",
        vec![MenuEntry::command(
            command,
            MenuItemState {
                title: "Open…".into(),
                enabled: true,
                checked: false,
                shortcut: Some(MenuShortcut {
                    key: "o".into(),
                    modifiers: ctrl,
                }),
            },
        )],
    )])
    .unwrap();
    let menu = app
        .install_main_menu(model)
        .expect("main menu must install");

    assert!(menu.perform_action(command).unwrap());
    assert!(menu.perform_key_equivalent("o", ctrl).unwrap());
    assert_eq!(menu.take_activations(), vec![command, command]);
    assert!(menu.take_activations().is_empty());

    menu.set_state(
        command,
        MenuItemState {
            title: "Open Recent".into(),
            enabled: false,
            checked: true,
            shortcut: None,
        },
    )
    .expect("the state update must succeed");
    let model = menu.model();
    let state = model.state(command).unwrap();
    assert_eq!(state.title, "Open Recent");
    assert!(!state.enabled);
    assert!(state.checked);
    assert!(!menu.perform_action(command).unwrap());
    assert!(!menu.perform_key_equivalent("o", ctrl).unwrap());
    assert!(menu.take_activations().is_empty());
    assert_eq!(menu.callback_panic_count(), 0);
}

#[test]
fn a_popup_opens_and_a_click_outside_closes_it() {
    if !enabled() {
        return;
    }
    let application = Application::new().unwrap();
    let window = application
        .create_window(WindowOptions::new("Popup parent", 300.0, 200.0))
        .unwrap();
    window.show();
    wait_until(&application, || window.is_visible());
    let anchor = Rect::new(Point::new(20.0, 30.0), Size::new(1.0, 1.0));
    assert!(capabilities().context_menus.is_supported());
    if application.backend() == BackendKind::X11 {
        x11_popup(&application, &window, anchor);
        return;
    }
    place(&application, "Popup parent", 100, 100);
    pump(&application, Duration::from_millis(300));
    // A press in the window gives the popup the serial of its grab.
    virtual_input("move 150 150");
    virtual_input("button 273 1");
    pump(&application, Duration::from_millis(200));
    let popup = window
        .open_popup(anchor, Size::new(120.0, 80.0))
        .expect("the popup opens");
    popup.show();
    virtual_input("button 273 0");
    wait_until(&application, || popup.is_visible());
    assert_eq!(
        application.window_count(),
        1,
        "a popup is not a window of its own"
    );
    assert_eq!(popup.content_size(), Size::new(120.0, 80.0));
    assert!(matches!(
        window.open_popup(anchor, Size::new(0.0, 10.0)),
        Err(Error::InvalidSize)
    ));
    // A click outside the popup ends its grab: the popup gets Closing.
    virtual_input("move 1000 700");
    virtual_input("button 272 1");
    virtual_input("button 272 0");
    let mut events = Vec::new();
    wait_until(&application, || {
        events.extend(popup.take_events());
        events
            .iter()
            .any(|event| matches!(event, WindowEvent::Closing))
    });
    popup.close();
    window.close();
    pump(&application, Duration::from_millis(100));
}

/// The popup case on X11, with XTEST for the pointer. Xvfb runs no
/// window manager, and its screen is 1280x800 points.
fn x11_popup(application: &Application, window: &Window, anchor: Rect) {
    let parent = window.geometry().frame.origin;
    let popup = window
        .open_popup(anchor, Size::new(120.0, 80.0))
        .expect("the popup opens");
    popup.show();
    wait_until(application, || popup.is_visible());
    assert_eq!(
        application.window_count(),
        1,
        "a popup is not a window of its own"
    );
    assert_eq!(popup.content_size(), Size::new(120.0, 80.0));
    // The popup opens below the bottom-left corner of the anchor.
    assert_eq!(
        popup.geometry().frame.origin,
        Point::new(parent.x + 20.0, parent.y + 31.0)
    );
    assert!(matches!(
        window.open_popup(anchor, Size::new(0.0, 10.0)),
        Err(Error::InvalidSize)
    ));
    // The grab sends a press inside the popup to the popup.
    popup.take_events();
    popup
        .simulate_pointer(Point::new(30.0, 20.0), Some(true))
        .unwrap();
    popup
        .simulate_pointer(Point::new(30.0, 20.0), Some(false))
        .unwrap();
    let mut events = Vec::new();
    wait_until(application, || {
        events.extend(popup.take_events());
        events.iter().any(|event| {
            matches!(
                event,
                WindowEvent::Pointer {
                    event: PointerEvent::Up { position, .. },
                    ..
                } if *position == Point::new(30.0, 20.0)
            )
        })
    });
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, WindowEvent::Closing))
    );
    // A click outside the popup ends its grab: the popup gets Closing,
    // and the parent gets no press.
    window.take_events();
    window
        .simulate_pointer(Point::new(1000.0, 700.0), Some(true))
        .unwrap();
    window
        .simulate_pointer(Point::new(1000.0, 700.0), Some(false))
        .unwrap();
    let mut events = Vec::new();
    wait_until(application, || {
        events.extend(popup.take_events());
        events
            .iter()
            .any(|event| matches!(event, WindowEvent::Closing))
    });
    assert!(!window.take_events().iter().any(|event| matches!(
        event,
        WindowEvent::Pointer {
            event: PointerEvent::Down { .. },
            ..
        }
    )));
    popup.close();
    // Near the bottom edge of the screen, the popup flips above the
    // anchor.
    let low = Rect::new(Point::new(20.0, 780.0 - parent.y), Size::new(1.0, 1.0));
    let flipped = window.open_popup(low, Size::new(120.0, 80.0)).unwrap();
    flipped.show();
    wait_until(application, || flipped.is_visible());
    assert_eq!(
        flipped.geometry().frame.origin,
        Point::new(parent.x + 20.0, 700.0)
    );
    flipped.close();
    window.close();
    pump(application, Duration::from_millis(100));
}

/// Dispatches events until `done` is true, for up to five seconds.
fn wait_until(application: &Application, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
}

fn pump(application: &Application, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
    }
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
