#![deny(unsafe_code)]

use super::{Application, Pool};
use crate::ffi::appkit::{
    ADD_BUTTON_WITH_TITLE, ALERT_RUN_MODAL, NSAlert, NSSavePanel, OPEN_PANEL, PANEL_RUN_MODAL,
    PANEL_URL, PANEL_URLS, SAVE_PANEL, SET_ALERT_STYLE, SET_ALLOWED_CONTENT_TYPES,
    SET_ALLOWS_MULTIPLE_SELECTION, SET_CAN_CHOOSE_DIRECTORIES, SET_CAN_CHOOSE_FILES,
    SET_DIRECTORY_URL, SET_INFORMATIVE_TEXT, SET_MESSAGE_TEXT, SET_NAME_FIELD_STRING_VALUE,
    SET_PROMPT, SET_SHOWS_HIDDEN_FILES, SET_TITLE, TYPE_WITH_FILENAME_EXTENSION,
};
use crate::ffi::foundation::{
    self, FILE_URL_WITH_PATH, NSObject, NSString, NsUrl, PATH, TextError, array_items, ns_array,
    string,
};
use crate::ffi::retained::{Retained, alloc_init};
use crate::ffi::{self, Obj};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogError {
    EmptyMessage,
    NoButtons,
    InvalidText,
    InvalidExtension,
    WrongThread,
    AppKitUnavailable,
    InvalidResult,
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMessage => f.write_str("the alert message is empty"),
            Self::NoButtons => f.write_str("the alert has no buttons"),
            Self::InvalidText => f.write_str("text contains an interior NUL character"),
            Self::InvalidExtension => {
                f.write_str("a file extension is empty or contains a dot or slash")
            }
            Self::WrongThread => f.write_str("dialogs must run on the main thread"),
            Self::AppKitUnavailable => f.write_str("AppKit is unavailable"),
            Self::InvalidResult => f.write_str("AppKit returned an unexpected dialog result"),
        }
    }
}

impl std::error::Error for DialogError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertButton {
    title: String,
}

impl AlertButton {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlertStyle {
    Warning,
    #[default]
    Informational,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertSpec {
    message: String,
    detail: Option<String>,
    buttons: Vec<AlertButton>,
    style: AlertStyle,
}

impl AlertSpec {
    pub fn new(
        message: impl Into<String>,
        buttons: impl IntoIterator<Item = AlertButton>,
    ) -> Result<Self, DialogError> {
        let message = message.into();
        if message.is_empty() {
            return Err(DialogError::EmptyMessage);
        }
        validate_text(&message)?;
        let buttons: Vec<_> = buttons.into_iter().collect();
        if buttons.is_empty() {
            return Err(DialogError::NoButtons);
        }
        for button in &buttons {
            if button.title.is_empty() {
                return Err(DialogError::InvalidText);
            }
            validate_text(&button.title)?;
        }
        Ok(Self {
            message,
            detail: None,
            buttons,
            style: AlertStyle::default(),
        })
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Result<Self, DialogError> {
        let detail = detail.into();
        validate_text(&detail)?;
        self.detail = Some(detail);
        Ok(self)
    }

    pub fn with_style(mut self, style: AlertStyle) -> Self {
        self.style = style;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn buttons(&self) -> &[AlertButton] {
        &self.buttons
    }
    pub fn style(&self) -> AlertStyle {
        self.style
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertResponse {
    Button(usize),
    Cancelled,
}

impl AlertResponse {
    pub fn from_native_code(code: isize, button_count: usize) -> Option<Self> {
        if code < 1000 {
            return Some(Self::Cancelled);
        }
        let index = usize::try_from(code - 1000).ok()?;
        (index < button_count).then_some(Self::Button(index))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileDialogMode {
    OpenFile,
    OpenFiles,
    SelectDirectory,
    SaveFile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileDialogSpec {
    mode: FileDialogMode,
    title: Option<String>,
    prompt: Option<String>,
    initial_directory: Option<PathBuf>,
    suggested_name: Option<String>,
    allowed_extensions: Vec<String>,
    shows_hidden_files: bool,
}

impl FileDialogSpec {
    fn new(mode: FileDialogMode) -> Self {
        Self {
            mode,
            title: None,
            prompt: None,
            initial_directory: None,
            suggested_name: None,
            allowed_extensions: Vec::new(),
            shows_hidden_files: false,
        }
    }

    pub fn open_file() -> Self {
        Self::new(FileDialogMode::OpenFile)
    }
    pub fn open_files() -> Self {
        Self::new(FileDialogMode::OpenFiles)
    }
    pub fn select_directory() -> Self {
        Self::new(FileDialogMode::SelectDirectory)
    }
    pub fn save_file() -> Self {
        Self::new(FileDialogMode::SaveFile)
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Result<Self, DialogError> {
        let title = title.into();
        validate_text(&title)?;
        self.title = Some(title);
        Ok(self)
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Result<Self, DialogError> {
        let prompt = prompt.into();
        validate_text(&prompt)?;
        self.prompt = Some(prompt);
        Ok(self)
    }

    pub fn with_initial_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.initial_directory = Some(path.into());
        self
    }

    pub fn with_suggested_name(mut self, name: impl Into<String>) -> Result<Self, DialogError> {
        let name = name.into();
        validate_text(&name)?;
        self.suggested_name = Some(name);
        Ok(self)
    }

    pub fn with_allowed_extensions(
        mut self,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DialogError> {
        let extensions: Vec<String> = extensions.into_iter().map(Into::into).collect();
        if extensions.iter().any(|extension| {
            extension.is_empty()
                || extension.starts_with('.')
                || extension.contains(['/', '\\', '\0'])
        }) {
            return Err(DialogError::InvalidExtension);
        }
        self.allowed_extensions = extensions;
        Ok(self)
    }

    pub fn with_hidden_files(mut self, show: bool) -> Self {
        self.shows_hidden_files = show;
        self
    }

    pub fn mode(&self) -> FileDialogMode {
        self.mode
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
    pub fn initial_directory(&self) -> Option<&Path> {
        self.initial_directory.as_deref()
    }
    pub fn suggested_name(&self) -> Option<&str> {
        self.suggested_name.as_deref()
    }
    pub fn allowed_extensions(&self) -> &[String] {
        &self.allowed_extensions
    }
    pub fn shows_hidden_files(&self) -> bool {
        self.shows_hidden_files
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDialogResponse {
    Selected(Vec<PathBuf>),
    Cancelled,
}

impl FileDialogResponse {
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Self::Selected(paths) => paths,
            Self::Cancelled => &[],
        }
    }
}

impl Application {
    /// Runs a native application-modal `NSAlert` and returns the selected button.
    pub fn show_alert(&self, spec: &AlertSpec) -> Result<AlertResponse, DialogError> {
        let _ = self;
        show_alert(spec)
    }

    /// Runs a native open, directory, or save panel. Cancellation is not an error.
    pub fn show_file_dialog(
        &self,
        spec: &FileDialogSpec,
    ) -> Result<FileDialogResponse, DialogError> {
        let _ = self;
        show_file_dialog(spec)
    }
}

fn show_alert(spec: &AlertSpec) -> Result<AlertResponse, DialogError> {
    ensure_main_thread()?;
    let _pool = Pool::new().map_err(map_error)?;
    let alert = alloc_init::<NSAlert>().map_err(map_error)?;
    SET_MESSAGE_TEXT.send(&alert, (&*ns_string(&spec.message)?,));
    if let Some(detail) = &spec.detail {
        SET_INFORMATIVE_TEXT.send(&alert, (&*ns_string(detail)?,));
    }
    let style = match spec.style {
        AlertStyle::Warning => 0,
        AlertStyle::Informational => 1,
        AlertStyle::Critical => 2,
    };
    SET_ALERT_STYLE.send(&alert, (style,));
    for button in &spec.buttons {
        ADD_BUTTON_WITH_TITLE.send(&alert, (&*ns_string(&button.title)?,));
    }
    let response = ALERT_RUN_MODAL.send(&alert, ());
    AlertResponse::from_native_code(response, spec.buttons.len()).ok_or(DialogError::InvalidResult)
}

fn show_file_dialog(spec: &FileDialogSpec) -> Result<FileDialogResponse, DialogError> {
    ensure_main_thread()?;
    let _pool = Pool::new().map_err(map_error)?;
    let paths = if spec.mode == FileDialogMode::SaveFile {
        let panel = SAVE_PANEL
            .send_class(())
            .map_err(map_error)?
            .ok_or(DialogError::AppKitUnavailable)?;
        configure_panel(&panel, spec)?;
        if PANEL_RUN_MODAL.send(&panel, ()) != 1 {
            return Ok(FileDialogResponse::Cancelled);
        }
        vec![path_from_url(PANEL_URL.send(&panel, ()).as_deref())?]
    } else {
        let panel = OPEN_PANEL
            .send_class(())
            .map_err(map_error)?
            .ok_or(DialogError::AppKitUnavailable)?;
        configure_panel(&panel, spec)?;
        let directories = spec.mode == FileDialogMode::SelectDirectory;
        SET_CAN_CHOOSE_DIRECTORIES.send(&panel, (directories,));
        SET_CAN_CHOOSE_FILES.send(&panel, (!directories,));
        SET_ALLOWS_MULTIPLE_SELECTION.send(&panel, (spec.mode == FileDialogMode::OpenFiles,));
        if PANEL_RUN_MODAL.send(&panel, ()) != 1 {
            return Ok(FileDialogResponse::Cancelled);
        }
        let urls = PANEL_URLS
            .send(&panel, ())
            .ok_or(DialogError::InvalidResult)?;
        array_items(&urls)
            .map(|url| path_from_url(url.downcast::<NsUrl>()))
            .collect::<Result<_, _>>()?
    };
    if paths.is_empty() {
        Err(DialogError::InvalidResult)
    } else {
        Ok(FileDialogResponse::Selected(paths))
    }
}

fn configure_panel(panel: &Obj<NSSavePanel>, spec: &FileDialogSpec) -> Result<(), DialogError> {
    if let Some(title) = &spec.title {
        SET_TITLE.send(panel, (&*ns_string(title)?,));
    }
    if let Some(prompt) = &spec.prompt {
        SET_PROMPT.send(panel, (&*ns_string(prompt)?,));
    }
    if let Some(directory) = &spec.initial_directory {
        let path = directory.to_str().ok_or(DialogError::InvalidText)?;
        let url = FILE_URL_WITH_PATH
            .send_class((&*ns_string(path)?,))
            .map_err(map_error)?;
        SET_DIRECTORY_URL.send(panel, (url.as_deref(),));
    }
    if let Some(name) = &spec.suggested_name {
        SET_NAME_FIELD_STRING_VALUE.send(panel, (&*ns_string(name)?,));
    }
    SET_SHOWS_HIDDEN_FILES.send(panel, (spec.shows_hidden_files,));
    if !spec.allowed_extensions.is_empty() {
        let mut types = Vec::new();
        for extension in &spec.allowed_extensions {
            let kind = TYPE_WITH_FILENAME_EXTENSION
                .send_class((&*ns_string(extension)?,))
                .map_err(map_error)?
                .ok_or(DialogError::InvalidExtension)?;
            types.push(kind);
        }
        let objects: Vec<&Obj<NSObject>> = types.iter().map(|kind| kind.upcast()).collect();
        SET_ALLOWED_CONTENT_TYPES.send(panel, (&*ns_array(&objects).map_err(map_error)?,));
    }
    Ok(())
}

fn path_from_url(url: Option<&Obj<NsUrl>>) -> Result<PathBuf, DialogError> {
    let path = PATH
        .send(url.ok_or(DialogError::InvalidResult)?, ())
        .ok_or(DialogError::InvalidResult)?;
    string(&path)
        .map(PathBuf::from)
        .ok_or(DialogError::InvalidResult)
}

fn validate_text(value: &str) -> Result<(), DialogError> {
    if value.contains('\0') {
        Err(DialogError::InvalidText)
    } else {
        Ok(())
    }
}

fn ensure_main_thread() -> Result<(), DialogError> {
    if ffi::is_main_thread() {
        Ok(())
    } else {
        Err(DialogError::WrongThread)
    }
}

fn map_error(error: super::Error) -> DialogError {
    match error {
        super::Error::WrongThread => DialogError::WrongThread,
        _ => DialogError::AppKitUnavailable,
    }
}

fn ns_string(value: &str) -> Result<Retained<NSString>, DialogError> {
    foundation::ns_string(value).map_err(|error| match error {
        TextError::InteriorNul => DialogError::InvalidText,
        TextError::Unavailable => DialogError::AppKitUnavailable,
    })
}
