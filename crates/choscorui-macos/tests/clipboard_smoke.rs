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
}

#[cfg(not(target_os = "macos"))]
fn main() {}
