//! The clipboard on a real display server, with a second process that
//! reads what the first one wrote, and a drag between two windows. The
//! display tests run only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, one at a time
//! (`--test-threads=1`): they share the focus and the pointer.
#![cfg(target_os = "linux")]

use choscorui_core::{DragFormat, DragItem, Image, WindowOptions};
use choscorui_linux::{Application, Error, SystemClipboard, Window, encode_png};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD: &str = "CHOSCORUI_CLIPBOARD_CHILD";

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

fn png() -> Vec<u8> {
    encode_png(&Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap()).unwrap()
}

fn files() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/tmp/choscorui a.txt"),
        PathBuf::from("/home/Tiếng Việt.pdf"),
    ]
}

fn items() -> Vec<DragItem> {
    vec![
        DragItem::Text("Xin chào 👩‍💻".into()),
        DragItem::Url("https://example.com/a?b=c".into()),
        DragItem::Image(png()),
        DragItem::Custom {
            format: "row-keys".into(),
            bytes: vec![1, 2, 3],
        },
        DragItem::Files(files()),
    ]
}

fn wanted() -> Vec<DragFormat> {
    vec![
        DragFormat::Files,
        DragFormat::Text,
        DragFormat::Image,
        DragFormat::Url,
        DragFormat::Custom("row-keys".into()),
    ]
}

/// Opens a window and waits until the compositor maps it, so the
/// application has the keyboard focus that the selection needs.
fn focused_window(application: &Application) -> Window {
    let window = application
        .create_window(WindowOptions::new("Clipboard", 200.0, 120.0))
        .unwrap();
    window.show();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !window.is_visible() {
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
    // The focus and the selection follow the configure.
    for _ in 0..10 {
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
    }
    window
}

#[test]
fn a_second_process_reads_the_standard_formats_but_not_private_data() {
    if !enabled() || std::env::var_os(CHILD).is_some() {
        return;
    }
    let application = Application::new().unwrap();
    let _window = focused_window(&application);
    let clipboard = application.clipboard();
    clipboard.write(&items()).unwrap();
    // The application reads its own selection, private data included.
    assert_eq!(
        clipboard.read(&wanted()).unwrap(),
        vec![
            DragItem::Files(files()),
            DragItem::Text("Xin chào 👩‍💻".into()),
            DragItem::Image(png()),
            DragItem::Url("https://example.com/a?b=c".into()),
            DragItem::Custom {
                format: "row-keys".into(),
                bytes: vec![1, 2, 3],
            },
        ]
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_reads_the_selection", "--nocapture"])
        .env(CHILD, "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // The application serves the data while the child reads it.
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "the child did not finish");
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
    };
    let lines: Vec<String> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| line.strip_prefix("CLIPBOARD ").map(str::to_owned))
        .collect();
    assert!(status.success(), "{lines:?}");
    let expected_formats = vec![
        DragFormat::Text,
        DragFormat::Files,
        DragFormat::Image,
        DragFormat::Url,
    ];
    let expected_items = vec![
        DragItem::Files(files()),
        DragItem::Text("Xin chào 👩‍💻".into()),
        DragItem::Image(png()),
        DragItem::Url("https://example.com/a?b=c".into()),
    ];
    assert_eq!(
        lines,
        vec![
            format!("{expected_formats:?}"),
            format!("{expected_items:?}"),
        ]
    );
}

/// The second process of the test above.
#[test]
fn child_reads_the_selection() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let application = Application::new().unwrap();
    let _window = focused_window(&application);
    let clipboard = application.clipboard();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut formats = clipboard.formats().unwrap();
    while formats.is_empty() && Instant::now() < deadline {
        application
            .dispatch_next_event(Duration::from_millis(20))
            .unwrap();
        formats = clipboard.formats().unwrap();
    }
    println!("CLIPBOARD {formats:?}");
    println!("CLIPBOARD {:?}", clipboard.read(&wanted()).unwrap());
}

#[test]
fn a_temporary_board_keeps_typed_items_in_the_process() {
    let clipboard = SystemClipboard::temporary();
    assert_eq!(clipboard.read_text().unwrap(), None);
    clipboard.write_text("Xin chào 👩‍💻").unwrap();
    assert_eq!(
        clipboard.read_text().unwrap().as_deref(),
        Some("Xin chào 👩‍💻")
    );
    clipboard.write_text("").unwrap();
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some(""));
    assert!(matches!(
        clipboard.write_text("a\0b"),
        Err(Error::InvalidClipboardText)
    ));
    clipboard.write(&items()).unwrap();
    assert_eq!(
        clipboard.formats().unwrap(),
        vec![
            DragFormat::Text,
            DragFormat::Files,
            DragFormat::Image,
            DragFormat::Url,
            DragFormat::Custom("row-keys".into()),
        ]
    );
    // Text-only writes replace the typed contents.
    clipboard.write_text("plain").unwrap();
    assert_eq!(clipboard.formats().unwrap(), vec![DragFormat::Text]);
    assert!(matches!(
        clipboard.write(&[DragItem::Image(vec![1, 2, 3])]),
        Err(Error::UnsupportedFormat)
    ));
    assert!(matches!(
        clipboard.write(&[DragItem::Custom {
            format: "bad/name".into(),
            bytes: vec![],
        }]),
        Err(Error::UnsupportedFormat)
    ));
    assert!(matches!(
        clipboard.write(&[DragItem::Files(vec![PathBuf::from("relative.txt")])]),
        Err(Error::InvalidClipboardText)
    ));
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

/// Dispatches events for `duration`, and collects the events of `windows`.
fn pump_for(
    application: &Application,
    windows: &[&Window],
    events: &mut [Vec<choscorui_linux::WindowEvent>],
    duration: Duration,
) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
        for (window, collected) in windows.iter().zip(events.iter_mut()) {
            collected.extend(window.take_events());
        }
    }
}

#[test]
fn a_drag_between_two_windows_drops_the_items_and_ends() {
    use choscorui_core::{DragEvent, DragOperation, DragOperations, PointerEvent};
    use choscorui_linux::WindowEvent;
    if !enabled() || std::env::var("CHOSCORUI_BACKEND").is_ok_and(|backend| backend == "x11") {
        return;
    }
    let application = Application::new().unwrap();
    let source = application
        .create_window(WindowOptions::new("Drag source", 200.0, 150.0))
        .unwrap();
    let target = application
        .create_window(WindowOptions::new("Drop target", 200.0, 150.0))
        .unwrap();
    target
        .accept_drops(
            &[DragFormat::Text, DragFormat::Custom("row-keys".into())],
            DragOperations::ALL,
        )
        .unwrap();
    assert!(matches!(
        source.begin_drag(&items(), DragOperations::ALL, None),
        Err(Error::NoDragEvent)
    ));
    source.show();
    target.show();
    let windows = [&source, &target];
    let mut events = [Vec::new(), Vec::new()];
    pump_for(
        &application,
        &windows,
        &mut events,
        Duration::from_millis(300),
    );
    place(&application, "Drag source", 100, 100);
    place(&application, "Drop target", 600, 100);
    pump_for(
        &application,
        &windows,
        &mut events,
        Duration::from_millis(200),
    );
    virtual_input("move 150 150");
    virtual_input("button 272 1");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !events[0].iter().any(|event| {
        matches!(
            event,
            WindowEvent::Pointer {
                event: PointerEvent::Down { .. },
                ..
            }
        )
    }) {
        assert!(Instant::now() < deadline, "no press: {:?}", events[0]);
        pump_for(
            &application,
            &windows,
            &mut events,
            Duration::from_millis(20),
        );
    }
    let custom = DragItem::Custom {
        format: "row-keys".into(),
        bytes: vec![1, 2, 3],
    };
    source
        .begin_drag(
            &[DragItem::Text("dragged".into()), custom.clone()],
            DragOperations::ALL,
            Some(&Image::from_rgba8(2, 2, vec![0, 0, 255, 255].repeat(4)).unwrap()),
        )
        .unwrap();
    for x in [200, 400, 620, 650] {
        virtual_input(&format!("move {x} 160"));
        pump_for(
            &application,
            &windows,
            &mut events,
            Duration::from_millis(100),
        );
    }
    virtual_input("button 272 0");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !events[0]
        .iter()
        .any(|event| matches!(event, WindowEvent::DragEnded(_)))
    {
        assert!(Instant::now() < deadline, "no end: {:?}", events);
        pump_for(
            &application,
            &windows,
            &mut events,
            Duration::from_millis(20),
        );
    }
    let drags: Vec<&DragEvent> = events[1]
        .iter()
        .filter_map(|event| match event {
            WindowEvent::Drag(drag) => Some(drag),
            _ => None,
        })
        .collect();
    assert!(
        matches!(drags.first(), Some(DragEvent::Enter { .. })),
        "{drags:?}"
    );
    let Some(DragEvent::Drop {
        items, operation, ..
    }) = drags.last()
    else {
        panic!("no drop: {drags:?}");
    };
    assert_eq!(items, &vec![DragItem::Text("dragged".into()), custom]);
    // Within one application, a drag without modifiers moves.
    assert_eq!(*operation, DragOperation::Move);
    assert!(
        events[0]
            .iter()
            .any(|event| matches!(event, WindowEvent::DragEnded(DragOperation::Move)))
    );
}
