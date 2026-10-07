use super::{Application, Id, Pool, Sel, objc_msgSend, selector};
use std::ffi::{CStr, CString, c_void};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

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
    unsafe {
        let alert = OwnedNative::new(send_id(send_id(class(c"NSAlert")?, c"alloc"), c"init"))?;
        send_void_id(
            alert.as_ptr(),
            c"setMessageText:",
            ns_string(&spec.message)?,
        );
        if let Some(detail) = &spec.detail {
            send_void_id(alert.as_ptr(), c"setInformativeText:", ns_string(detail)?);
        }
        let style = match spec.style {
            AlertStyle::Warning => 0,
            AlertStyle::Informational => 1,
            AlertStyle::Critical => 2,
        };
        send_void_isize(alert.as_ptr(), c"setAlertStyle:", style);
        for button in &spec.buttons {
            send_id_id(
                alert.as_ptr(),
                c"addButtonWithTitle:",
                ns_string(&button.title)?,
            );
        }
        let response = send_isize(alert.as_ptr(), c"runModal");
        AlertResponse::from_native_code(response, spec.buttons.len())
            .ok_or(DialogError::InvalidResult)
    }
}

fn show_file_dialog(spec: &FileDialogSpec) -> Result<FileDialogResponse, DialogError> {
    ensure_main_thread()?;
    let _pool = Pool::new().map_err(map_error)?;
    unsafe {
        let panel = match spec.mode {
            FileDialogMode::SaveFile => send_id(class(c"NSSavePanel")?, c"savePanel"),
            _ => send_id(class(c"NSOpenPanel")?, c"openPanel"),
        };
        let panel = NonNull::new(panel).ok_or(DialogError::AppKitUnavailable)?;
        configure_panel(panel.as_ptr(), spec)?;
        if send_isize(panel.as_ptr(), c"runModal") != 1 {
            return Ok(FileDialogResponse::Cancelled);
        }
        let paths = match spec.mode {
            FileDialogMode::OpenFile
            | FileDialogMode::OpenFiles
            | FileDialogMode::SelectDirectory => paths_from_urls(send_id(panel.as_ptr(), c"URLs"))?,
            FileDialogMode::SaveFile => vec![path_from_url(send_id(panel.as_ptr(), c"URL"))?],
        };
        if paths.is_empty() {
            Err(DialogError::InvalidResult)
        } else {
            Ok(FileDialogResponse::Selected(paths))
        }
    }
}

unsafe fn configure_panel(panel: Id, spec: &FileDialogSpec) -> Result<(), DialogError> {
    if let Some(title) = &spec.title {
        unsafe { send_void_id(panel, c"setTitle:", ns_string(title)?) };
    }
    if let Some(prompt) = &spec.prompt {
        unsafe { send_void_id(panel, c"setPrompt:", ns_string(prompt)?) };
    }
    if let Some(directory) = &spec.initial_directory {
        let path = directory.to_str().ok_or(DialogError::InvalidText)?;
        let url = unsafe { send_id_id(class(c"NSURL")?, c"fileURLWithPath:", ns_string(path)?) };
        unsafe { send_void_id(panel, c"setDirectoryURL:", url) };
    }
    if let Some(name) = &spec.suggested_name {
        unsafe { send_void_id(panel, c"setNameFieldStringValue:", ns_string(name)?) };
    }
    unsafe {
        send_void_bool(
            panel,
            c"setShowsHiddenFiles:",
            i8::from(spec.shows_hidden_files),
        )
    };
    if !spec.allowed_extensions.is_empty() {
        let strings: Result<Vec<_>, _> = spec
            .allowed_extensions
            .iter()
            .map(|s| ns_string(s))
            .collect();
        let strings = strings?;
        let array = unsafe {
            send_id_ptr_usize(
                class(c"NSArray")?,
                c"arrayWithObjects:count:",
                strings.as_ptr(),
                strings.len(),
            )
        };
        unsafe { send_void_id(panel, c"setAllowedFileTypes:", array) };
    }
    if spec.mode != FileDialogMode::SaveFile {
        let directories = spec.mode == FileDialogMode::SelectDirectory;
        unsafe {
            send_void_bool(panel, c"setCanChooseDirectories:", i8::from(directories));
            send_void_bool(panel, c"setCanChooseFiles:", i8::from(!directories));
            send_void_bool(
                panel,
                c"setAllowsMultipleSelection:",
                i8::from(spec.mode == FileDialogMode::OpenFiles),
            );
        }
    }
    Ok(())
}

unsafe fn paths_from_urls(urls: Id) -> Result<Vec<PathBuf>, DialogError> {
    let count = unsafe { send_usize(urls, c"count") };
    (0..count)
        .map(|index| unsafe { path_from_url(send_id_usize(urls, c"objectAtIndex:", index)) })
        .collect()
}

unsafe fn path_from_url(url: Id) -> Result<PathBuf, DialogError> {
    if url.is_null() {
        return Err(DialogError::InvalidResult);
    }
    let path = unsafe { send_id(url, c"path") };
    let utf8 = unsafe { send_cstr(path, c"UTF8String") };
    if utf8.is_null() {
        return Err(DialogError::InvalidResult);
    }
    Ok(PathBuf::from(
        unsafe { CStr::from_ptr(utf8) }
            .to_str()
            .map_err(|_| DialogError::InvalidResult)?,
    ))
}

fn validate_text(value: &str) -> Result<(), DialogError> {
    if value.contains('\0') {
        Err(DialogError::InvalidText)
    } else {
        Ok(())
    }
}

fn ensure_main_thread() -> Result<(), DialogError> {
    if unsafe { super::pthread_main_np() } == 0 {
        Err(DialogError::WrongThread)
    } else {
        Ok(())
    }
}

fn map_error(error: super::Error) -> DialogError {
    match error {
        super::Error::WrongThread => DialogError::WrongThread,
        _ => DialogError::AppKitUnavailable,
    }
}

struct OwnedNative(NonNull<c_void>);

impl OwnedNative {
    fn new(raw: Id) -> Result<Self, DialogError> {
        NonNull::new(raw)
            .map(Self)
            .ok_or(DialogError::AppKitUnavailable)
    }

    fn as_ptr(&self) -> Id {
        self.0.as_ptr()
    }
}

impl Drop for OwnedNative {
    fn drop(&mut self) {
        // SAFETY: this wrapper exclusively owns the +1 Objective-C reference.
        unsafe { send_void(self.0.as_ptr(), c"release") };
    }
}

fn ns_string(value: &str) -> Result<Id, DialogError> {
    let value = CString::new(value).map_err(|_| DialogError::InvalidText)?;
    unsafe {
        NonNull::new(send_id_cstr(
            class(c"NSString")?,
            c"stringWithUTF8String:",
            value.as_ptr(),
        ))
        .map(NonNull::as_ptr)
        .ok_or(DialogError::AppKitUnavailable)
    }
}

unsafe fn class(name: &CStr) -> Result<Id, DialogError> {
    unsafe { super::class(name) }.map_err(map_error)
}

unsafe fn send_id(receiver: Id, name: &CStr) -> Id {
    let function: unsafe extern "C" fn(Id, Sel) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name)) }
}
unsafe fn send_id_id(receiver: Id, name: &CStr, value: Id) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, Id) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_id_cstr(receiver: Id, name: &CStr, value: *const i8) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, *const i8) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_id_usize(receiver: Id, name: &CStr, value: usize) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_id_ptr_usize(receiver: Id, name: &CStr, values: *const Id, count: usize) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, *const Id, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), values, count) }
}
unsafe fn send_void(receiver: Id, name: &CStr) {
    let function: unsafe extern "C" fn(Id, Sel) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name)) }
}
unsafe fn send_void_id(receiver: Id, name: &CStr, value: Id) {
    let function: unsafe extern "C" fn(Id, Sel, Id) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_void_bool(receiver: Id, name: &CStr, value: i8) {
    let function: unsafe extern "C" fn(Id, Sel, i8) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_void_isize(receiver: Id, name: &CStr, value: isize) {
    let function: unsafe extern "C" fn(Id, Sel, isize) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
unsafe fn send_isize(receiver: Id, name: &CStr) -> isize {
    let function: unsafe extern "C" fn(Id, Sel) -> isize =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name)) }
}
unsafe fn send_usize(receiver: Id, name: &CStr) -> usize {
    let function: unsafe extern "C" fn(Id, Sel) -> usize =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name)) }
}
unsafe fn send_cstr(receiver: Id, name: &CStr) -> *const i8 {
    let function: unsafe extern "C" fn(Id, Sel) -> *const i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name)) }
}
