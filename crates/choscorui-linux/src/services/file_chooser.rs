//! File dialogs of the `FileChooser` portal.

use super::portal::{self, RequestError};
use super::urls;
use crate::dbus::{Connection, Value};
use choscorui_core::{FileDialogKind, FileDialogRequest};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

const INTERFACE: &str = "org.freedesktop.portal.FileChooser";

/// What the user did in a file dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDialogResponse {
    Selected(Vec<PathBuf>),
    Cancelled,
}

impl FileDialogResponse {
    /// The chosen paths, empty after a cancel.
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Self::Selected(paths) => paths,
            Self::Cancelled => &[],
        }
    }
}

/// Why the portal showed no dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileChooserError {
    /// No portal implements `FileChooser`. Show the drawn dialog.
    NoPortal,
    /// The portal failed.
    Failed(String),
}

impl std::fmt::Display for FileChooserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPortal => f.write_str("no portal shows file dialogs"),
            Self::Failed(reason) => write!(f, "the file dialog portal failed: {reason}"),
        }
    }
}

impl std::error::Error for FileChooserError {}

/// A file dialog for the portal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChooserRequest {
    pub kind: FileDialogKind,
    pub title: Option<String>,
    /// The label of the button that accepts, such as "Export".
    pub accept_label: Option<String>,
    /// The folder that a save dialog opens in.
    pub current_folder: Option<PathBuf>,
    /// The file name that a save dialog starts with.
    pub current_name: Option<String>,
    /// File name extensions without the dot. Empty allows every file.
    pub extensions: Vec<String>,
    /// The parent window: `wayland:<handle>`, `x11:<xid>`, or empty.
    pub parent_window: String,
}

impl FileChooserRequest {
    /// A dialog of `kind` that allows every file, without a parent.
    pub fn new(kind: FileDialogKind) -> Self {
        Self {
            kind,
            title: None,
            accept_label: None,
            current_folder: None,
            current_name: None,
            extensions: Vec::new(),
            parent_window: String::new(),
        }
    }

    /// The portal form of a core request, for the window `parent_window`.
    pub fn from_request(request: &FileDialogRequest, parent_window: &str) -> Self {
        Self {
            kind: request.kind,
            title: request.title.clone(),
            accept_label: None,
            current_folder: None,
            current_name: request.suggested_name.clone(),
            extensions: request.allowed_types.clone(),
            parent_window: parent_window.to_owned(),
        }
    }

    /// The `filters` option: one filter of every extension, with the
    /// lower and upper case forms of each pattern.
    fn filter(&self) -> Option<Value> {
        if self.extensions.is_empty() {
            return None;
        }
        let mut patterns = Vec::new();
        for extension in &self.extensions {
            for form in [extension.to_lowercase(), extension.to_uppercase()] {
                let pattern =
                    Value::Struct(vec![Value::from(0u32), Value::from(format!("*.{form}"))]);
                if !patterns.contains(&pattern) {
                    patterns.push(pattern);
                }
            }
        }
        let name = self
            .extensions
            .iter()
            .map(|extension| format!("*.{extension}"))
            .collect::<Vec<_>>()
            .join(", ");
        Some(Value::Struct(vec![
            Value::from(name),
            Value::array("(us)", patterns),
        ]))
    }

    /// The method and the options of the request.
    fn options(&self) -> (&'static str, Value) {
        let mut options: Vec<(&str, Value)> = vec![("modal", Value::from(true))];
        if let Some(label) = &self.accept_label {
            options.push(("accept_label", Value::from(label.as_str())));
        }
        if let Some(filter) = self.filter() {
            options.push(("filters", Value::array("(sa(us))", vec![filter.clone()])));
            options.push(("current_filter", filter));
        }
        let method = match self.kind {
            FileDialogKind::Save => {
                if let Some(name) = &self.current_name {
                    options.push(("current_name", Value::from(name.as_str())));
                }
                if let Some(folder) = &self.current_folder {
                    // A byte string that ends with NUL.
                    let mut bytes: Vec<Value> = folder
                        .as_os_str()
                        .as_bytes()
                        .iter()
                        .map(|&byte| Value::Byte(byte))
                        .collect();
                    bytes.push(Value::Byte(0));
                    options.push(("current_folder", Value::array("y", bytes)));
                }
                "SaveFile"
            }
            FileDialogKind::OpenMany => {
                options.push(("multiple", Value::from(true)));
                "OpenFile"
            }
            FileDialogKind::Directory => {
                options.push(("directory", Value::from(true)));
                "OpenFile"
            }
            FileDialogKind::Open => "OpenFile",
        };
        (method, Value::vardict(options))
    }
}

/// Shows the dialog of `request` through the portal and waits for the
/// answer in a nested loop.
pub fn choose_files(
    connection: Option<&Connection>,
    request: &FileChooserRequest,
) -> Result<FileDialogResponse, FileChooserError> {
    let connection = connection.ok_or(FileChooserError::NoPortal)?;
    match connection.name_available(portal::DESKTOP) {
        Ok(true) => {}
        _ => return Err(FileChooserError::NoPortal),
    }
    let (method, options) = request.options();
    let title = request.title.clone().unwrap_or_default();
    let args = vec![
        Value::from(request.parent_window.as_str()),
        Value::from(title),
        options,
    ];
    let response =
        portal::request(connection, INTERFACE, method, args).map_err(|error| match error {
            RequestError::NoPortal => FileChooserError::NoPortal,
            RequestError::Failed(reason) => FileChooserError::Failed(reason),
        })?;
    match response.code {
        0 => {
            let uris = response
                .results
                .get("uris")
                .and_then(Value::as_strings)
                .unwrap_or_default();
            let paths = uris
                .iter()
                .map(|uri| {
                    urls::file_uri_path(uri).ok_or_else(|| {
                        FileChooserError::Failed(format!("{uri} is not a local file"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FileDialogResponse::Selected(paths))
        }
        1 => Ok(FileDialogResponse::Cancelled),
        code => Err(FileChooserError::Failed(format!(
            "the dialog ended with code {code}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_dialog_sends_its_name_folder_and_filters() {
        let request = FileChooserRequest {
            current_name: Some("report.txt".to_owned()),
            current_folder: Some(PathBuf::from("/tmp")),
            extensions: vec!["txt".to_owned(), "MD".to_owned()],
            ..FileChooserRequest::new(FileDialogKind::Save)
        };
        let (method, options) = request.options();
        assert_eq!(method, "SaveFile");
        assert_eq!(
            options.get("current_name").and_then(Value::as_str),
            Some("report.txt")
        );
        let folder: Vec<u8> = options
            .get("current_folder")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|byte| u8::try_from(byte.as_u32().unwrap()).unwrap())
            .collect();
        assert_eq!(folder, b"/tmp\0");
        let filter = options
            .get("current_filter")
            .and_then(Value::as_struct)
            .unwrap();
        assert_eq!(filter[0].as_str(), Some("*.txt, *.MD"));
        let patterns: Vec<&str> = filter[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|pattern| pattern.as_struct().unwrap()[1].as_str().unwrap())
            .collect();
        assert_eq!(patterns, ["*.txt", "*.TXT", "*.md", "*.MD"]);
        assert!(options.check().is_ok());
    }

    #[test]
    fn open_dialogs_ask_for_many_files_or_a_directory() {
        let many = FileChooserRequest::new(FileDialogKind::OpenMany);
        let (method, options) = many.options();
        assert_eq!(method, "OpenFile");
        assert_eq!(options.get("multiple").and_then(Value::as_bool), Some(true));
        assert!(options.get("filters").is_none());
        let directory = FileChooserRequest::new(FileDialogKind::Directory);
        let (_, options) = directory.options();
        assert_eq!(
            options.get("directory").and_then(Value::as_bool),
            Some(true)
        );
    }
}
