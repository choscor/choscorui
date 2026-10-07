#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_CLIPBOARD").is_none() {
        return;
    }
    assert!(
        std::thread::spawn(|| matches!(
            choscorui_macos::Clipboard::general(),
            Err(choscorui_macos::Error::WrongThread)
        ))
        .join()
        .unwrap()
    );
    let _app = choscorui_macos::Application::new().unwrap();
    let clipboard = choscorui_macos::Clipboard::temporary().unwrap();
    assert_eq!(clipboard.read_text().unwrap(), None);
    clipboard.write_text("Xin chào 👩‍💻").unwrap();
    assert_eq!(
        clipboard.read_text().unwrap().as_deref(),
        Some("Xin chào 👩‍💻")
    );
    clipboard.write_text("").unwrap();
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some(""));
    drop(clipboard);
    let clipboard = choscorui_macos::Clipboard::temporary().unwrap();
    assert!(clipboard.write_text("a\0b").is_err());
    assert_eq!(clipboard.read_text().unwrap(), None);
    typed_payloads();
}

#[cfg(target_os = "macos")]
fn typed_payloads() {
    use choscorui_core::{DragFormat, DragItem, Image};
    use choscorui_macos::{Clipboard, Error, encode_png};
    use std::path::PathBuf;

    let clipboard = Clipboard::temporary().unwrap();
    let png = encode_png(&Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap())
        .unwrap();
    let files = vec![
        PathBuf::from("/tmp/choscorui a.txt"),
        PathBuf::from("/Users/Shared/Tiếng Việt.pdf"),
    ];
    let items = vec![
        DragItem::Text("Xin chào".into()),
        DragItem::Url("https://example.com/a?b=c".into()),
        DragItem::Image(png.clone()),
        DragItem::Custom {
            format: "row-keys".into(),
            bytes: vec![1, 2, 3],
        },
        DragItem::Files(files.clone()),
    ];
    clipboard.write(&items).unwrap();
    let formats = clipboard.formats().unwrap();
    for format in [
        DragFormat::Text,
        DragFormat::Url,
        DragFormat::Image,
        DragFormat::Custom("row-keys".into()),
        DragFormat::Files,
    ] {
        assert!(
            formats.contains(&format),
            "{format:?} missing from {formats:?}"
        );
    }
    let read = clipboard
        .read(&[
            DragFormat::Files,
            DragFormat::Text,
            DragFormat::Image,
            DragFormat::Url,
            DragFormat::Custom("row-keys".into()),
            DragFormat::Custom("absent".into()),
        ])
        .unwrap();
    assert_eq!(
        read,
        vec![
            DragItem::Files(files),
            DragItem::Text("Xin chào".into()),
            DragItem::Image(png),
            DragItem::Url("https://example.com/a?b=c".into()),
            DragItem::Custom {
                format: "row-keys".into(),
                bytes: vec![1, 2, 3],
            },
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

#[cfg(not(target_os = "macos"))]
fn main() {}
