//! The desktop services of the Linux backend, over the session bus:
//! notifications, the status item, URLs, the appearance settings, file
//! dialogs of the portal, and `org.freedesktop.Application`.
//!
//! `NativeServices` implements `choscorui_core::ServiceBackend`. Without
//! a session bus every D-Bus service reports `Unsupported`, and
//! `open_url` uses `xdg-open`. The backend owns the event loop: it waits
//! on the descriptors of `NativeServices::connection` and dispatches it.
//! The clipboard and the drawn file dialog belong to the windows, so the
//! backend sets them with `set_clipboard` and `set_file_dialog_fallback`.

mod appearance;
mod application;
mod dbusmenu;
mod file_chooser;
mod introspect;
mod mimeapps;
mod notifications;
mod portal;
mod status_item;
mod urls;

pub use appearance::SystemAppearance;
pub use application::launch_events;
pub use file_chooser::{FileChooserError, FileChooserRequest, FileDialogResponse, choose_files};
pub use mimeapps::XdgDirs;
pub use notifications::NotificationRoute;
pub use urls::{url_handler, url_handler_in};

use crate::dbus::{self, Connection};
use appearance::AppearanceWatch;
use application::ApplicationService;
use choscorui_core::{
    AccessibilityPreferences, BackendCapabilities, Condition, DesktopEvent, FileDialogRequest,
    NotificationPermission, Service, ServiceBackend, ServiceError, StatusItemSpec, Support,
    SystemNotification,
};
use notifications::Notifier;
use status_item::StatusItem;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Why a desktop request failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DesktopError {
    /// The text is not a URL with a scheme.
    InvalidUrl,
    /// No installed application opens the URL.
    NoHandler,
    /// The text contains an interior NUL character.
    InvalidText,
    /// The service is not on the session bus.
    Unavailable,
    Failed(String),
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("the URL is not valid"),
            Self::NoHandler => f.write_str("no application opens the URL"),
            Self::InvalidText => f.write_str("text contains an interior NUL character"),
            Self::Unavailable => f.write_str("the desktop service is not available"),
            Self::Failed(reason) => write!(f, "the desktop request failed: {reason}"),
        }
    }
}

impl std::error::Error for DesktopError {}

/// The application and its environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceOptions {
    /// The application id, such as `org.example.App`: the bus name of
    /// `org.freedesktop.Application` and the desktop entry.
    pub app_id: String,
    /// True in a Flatpak sandbox: notifications go through the portal.
    pub flatpak: bool,
}

impl ServiceOptions {
    /// The options of `app_id`. A sandbox has `/.flatpak-info`.
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            flatpak: Path::new("/.flatpak-info").exists(),
        }
    }
}

/// The drawn file dialog, for a session without a portal.
pub type FileDialogFallback =
    Box<dyn Fn(&FileChooserRequest) -> Result<FileDialogResponse, ServiceError>>;

/// The text clipboard of the windows.
pub trait TextClipboard {
    fn text(&self) -> Result<Option<String>, ServiceError>;
    fn set_text(&self, text: &str) -> Result<(), ServiceError>;
}

type EventQueue = Rc<RefCell<VecDeque<DesktopEvent>>>;

/// The desktop services of one application on the session bus.
pub struct NativeServices {
    connection: Option<Connection>,
    options: ServiceOptions,
    base: Cell<BackendCapabilities>,
    portal: bool,
    host: Cell<bool>,
    notifier: Option<Notifier>,
    status_item: RefCell<Option<StatusItem>>,
    appearance: Option<AppearanceWatch>,
    application: Option<ApplicationService>,
    events: EventQueue,
    visible_windows: Rc<Cell<bool>>,
    /// True while a window of the application has the keyboard focus.
    active: Cell<bool>,
    parent_window: RefCell<String>,
    file_dialog_fallback: RefCell<Option<FileDialogFallback>>,
    clipboard: RefCell<Option<Box<dyn TextClipboard>>>,
}

impl std::fmt::Debug for NativeServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeServices")
            .field("app_id", &self.options.app_id)
            .field("bus", &self.connection.is_some())
            .finish_non_exhaustive()
    }
}

fn failed(service: Service) -> impl Fn(dbus::Error) -> ServiceError {
    move |error| match error {
        dbus::Error::InvalidArgument(_) => ServiceError::InvalidText,
        _ => ServiceError::Failed(service),
    }
}

impl NativeServices {
    /// The services on a new connection to the session bus, or without
    /// D-Bus services when there is no session bus.
    pub fn new(options: ServiceOptions) -> Self {
        Self::with_connection(Connection::session().ok(), options)
    }

    /// The services on `connection`. It probes each service one time.
    pub fn with_connection(connection: Option<Connection>, options: ServiceOptions) -> Self {
        let events = EventQueue::default();
        let visible_windows = Rc::new(Cell::new(false));
        let mut services = Self {
            connection: None,
            base: Cell::new(BackendCapabilities::none("linux")),
            portal: false,
            host: Cell::new(false),
            notifier: None,
            status_item: RefCell::default(),
            appearance: None,
            application: None,
            events: Rc::clone(&events),
            visible_windows: Rc::clone(&visible_windows),
            active: Cell::new(false),
            parent_window: RefCell::default(),
            file_dialog_fallback: RefCell::default(),
            clipboard: RefCell::default(),
            options,
        };
        let Some(connection) = connection else {
            return services;
        };
        services.portal = connection.name_available(portal::DESKTOP).unwrap_or(false);
        let route = if services.options.flatpak {
            NotificationRoute::Portal
        } else {
            NotificationRoute::Server
        };
        services.notifier = Notifier::new(&connection, route, &services.options.app_id)
            .ok()
            .flatten();
        services.host.set(status_item::host_available(&connection));
        services.appearance = AppearanceWatch::new(&connection).ok().flatten();
        services.application = ApplicationService::export(
            &connection,
            &services.options.app_id,
            events,
            visible_windows,
        )
        .ok();
        services.connection = Some(connection);
        services
    }

    /// The session bus connection. The backend loop waits on its
    /// descriptors and dispatches it.
    pub fn connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    pub fn options(&self) -> &ServiceOptions {
        &self.options
    }

    /// The capabilities of the windows, which the service fields extend.
    pub fn set_base_capabilities(&self, base: BackendCapabilities) {
        self.base.set(base);
    }

    /// The portal handle of the window that owns the dialogs:
    /// `wayland:<handle>` or `x11:<xid>`.
    pub fn set_parent_window(&self, handle: impl Into<String>) {
        *self.parent_window.borrow_mut() = handle.into();
    }

    /// The dialog to show when no portal shows file dialogs.
    pub fn set_file_dialog_fallback(&self, fallback: Option<FileDialogFallback>) {
        *self.file_dialog_fallback.borrow_mut() = fallback;
    }

    /// The text clipboard of the windows. Without one, the clipboard is
    /// unsupported.
    pub fn set_clipboard(&self, clipboard: Option<Box<dyn TextClipboard>>) {
        *self.clipboard.borrow_mut() = clipboard;
    }

    /// Records whether a window of the application has the keyboard
    /// focus. A change queues `Activated` or `Deactivated`; an activation
    /// also reads the appearance settings again.
    pub fn set_active(&self, active: bool) {
        if self.active.replace(active) == active {
            return;
        }
        if active {
            self.reload_appearance();
        }
        self.events.borrow_mut().push_back(if active {
            DesktopEvent::Activated
        } else {
            DesktopEvent::Deactivated
        });
    }

    /// Queues `OpenFiles` and `OpenUrls` for the files and URLs in the
    /// command-line arguments, without the program name.
    pub fn open_launch_arguments(&self, args: impl IntoIterator<Item = std::ffi::OsString>) {
        self.events.borrow_mut().extend(launch_events(args));
    }

    /// Whether a window shows, for `DesktopEvent::Reopen`.
    pub fn set_has_visible_windows(&self, visible: bool) {
        self.visible_windows.set(visible);
    }

    /// True when this process owns the application name on the bus;
    /// `None` without a bus or a valid application id.
    pub fn is_primary_instance(&self) -> Option<bool> {
        self.application
            .as_ref()
            .map(ApplicationService::is_primary)
    }

    /// The route of the notifications, when a service posts them.
    pub fn notification_route(&self) -> Option<NotificationRoute> {
        self.notifier.as_ref().map(Notifier::route)
    }

    /// Opens `url` with the default application: through the `OpenURI`
    /// portal when one runs, else with `xdg-open`.
    pub fn open_url(&self, url: &str) -> Result<(), DesktopError> {
        urls::open_url(
            self.connection.as_ref(),
            self.portal,
            &self.parent_window.borrow(),
            url,
        )
    }

    /// What the desktop asks for: light with default preferences when no
    /// portal answers.
    pub fn appearance(&self) -> SystemAppearance {
        self.appearance
            .as_ref()
            .map(AppearanceWatch::current)
            .unwrap_or_default()
    }

    pub fn system_prefers_dark(&self) -> bool {
        self.appearance().prefers_dark
    }

    pub fn accessibility_preferences(&self) -> AccessibilityPreferences {
        self.appearance().accessibility
    }

    /// True once after an appearance setting changed.
    pub fn take_appearance_changed(&self) -> bool {
        self.appearance
            .as_ref()
            .is_some_and(AppearanceWatch::take_changed)
    }

    /// Reads the appearance settings again, for example when a window
    /// gains focus.
    pub fn reload_appearance(&self) {
        if let (Some(appearance), Some(connection)) = (&self.appearance, &self.connection) {
            appearance.reload(connection);
        }
    }

    /// Shows or hides the status item.
    pub fn set_status_item_visible(&self, visible: bool) -> Result<(), ServiceError> {
        match self.status_item.borrow().as_ref() {
            Some(item) => item
                .set_visible(visible)
                .map_err(failed(Service::StatusItem)),
            None => Ok(()),
        }
    }

    /// The bus name of the status item, while one shows.
    pub fn status_item_name(&self) -> Option<String> {
        self.status_item
            .borrow()
            .as_ref()
            .map(|item| item.bus_name().to_owned())
    }

    /// Shows the file dialog of `request`: the portal's, else the
    /// fallback. The wait for the portal runs a nested loop.
    pub fn show_file_dialog(
        &self,
        request: &FileChooserRequest,
    ) -> Result<FileDialogResponse, ServiceError> {
        let mut request = request.clone();
        if request.parent_window.is_empty() {
            request
                .parent_window
                .clone_from(&self.parent_window.borrow());
        }
        let portal = if self.portal {
            choose_files(self.connection.as_ref(), &request)
        } else {
            Err(FileChooserError::NoPortal)
        };
        match portal {
            Ok(response) => Ok(response),
            Err(FileChooserError::NoPortal) => match self.file_dialog_fallback.borrow().as_ref() {
                Some(fallback) => fallback(&request),
                None => Err(ServiceError::Unsupported(Service::FileDialogs)),
            },
            Err(FileChooserError::Failed(_)) => Err(ServiceError::Failed(Service::FileDialogs)),
        }
    }

    /// Moves the signals that came since the last call into events.
    fn poll(&self) {
        if let Some(notifier) = &self.notifier {
            let clicks = notifier.take_events();
            self.events.borrow_mut().extend(clicks);
        }
        if let Some(item) = self.status_item.borrow().as_ref() {
            item.poll();
        }
    }
}

impl ServiceBackend for NativeServices {
    fn capabilities(&self) -> BackendCapabilities {
        use Support::{Conditional, Supported, Unsupported};
        let mut capabilities = self.base.get();
        let bus = self.connection.is_some();
        capabilities.notifications = if self.notifier.is_some() {
            Supported
        } else {
            Unsupported
        };
        capabilities.status_items = match (bus, self.host.get()) {
            (false, _) => Unsupported,
            (true, true) => Supported,
            (true, false) => Conditional(Condition::StatusNotifierHost),
        };
        capabilities.open_urls = Supported;
        capabilities.system_appearance = if self.appearance.is_some() {
            Supported
        } else {
            Unsupported
        };
        let dialogs = self.portal || self.file_dialog_fallback.borrow().is_some();
        capabilities.native_dialogs = if dialogs { Supported } else { Unsupported };
        if self.clipboard.borrow().is_some() {
            capabilities.clipboard = Supported;
        }
        capabilities
    }

    fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        match self.clipboard.borrow().as_ref() {
            Some(clipboard) => clipboard.text(),
            None => Err(ServiceError::Unsupported(Service::Clipboard)),
        }
    }

    fn set_clipboard_text(&self, text: &str) -> Result<(), ServiceError> {
        match self.clipboard.borrow().as_ref() {
            Some(clipboard) => clipboard.set_text(text),
            None => Err(ServiceError::Unsupported(Service::Clipboard)),
        }
    }

    /// The desktop has no permission step for notifications.
    fn notification_permission(&self) -> Result<NotificationPermission, ServiceError> {
        match self.notifier {
            Some(_) => Ok(NotificationPermission::Granted),
            None => Err(ServiceError::Unsupported(Service::Notifications)),
        }
    }

    fn request_notification_permission(&self) -> Result<(), ServiceError> {
        self.notification_permission().map(drop)
    }

    fn post_notification(&self, notification: &SystemNotification) -> Result<(), ServiceError> {
        let (Some(notifier), Some(connection)) = (&self.notifier, &self.connection) else {
            return Err(ServiceError::Unsupported(Service::Notifications));
        };
        notifier
            .post(connection, notification)
            .map_err(failed(Service::Notifications))
    }

    /// The `Name` of the desktop entry of the handler, else its file
    /// name without `.desktop`.
    fn url_handler(&self, url: &str) -> Result<Option<String>, ServiceError> {
        let handler = urls::url_handler(url).map_err(|error| match error {
            DesktopError::InvalidUrl => ServiceError::InvalidUrl,
            _ => ServiceError::Failed(Service::OpenUrls),
        })?;
        Ok(handler.map(|path: PathBuf| {
            mimeapps::application_name(&path).unwrap_or_else(|| {
                path.file_stem().map_or_else(
                    || path.display().to_string(),
                    |stem| stem.to_string_lossy().into_owned(),
                )
            })
        }))
    }

    fn set_status_item(&self, item: Option<&StatusItemSpec>) -> Result<(), ServiceError> {
        let mut slot = self.status_item.borrow_mut();
        let Some(spec) = item else {
            *slot = None;
            return Ok(());
        };
        if let Some(existing) = slot.as_ref() {
            return existing.update(spec).map_err(failed(Service::StatusItem));
        }
        let connection = self
            .connection
            .as_ref()
            .ok_or(ServiceError::Unsupported(Service::StatusItem))?;
        let created = StatusItem::new(
            connection,
            &self.options.app_id,
            spec,
            Rc::clone(&self.events),
        )
        .map_err(failed(Service::StatusItem))?;
        *slot = Some(created);
        Ok(())
    }

    fn choose_files(&self, request: &FileDialogRequest) -> Result<Vec<PathBuf>, ServiceError> {
        let request = FileChooserRequest::from_request(request, &self.parent_window.borrow());
        self.show_file_dialog(&request)
            .map(|response| response.paths().to_vec())
    }

    /// The application events, the notification clicks, and the status
    /// menu commands, oldest first.
    fn take_events(&self) -> Vec<DesktopEvent> {
        self.poll();
        self.events.borrow_mut().drain(..).collect()
    }
}
