use choscorui_macos::{
    AlertButton, AlertResponse, AlertSpec, AlertStyle, DialogError, FileDialogMode,
    FileDialogResponse, FileDialogSpec,
};
use std::path::PathBuf;

#[test]
fn alert_requires_a_message_and_at_least_one_valid_button() {
    assert_eq!(
        AlertSpec::new("", [AlertButton::new("OK")]),
        Err(DialogError::EmptyMessage)
    );
    assert_eq!(AlertSpec::new("Message", []), Err(DialogError::NoButtons));
    assert_eq!(
        AlertSpec::new("Message", [AlertButton::new("bad\0button")]),
        Err(DialogError::InvalidText)
    );
}

#[test]
fn alert_preserves_platform_neutral_order_and_metadata() {
    let alert = AlertSpec::new(
        "Discard changes?",
        [AlertButton::new("Discard"), AlertButton::new("Cancel")],
    )
    .unwrap()
    .with_detail("The document has unsaved edits.")
    .unwrap()
    .with_style(AlertStyle::Warning);

    assert_eq!(alert.message(), "Discard changes?");
    assert_eq!(alert.detail(), Some("The document has unsaved edits."));
    assert_eq!(alert.buttons()[1].title(), "Cancel");
    assert_eq!(alert.style(), AlertStyle::Warning);
    assert_eq!(
        AlertResponse::from_native_code(1000, 2),
        Some(AlertResponse::Button(0))
    );
    assert_eq!(
        AlertResponse::from_native_code(1001, 2),
        Some(AlertResponse::Button(1))
    );
    assert_eq!(
        AlertResponse::from_native_code(-1000, 2),
        Some(AlertResponse::Cancelled)
    );
    assert_eq!(AlertResponse::from_native_code(1002, 2), None);
}

#[test]
fn open_multiple_and_directory_modes_expose_the_right_contract() {
    let open = FileDialogSpec::open_files()
        .with_title("Choose documents")
        .unwrap()
        .with_allowed_extensions(["rs", "toml"])
        .unwrap();
    assert_eq!(open.mode(), FileDialogMode::OpenFiles);
    assert_eq!(open.allowed_extensions(), &["rs", "toml"]);

    let directory = FileDialogSpec::select_directory();
    assert_eq!(directory.mode(), FileDialogMode::SelectDirectory);
    assert!(directory.allowed_extensions().is_empty());
}

#[test]
fn file_dialog_rejects_invalid_text_and_extensions() {
    assert_eq!(
        FileDialogSpec::open_file().with_title("bad\0title"),
        Err(DialogError::InvalidText)
    );
    assert_eq!(
        FileDialogSpec::open_file().with_allowed_extensions([".rs"]),
        Err(DialogError::InvalidExtension)
    );
    assert_eq!(
        FileDialogSpec::open_file().with_allowed_extensions(["a/b"]),
        Err(DialogError::InvalidExtension)
    );
}

#[test]
fn file_response_distinguishes_cancel_from_selection() {
    assert!(FileDialogResponse::Cancelled.paths().is_empty());
    let paths = vec![PathBuf::from("/tmp/a.rs"), PathBuf::from("/tmp/b.rs")];
    let response = FileDialogResponse::Selected(paths.clone());
    assert_eq!(response.paths(), paths.as_slice());
}
