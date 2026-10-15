//! The clipboard on a real display server, with a second process that
//! reads what the first one wrote. The display tests run only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`.
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
