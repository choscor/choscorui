//! Desktop integration: application activation, reopen, and open-document
//! events; menu-bar status items; user notifications; and opening URLs
//! with the system handler.
#![deny(unsafe_code)]

use super::*;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    APPLICATION_DELEGATE, BUTTON_TITLE, ChoscoruiV2AppDelegate, IS_STATUS_ITEM_VISIBLE,
    NSApplication, NSStatusItem, OPEN_URLS, REMOVE_STATUS_ITEM, SET_APPLICATION_DELEGATE,
    SET_BUTTON_TITLE, SET_STATUS_ITEM_MENU, SET_STATUS_ITEM_VISIBLE, SET_TOOL_TIP,
    SHARED_APPLICATION, SHARED_WORKSPACE, SHOULD_HANDLE_REOPEN, STATUS_ITEM_BUTTON,
    STATUS_ITEM_WITH_LENGTH, SYSTEM_STATUS_BAR, URL_FOR_APPLICATION_TO_OPEN_URL,
    WORKSPACE_OPEN_URL,
};
use crate::ffi::block::{AuthorizationArgs, AuthorizationHandler, Block, GlobalBlock};
use crate::ffi::encode::Bool;
use crate::ffi::foundation::{
    ABSOLUTE_STRING, BUNDLE_IDENTIFIER, IS_FILE_URL, MAIN_BUNDLE, NSArray, NSNotification,
    NSObject, NSString, NsUrl, PATH, SCHEME, TextError, URL_WITH_STRING, array_items, string,
};
use crate::ffi::retained::{ProcessObject, Retained, alloc_init};
use crate::ffi::user_notifications::{
    ADD_NOTIFICATION_REQUEST, CURRENT_NOTIFICATION_CENTER, DEFAULT_SOUND, NEW_CONTENT,
    NOTIFICATION_REQUEST, REQUEST_AUTHORIZATION, REQUEST_IDENTIFIER, REQUEST_WITH_IDENTIFIER,
    RESPONSE_ACTION_IDENTIFIER, RESPONSE_NOTIFICATION, SET_CENTER_DELEGATE, SET_CONTENT_BODY,
    SET_CONTENT_SOUND, SET_CONTENT_TITLE, UNNotification, UNNotificationResponse,
    UNUserNotificationCenter,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

/// Application-level events, delivered in AppKit callback order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEvent {
    Activated,
    Deactivated,
    /// The Dock icon was clicked (or the app relaunched) while running.
    /// Show a window when `has_visible_windows` is false.
    Reopen {
        has_visible_windows: bool,
    },
    /// Documents opened from Finder, the Dock, or `open -a`.
    OpenFiles(Vec<PathBuf>),
    /// Non-file URLs for registered URL schemes.
    OpenUrls(Vec<String>),
    /// The user clicked a notification or one of its actions.
    NotificationActivated {
        identifier: String,
        action: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopError {
    InvalidUrl,
    /// No installed application handles the URL's scheme.
    NoHandler,
    /// User notifications need a bundled app (`Info.plist` with a
    /// `CFBundleIdentifier`); a bare executable cannot post them.
    RequiresAppBundle,
    PermissionDenied,
    InvalidText,
    AppKitUnavailable,
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("the URL is not valid"),
            Self::NoHandler => f.write_str("no application is registered for this URL"),
            Self::RequiresAppBundle => f.write_str(
                "this feature needs the process to run from an app bundle with a bundle identifier",
            ),
            Self::PermissionDenied => f.write_str("the user has not granted permission"),
            Self::InvalidText => f.write_str("text contains an interior NUL character"),
            Self::AppKitUnavailable => f.write_str("AppKit is unavailable"),
        }
    }
}

impl std::error::Error for DesktopError {}

impl From<Error> for DesktopError {
    fn from(error: Error) -> Self {
        match error {
            Error::InvalidTitle => Self::InvalidText,
            _ => Self::AppKitUnavailable,
        }
    }
}

fn app_events() -> &'static Mutex<VecDeque<AppEvent>> {
    static EVENTS: OnceLock<Mutex<VecDeque<AppEvent>>> = OnceLock::new();
    EVENTS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn push_event(event: AppEvent) {
    if let Ok(mut events) = app_events().lock() {
        events.push_back(event);
    }
}

fn urls_from_array(urls: &Obj<NSArray>) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut others = Vec::new();
    for url in array_items(urls).filter_map(|url| url.downcast::<NsUrl>()) {
        if IS_FILE_URL.send(url, ()) {
            if let Some(path) = PATH.send(url, ()).and_then(|path| string(&path)) {
                files.push(PathBuf::from(path));
            }
        } else if let Some(text) = ABSOLUTE_STRING.send(url, ()).and_then(|text| string(&text)) {
            others.push(text);
        }
    }
    (files, others)
}

pub(crate) fn app_did_become_active(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<NSNotification>>,
) {
    push_event(AppEvent::Activated);
}

pub(crate) fn app_did_resign_active(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<NSNotification>>,
) {
    push_event(AppEvent::Deactivated);
}

/// Queues a reopen event. The application decides what to show, so AppKit
/// does nothing more.
pub(crate) fn app_should_handle_reopen(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<NSApplication>>,
    visible: Bool,
) -> bool {
    push_event(AppEvent::Reopen {
        has_visible_windows: visible.get(),
    });
    false
}

pub(crate) fn app_open_urls(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<NSApplication>>,
    urls: Option<&Obj<NSArray>>,
) {
    let Some(urls) = urls else {
        return;
    };
    let (files, others) = urls_from_array(urls);
    if !files.is_empty() {
        push_event(AppEvent::OpenFiles(files));
    }
    if !others.is_empty() {
        push_event(AppEvent::OpenUrls(others));
    }
}

/// Calls a completion handler when it drops, also while a panic unwinds,
/// so the handler runs exactly one time.
struct CompletionHandler<'a>(Option<&'a Block<()>>);

impl Drop for CompletionHandler<'_> {
    fn drop(&mut self) {
        if let Some(handler) = self.0 {
            handler.call();
        }
    }
}

/// Queues the activation of a notification and then calls the completion
/// handler.
pub(crate) fn notification_did_receive(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<UNUserNotificationCenter>>,
    response: Option<&Obj<UNNotificationResponse>>,
    handler: Option<&Block<()>>,
) {
    let _handler = CompletionHandler(handler);
    let Some(response) = response else {
        return;
    };
    let identifier = RESPONSE_NOTIFICATION
        .send(response, ())
        .and_then(|notification| NOTIFICATION_REQUEST.send(notification, ()))
        .and_then(|request| REQUEST_IDENTIFIER.send(request, ()))
        .and_then(|identifier| string(&identifier))
        .unwrap_or_default();
    let action = RESPONSE_ACTION_IDENTIFIER
        .send(response, ())
        .and_then(|action| string(&action))
        .unwrap_or_default();
    push_event(AppEvent::NotificationActivated { identifier, action });
}

/// Shows banners while the app is frontmost too.
pub(crate) fn notification_will_present(
    _: &Obj<ChoscoruiV2AppDelegate>,
    _: Option<&Obj<UNUserNotificationCenter>>,
    _: Option<&Obj<UNNotification>>,
    handler: Option<&Block<(usize,)>>,
) {
    // UNNotificationPresentationOptionSound | List | Banner.
    if let Some(handler) = handler {
        handler.call((1 << 1) | (1 << 3) | (1 << 4));
    }
}

/// The process-wide delegate object. NSApplication and the notification
/// center hold delegates weakly, so it lives for the whole process.
fn app_delegate() -> Result<&'static Obj<NSObject>, Error> {
    static DELEGATE: OnceLock<Result<ProcessObject<NSObject>, Error>> = OnceLock::new();
    let result = DELEGATE.get_or_init(|| {
        ChoscoruiV2AppDelegate::register()?;
        alloc_init::<ChoscoruiV2AppDelegate>()
            .map(|delegate| ProcessObject::new(delegate.into_super::<NSObject>()))
    });
    match result {
        Ok(delegate) => delegate.get().ok_or(Error::WrongThread),
        Err(error) => Err(*error),
    }
}

/// Installs the delegate before `finishLaunching`, so open-document
/// events delivered at launch are not lost.
pub(super) fn install_app_delegate(application: &Obj<NSApplication>) -> Result<(), Error> {
    let delegate = app_delegate()?;
    let installed = APPLICATION_DELEGATE.send(application, ());
    if installed.is_none_or(|installed| installed.addr() != delegate.addr()) {
        SET_APPLICATION_DELEGATE.send(application, (Some(delegate),));
    }
    Ok(())
}

/// The shared `NSApplication`.
fn shared_application() -> Result<&'static Obj<NSApplication>, Error> {
    SHARED_APPLICATION
        .send_class(())?
        .ok_or(Error::AppKitUnavailable)
}

impl Application {
    /// Drains activation, reopen, open-document, and notification events.
    pub fn take_app_events(&self) -> Vec<AppEvent> {
        app_events().lock().unwrap().drain(..).collect()
    }

    /// Delivers an open-URLs callback through the installed delegate, as
    /// Finder would. Test support for the native callback path.
    #[doc(hidden)]
    pub fn simulate_open_urls(&self, urls: &[&str]) -> Result<(), DesktopError> {
        let _pool = Pool::new()?;
        let mut values = Vec::new();
        for url in urls {
            values.push(ns_url(url)?);
        }
        let objects: Vec<&Obj<NSObject>> = values.iter().map(|url| url.upcast()).collect();
        let array = ffi::foundation::ns_array(&objects)?;
        let application = shared_application()?;
        let delegate = APPLICATION_DELEGATE
            .send(application, ())
            .ok_or(DesktopError::AppKitUnavailable)?;
        OPEN_URLS.send(delegate, (application, &array));
        Ok(())
    }

    /// Delivers a Dock reopen callback through the installed delegate.
    #[doc(hidden)]
    pub fn simulate_reopen(&self, has_visible_windows: bool) -> bool {
        let _pool = Pool::new();
        let Ok(application) = shared_application() else {
            return false;
        };
        APPLICATION_DELEGATE
            .send(application, ())
            .is_some_and(|delegate| {
                SHOULD_HANDLE_REOPEN.send(delegate, (application, has_visible_windows))
            })
    }
}

/// An `NSString`, or `InvalidText` when `value` contains a NUL.
fn ns_string(value: &str) -> Result<Retained<NSString>, DesktopError> {
    ffi::foundation::ns_string(value).map_err(|error| match error {
        TextError::InteriorNul => DesktopError::InvalidText,
        TextError::Unavailable => DesktopError::AppKitUnavailable,
    })
}

fn ns_url(url: &str) -> Result<Retained<NsUrl>, DesktopError> {
    let text = ns_string(url).map_err(|_| DesktopError::InvalidUrl)?;
    let url = URL_WITH_STRING
        .send_class((&text,))?
        .ok_or(DesktopError::InvalidUrl)?;
    if SCHEME.send(&url, ()).is_none() {
        return Err(DesktopError::InvalidUrl);
    }
    Ok(url)
}

/// The application that would open `url` (for example Safari for
/// `https:`), without opening it.
pub fn url_handler(url: &str) -> Result<Option<PathBuf>, DesktopError> {
    let _pool = Pool::new()?;
    let url = ns_url(url)?;
    let workspace = SHARED_WORKSPACE
        .send_class(())?
        .ok_or(DesktopError::AppKitUnavailable)?;
    let Some(app) = URL_FOR_APPLICATION_TO_OPEN_URL.send(workspace, (&url,)) else {
        return Ok(None);
    };
    Ok(PATH
        .send(&app, ())
        .and_then(|path| string(&path))
        .map(PathBuf::from))
}

/// Opens `url` with the system's default handler (browser, mail client,
/// Finder for `file:` URLs). Sandboxed apps may open any URL this way.
pub fn open_url(url: &str) -> Result<(), DesktopError> {
    if url_handler(url)?.is_none() {
        return Err(DesktopError::NoHandler);
    }
    let _pool = Pool::new()?;
    let url = ns_url(url)?;
    let workspace = SHARED_WORKSPACE
        .send_class(())?
        .ok_or(DesktopError::AppKitUnavailable)?;
    if !WORKSPACE_OPEN_URL.send(workspace, (&url,)) {
        return Err(DesktopError::NoHandler);
    }
    Ok(())
}

/// A menu-bar extra (`NSStatusItem`) with a title, tooltip, and menu.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    menu: Option<ContextMenu>,
    _main_thread: PhantomData<Rc<()>>,
}

impl StatusItem {
    /// Creates a variable-length item. Call on the main thread after
    /// `Application::new`.
    pub fn new(title: &str) -> Result<Self, Error> {
        let _pool = Pool::new()?;
        let bar = SYSTEM_STATUS_BAR
            .send_class(())?
            .ok_or(Error::AppKitUnavailable)?;
        // NSVariableStatusItemLength = -1.
        let item = STATUS_ITEM_WITH_LENGTH
            .send(bar, (-1.0,))
            .ok_or(Error::AppKitUnavailable)?;
        let status = Self {
            item,
            menu: None,
            _main_thread: PhantomData,
        };
        status.set_title(title)?;
        Ok(status)
    }

    pub fn set_title(&self, title: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let title = ffi::foundation::ns_string(title)?;
        if let Some(button) = STATUS_ITEM_BUTTON.send(&self.item, ()) {
            SET_BUTTON_TITLE.send(button, (&title,));
        }
        Ok(())
    }

    pub fn title(&self) -> String {
        let _pool = Pool::new();
        STATUS_ITEM_BUTTON
            .send(&self.item, ())
            .and_then(|button| BUTTON_TITLE.send(button, ()))
            .and_then(|title| string(&title))
            .unwrap_or_default()
    }

    /// Tooltip, also used as the accessibility help text.
    pub fn set_tooltip(&self, tooltip: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let tooltip = ffi::foundation::ns_string(tooltip)?;
        if let Some(button) = STATUS_ITEM_BUTTON.send(&self.item, ()) {
            SET_TOOL_TIP.send(button, (Some(&tooltip),));
        }
        Ok(())
    }

    pub fn set_visible(&self, visible: bool) {
        let _pool = Pool::new();
        SET_STATUS_ITEM_VISIBLE.send(&self.item, (visible,));
    }

    pub fn is_visible(&self) -> bool {
        let _pool = Pool::new();
        IS_STATUS_ITEM_VISIBLE.send(&self.item, ())
    }

    /// Attaches the menu shown on click. The item owns the menu so its
    /// action target outlives every possible click.
    pub fn set_menu(&mut self, menu: Option<ContextMenu>) {
        let _pool = Pool::new();
        SET_STATUS_ITEM_MENU.send(&self.item, (menu.as_ref().map(ContextMenu::menu),));
        self.menu = menu;
    }

    pub fn menu(&self) -> Option<&ContextMenu> {
        self.menu.as_ref()
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        SET_STATUS_ITEM_MENU.send(&self.item, (None,));
        if let Ok(Some(bar)) = SYSTEM_STATUS_BAR.send_class(()) {
            REMOVE_STATUS_ITEM.send(bar, (&self.item,));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationPermission {
    NotRequested,
    Pending,
    Granted,
    Denied,
}

static PERMISSION: AtomicU8 = AtomicU8::new(0);

fn permission_from(value: u8) -> NotificationPermission {
    match value {
        1 => NotificationPermission::Pending,
        2 => NotificationPermission::Granted,
        3 => NotificationPermission::Denied,
        _ => NotificationPermission::NotRequested,
    }
}

/// True when the process runs from an app bundle with an identifier,
/// which UserNotifications requires.
pub fn has_bundle_identifier() -> bool {
    let Ok(_pool) = Pool::new() else {
        return false;
    };
    MAIN_BUNDLE
        .send_class(())
        .ok()
        .flatten()
        .is_some_and(|bundle| BUNDLE_IDENTIFIER.send(&bundle, ()).is_some())
}

fn notification_center() -> Result<&'static Obj<UNUserNotificationCenter>, DesktopError> {
    if !has_bundle_identifier() {
        return Err(DesktopError::RequiresAppBundle);
    }
    let center = CURRENT_NOTIFICATION_CENTER
        .send_class(())?
        .ok_or(DesktopError::AppKitUnavailable)?;
    SET_CENTER_DELEGATE.send(center, (Some(app_delegate()?),));
    Ok(center)
}

/// Stores the answer of the authorization request.
struct PermissionAnswer;

impl AuthorizationHandler for PermissionAnswer {
    fn finished(granted: bool) {
        PERMISSION.store(if granted { 2 } else { 3 }, Ordering::SeqCst);
    }
}

/// Asks for alert, sound, and badge permission. The answer arrives
/// asynchronously; poll `notification_permission`.
pub fn request_notification_permission() -> Result<(), DesktopError> {
    static BLOCK: OnceLock<GlobalBlock<AuthorizationArgs>> = OnceLock::new();
    let block = BLOCK.get_or_init(GlobalBlock::authorization::<PermissionAnswer>);
    let _pool = Pool::new()?;
    let center = notification_center()?;
    PERMISSION.store(1, Ordering::SeqCst);
    // UNAuthorizationOptionBadge | Sound | Alert.
    REQUEST_AUTHORIZATION.send(center, ((1 << 0) | (1 << 1) | (1 << 2), block.as_block()));
    Ok(())
}

pub fn notification_permission() -> NotificationPermission {
    permission_from(PERMISSION.load(Ordering::SeqCst))
}

/// Posts (or replaces, by identifier) a notification. Clicking it yields
/// `AppEvent::NotificationActivated`.
pub fn post_notification(identifier: &str, title: &str, body: &str) -> Result<(), DesktopError> {
    if notification_permission() == NotificationPermission::Denied {
        return Err(DesktopError::PermissionDenied);
    }
    let _pool = Pool::new()?;
    let center = notification_center()?;
    let content = NEW_CONTENT
        .send_class(())?
        .ok_or(DesktopError::AppKitUnavailable)?;
    SET_CONTENT_TITLE.send(&content, (&*ns_string(title)?,));
    SET_CONTENT_BODY.send(&content, (&*ns_string(body)?,));
    let sound = DEFAULT_SOUND.send_class(())?;
    SET_CONTENT_SOUND.send(&content, (sound.as_deref(),));
    let request = REQUEST_WITH_IDENTIFIER
        .send_class((&*ns_string(identifier)?, &content, None))?
        .ok_or(DesktopError::AppKitUnavailable)?;
    ADD_NOTIFICATION_REQUEST.send(center, (&request, None));
    Ok(())
}

/// What this backend provides on the running system. Notifications need
/// the process to run from an app bundle with a bundle identifier.
pub fn capabilities() -> choscorui_core::BackendCapabilities {
    use choscorui_core::Support::{Conditional, Supported, Unsupported};
    let mut capabilities = choscorui_core::BackendCapabilities::none("macos");
    capabilities.multiple_windows = Supported;
    capabilities.native_menus = Supported;
    capabilities.context_menus = Supported;
    capabilities.native_dialogs = Supported;
    capabilities.clipboard = Supported;
    capabilities.drag_and_drop = Supported;
    capabilities.input_methods = Supported;
    capabilities.accessibility = Supported;
    capabilities.status_items = Supported;
    capabilities.notifications = if has_bundle_identifier() {
        Supported
    } else {
        Conditional("run from an app bundle with a bundle identifier")
    };
    capabilities.open_urls = Supported;
    capabilities.system_appearance = Supported;
    capabilities.text_shaping = Supported;
    capabilities.image_codecs = Supported;
    capabilities.gpu_painting = Unsupported;
    capabilities
}
