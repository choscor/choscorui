//! Desktop services that an application uses without platform code: the
//! clipboard, notifications, a status item, the URL-handler query, and
//! file dialogs. A backend implements [`ServiceBackend`]. Applications
//! use [`Services`], which checks the backend's capabilities before each
//! call, so a service that the backend does not provide returns a
//! [`ServiceError`] and never panics.

use crate::{BackendCapabilities, Condition, Support};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A desktop service, as an error names it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Service {
    Clipboard,
    Notifications,
    StatusItem,
    OpenUrls,
    FileDialogs,
}

impl Service {
    /// The service's name in error messages.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clipboard => "the clipboard",
            Self::Notifications => "notifications",
            Self::StatusItem => "the status item",
            Self::OpenUrls => "URL handlers",
            Self::FileDialogs => "file dialogs",
        }
    }

    /// The capability that gates the service.
    fn support(self, capabilities: &BackendCapabilities) -> Support {
        match self {
            Self::Clipboard => capabilities.clipboard,
            Self::Notifications => capabilities.notifications,
            Self::StatusItem => capabilities.status_items,
            Self::OpenUrls => capabilities.open_urls,
            Self::FileDialogs => capabilities.native_dialogs,
        }
    }
}

/// Why a service call failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceError {
    /// The backend does not provide the service.
    Unsupported(Service),
    /// The backend provides the service only when `condition` holds.
    Unavailable {
        service: Service,
        condition: Condition,
    },
    /// The user has not granted permission.
    PermissionDenied,
    /// The text contains an interior NUL character.
    InvalidText,
    /// The URL is not valid.
    InvalidUrl,
    /// The native call failed.
    Failed(Service),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(service) => {
                write!(f, "{} is not supported by this backend", service.name())
            }
            Self::Unavailable { service, condition } => {
                write!(f, "{} needs this condition: {condition}", service.name())
            }
            Self::PermissionDenied => f.write_str("the user has not granted permission"),
            Self::InvalidText => f.write_str("text contains an interior NUL character"),
            Self::InvalidUrl => f.write_str("the URL is not valid"),
            Self::Failed(service) => write!(f, "{} failed", service.name()),
        }
    }
}

impl std::error::Error for ServiceError {}

/// Whether the user lets the application post notifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationPermission {
    NotRequested,
    /// The application asked, and the answer has not arrived.
    Pending,
    Granted,
    Denied,
}

/// A notification to post. A new notification with the same identifier
/// replaces the old one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemNotification {
    pub identifier: String,
    pub title: String,
    pub body: String,
}

impl SystemNotification {
    /// A notification with `title` and `body`. A new notification with
    /// the same `identifier` replaces the old one.
    pub fn new(
        identifier: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            identifier: identifier.into(),
            title: title.into(),
            body: body.into(),
        }
    }
}

/// The item that an application shows in the system status area.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusItemSpec {
    pub title: String,
    pub tooltip: Option<String>,
}

impl StatusItemSpec {
    /// An item that shows `title`, without a tooltip.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            tooltip: None,
        }
    }

    /// The text that shows when the pointer rests on the item.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// What a file dialog chooses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileDialogKind {
    /// One existing file.
    Open,
    /// One or more existing files.
    OpenMany,
    /// One directory.
    Directory,
    /// A path to save to.
    Save,
}

/// A file dialog to show.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileDialogRequest {
    pub kind: FileDialogKind,
    pub title: Option<String>,
    /// File name extensions without the dot, such as `"txt"`. Empty
    /// allows every file.
    pub allowed_types: Vec<String>,
    /// The file name that a save dialog starts with.
    pub suggested_name: Option<String>,
}

impl FileDialogRequest {
    /// A dialog of `kind` that allows every file.
    pub fn new(kind: FileDialogKind) -> Self {
        Self {
            kind,
            title: None,
            allowed_types: Vec::new(),
            suggested_name: None,
        }
    }

    /// Allows only files with the extensions `types`.
    #[must_use]
    pub fn allowed_types(mut self, types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed_types = types.into_iter().map(Into::into).collect();
        self
    }

    /// The dialog's title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The name that a save dialog starts with.
    #[must_use]
    pub fn suggested_name(mut self, name: impl Into<String>) -> Self {
        self.suggested_name = Some(name.into());
        self
    }
}

/// The services of one backend. [`Services`] calls a method only when
/// the capability of its service is `Supported`. A method that a backend
/// leaves out reports the service as unsupported.
pub trait ServiceBackend {
    /// What the backend provides.
    fn capabilities(&self) -> BackendCapabilities;

    /// The text on the clipboard, if any.
    fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        Err(ServiceError::Unsupported(Service::Clipboard))
    }

    /// Replaces the clipboard contents with `text`.
    fn set_clipboard_text(&self, text: &str) -> Result<(), ServiceError> {
        let _ = text;
        Err(ServiceError::Unsupported(Service::Clipboard))
    }

    /// Whether the user lets the application post notifications.
    fn notification_permission(&self) -> Result<NotificationPermission, ServiceError> {
        Err(ServiceError::Unsupported(Service::Notifications))
    }

    /// Asks the user for notification permission. The answer arrives
    /// later.
    fn request_notification_permission(&self) -> Result<(), ServiceError> {
        Err(ServiceError::Unsupported(Service::Notifications))
    }

    /// Posts `notification`.
    fn post_notification(&self, notification: &SystemNotification) -> Result<(), ServiceError> {
        let _ = notification;
        Err(ServiceError::Unsupported(Service::Notifications))
    }

    /// The name of the application that opens `url`, if any.
    fn url_handler(&self, url: &str) -> Result<Option<String>, ServiceError> {
        let _ = url;
        Err(ServiceError::Unsupported(Service::OpenUrls))
    }

    /// Shows `item` in the status area, or removes the item for `None`.
    fn set_status_item(&self, item: Option<&StatusItemSpec>) -> Result<(), ServiceError> {
        let _ = item;
        Err(ServiceError::Unsupported(Service::StatusItem))
    }

    /// Shows the dialog of `request` and returns the chosen paths, empty
    /// when the user cancels.
    fn choose_files(&self, request: &FileDialogRequest) -> Result<Vec<PathBuf>, ServiceError> {
        let _ = request;
        Err(ServiceError::Unsupported(Service::FileDialogs))
    }
}

/// A backend without services.
struct Headless;

impl ServiceBackend for Headless {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::none("headless")
    }
}

/// The desktop services of the running backend. Each call checks the
/// capability of its service first: `Unsupported` returns
/// [`ServiceError::Unsupported`], and `Conditional` returns
/// [`ServiceError::Unavailable`] with the condition.
#[derive(Clone)]
pub struct Services {
    backend: Rc<dyn ServiceBackend>,
    capabilities: BackendCapabilities,
    status_item: Rc<RefCell<Option<StatusItemSpec>>>,
}

impl Default for Services {
    fn default() -> Self {
        Self::none()
    }
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services")
            .field("backend", &self.capabilities.backend)
            .finish_non_exhaustive()
    }
}

impl Services {
    /// The services of `backend`, with the capabilities that it reports
    /// now.
    pub fn new(backend: Rc<dyn ServiceBackend>) -> Self {
        Self {
            capabilities: backend.capabilities(),
            backend,
            status_item: Rc::default(),
        }
    }

    /// Services that a headless backend has: none.
    pub fn none() -> Self {
        Self::new(Rc::new(Headless))
    }

    /// What the backend provides.
    pub fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn check(&self, service: Service) -> Result<&dyn ServiceBackend, ServiceError> {
        match service.support(&self.capabilities) {
            Support::Supported => Ok(&*self.backend),
            Support::Unsupported => Err(ServiceError::Unsupported(service)),
            Support::Conditional(condition) => {
                Err(ServiceError::Unavailable { service, condition })
            }
        }
    }

    /// The text on the clipboard, if any.
    pub fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        self.check(Service::Clipboard)?.clipboard_text()
    }

    /// Replaces the clipboard contents with `text`.
    pub fn set_clipboard_text(&self, text: &str) -> Result<(), ServiceError> {
        self.check(Service::Clipboard)?.set_clipboard_text(text)
    }

    /// Whether the user lets the application post notifications.
    pub fn notification_permission(&self) -> Result<NotificationPermission, ServiceError> {
        self.check(Service::Notifications)?
            .notification_permission()
    }

    /// Asks the user for notification permission. Poll
    /// [`Services::notification_permission`] for the answer.
    pub fn request_notification_permission(&self) -> Result<(), ServiceError> {
        self.check(Service::Notifications)?
            .request_notification_permission()
    }

    /// Posts `notification`. When permission was never requested, asks
    /// for it first.
    pub fn post_notification(&self, notification: &SystemNotification) -> Result<(), ServiceError> {
        let backend = self.check(Service::Notifications)?;
        if backend.notification_permission()? == NotificationPermission::NotRequested {
            backend.request_notification_permission()?;
        }
        backend.post_notification(notification)
    }

    /// The name of the application that opens `url`, if any.
    pub fn url_handler(&self, url: &str) -> Result<Option<String>, ServiceError> {
        self.check(Service::OpenUrls)?.url_handler(url)
    }

    /// Shows `item` in the status area, or removes the item for `None`.
    pub fn set_status_item(&self, item: Option<StatusItemSpec>) -> Result<(), ServiceError> {
        self.check(Service::StatusItem)?
            .set_status_item(item.as_ref())?;
        *self.status_item.borrow_mut() = item;
        Ok(())
    }

    /// The status item that shows now, if any.
    pub fn status_item(&self) -> Option<StatusItemSpec> {
        self.status_item.borrow().clone()
    }

    /// Shows the file dialog of `request` and returns the chosen paths,
    /// empty when the user cancels.
    pub fn choose_files(&self, request: &FileDialogRequest) -> Result<Vec<PathBuf>, ServiceError> {
        self.check(Service::FileDialogs)?.choose_files(request)
    }
}
