//! Desktop integration: application activation, reopen, and open-document
//! events; menu-bar status items; user notifications; and opening URLs
//! with the system handler.

use super::window_state::send_id_usize;
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

unsafe extern "C" {
    static _NSConcreteGlobalBlock: c_void;
}

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

#[repr(C)]
struct BlockHeader {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: *const c_void,
}

/// Calls an Objective-C block taking no arguments.
unsafe fn call_block(block: Id) {
    if block.is_null() {
        return;
    }
    unsafe {
        let invoke: unsafe extern "C" fn(Id) =
            std::mem::transmute((*(block as *const BlockHeader)).invoke);
        invoke(block)
    }
}

/// Calls an Objective-C block taking one NSUInteger.
unsafe fn call_block_usize(block: Id, value: usize) {
    if block.is_null() {
        return;
    }
    unsafe {
        let invoke: unsafe extern "C" fn(Id, usize) =
            std::mem::transmute((*(block as *const BlockHeader)).invoke);
        invoke(block, value)
    }
}

unsafe fn urls_from_array(urls: Id) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut others = Vec::new();
    if urls.is_null() {
        return (files, others);
    }
    unsafe {
        for index in 0..send_usize(urls, c"count") {
            let url = send_id_usize(urls, c"objectAtIndex:", index);
            if url.is_null() {
                continue;
            }
            if send_bool(url, c"isFileURL") != 0 {
                if let Some(path) = ffi::legacy::string(send_id(url, c"path")) {
                    files.push(PathBuf::from(path));
                }
            } else if let Some(text) = ffi::legacy::string(send_id(url, c"absoluteString")) {
                others.push(text);
            }
        }
    }
    (files, others)
}

unsafe extern "C" fn app_did_become_active(_: Id, _: Sel, _: Id) {
    let _ = catch_unwind(|| push_event(AppEvent::Activated));
}

unsafe extern "C" fn app_did_resign_active(_: Id, _: Sel, _: Id) {
    let _ = catch_unwind(|| push_event(AppEvent::Deactivated));
}

unsafe extern "C" fn app_should_handle_reopen(_: Id, _: Sel, _: Id, visible: i8) -> i8 {
    let _ = catch_unwind(|| {
        push_event(AppEvent::Reopen {
            has_visible_windows: visible != 0,
        })
    });
    // The application decides what to show; AppKit does nothing more.
    0
}

unsafe extern "C" fn app_open_urls(_: Id, _: Sel, _: Id, urls: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes an NSArray<NSURL *> valid for the call.
        let (files, others) = unsafe { urls_from_array(urls) };
        if !files.is_empty() {
            push_event(AppEvent::OpenFiles(files));
        }
        if !others.is_empty() {
            push_event(AppEvent::OpenUrls(others));
        }
    }));
}

unsafe extern "C" fn notification_did_receive(_: Id, _: Sel, _: Id, response: Id, handler: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: response is a live UNNotificationResponse for the call.
        unsafe {
            let request = send_id(send_id(response, c"notification"), c"request");
            let identifier =
                ffi::legacy::string(send_id(request, c"identifier")).unwrap_or_default();
            let action =
                ffi::legacy::string(send_id(response, c"actionIdentifier")).unwrap_or_default();
            push_event(AppEvent::NotificationActivated { identifier, action });
        }
    }));
    // SAFETY: the completion handler must be called exactly once.
    unsafe { call_block(handler) };
}

unsafe extern "C" fn notification_will_present(_: Id, _: Sel, _: Id, _: Id, handler: Id) {
    // Show banners while the app is frontmost too:
    // UNNotificationPresentationOptionSound | List | Banner.
    unsafe { call_block_usize(handler, (1 << 1) | (1 << 3) | (1 << 4)) };
}

unsafe fn app_delegate_class() -> Result<Id, Error> {
    static CLASS: OnceLock<Result<usize, Error>> = OnceLock::new();
    let result = CLASS.get_or_init(|| unsafe {
        let superclass = class(c"NSObject")?;
        let cls = objc_allocateClassPair(superclass, c"ChoscoruiV2AppDelegate".as_ptr(), 0);
        if cls.is_null() {
            return Err(Error::AppKitUnavailable);
        }
        let methods: [(&CStr, *const c_void, &CStr); 6] = [
            (
                c"applicationDidBecomeActive:",
                app_did_become_active as *const c_void,
                c"v@:@",
            ),
            (
                c"applicationDidResignActive:",
                app_did_resign_active as *const c_void,
                c"v@:@",
            ),
            (
                c"applicationShouldHandleReopen:hasVisibleWindows:",
                app_should_handle_reopen as *const c_void,
                c"c@:@c",
            ),
            (
                c"application:openURLs:",
                app_open_urls as *const c_void,
                c"v@:@@",
            ),
            (
                c"userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:",
                notification_did_receive as *const c_void,
                c"v@:@@@?",
            ),
            (
                c"userNotificationCenter:willPresentNotification:withCompletionHandler:",
                notification_will_present as *const c_void,
                c"v@:@@@?",
            ),
        ];
        for (name, imp, types) in methods {
            if class_addMethod(cls, selector(name), imp, types.as_ptr()) == 0 {
                objc_disposeClassPair(cls);
                return Err(Error::AppKitUnavailable);
            }
        }
        objc_registerClassPair(cls);
        Ok(cls as usize)
    });
    result.map(|value| value as Id)
}

/// The process-wide delegate object. NSApplication and the notification
/// center hold delegates weakly, so it lives for the whole process.
fn app_delegate() -> Result<Id, Error> {
    static DELEGATE: OnceLock<Result<usize, Error>> = OnceLock::new();
    let result = DELEGATE.get_or_init(|| unsafe {
        let delegate = send_id(send_id(app_delegate_class()?, c"alloc"), c"init");
        NonNull::new(delegate)
            .map(|delegate| delegate.as_ptr() as usize)
            .ok_or(Error::AppKitUnavailable)
    });
    result.map(|value| value as Id)
}

/// Installs the delegate before `finishLaunching`, so open-document
/// events delivered at launch are not lost.
pub(super) unsafe fn install_app_delegate(application: Id) -> Result<(), Error> {
    let delegate = app_delegate()?;
    unsafe {
        if send_id(application, c"delegate") != delegate {
            send_void_id(application, c"setDelegate:", delegate);
        }
    }
    Ok(())
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
        unsafe {
            let mut values = Vec::new();
            for url in urls {
                values.push(ns_url(url)?);
            }
            let array = ns_array(&values)?;
            let delegate = send_id(self.raw.as_ptr(), c"delegate");
            let f: unsafe extern "C" fn(Id, Sel, Id, Id) =
                std::mem::transmute(objc_msgSend as *const ());
            f(
                delegate,
                selector(c"application:openURLs:"),
                self.raw.as_ptr(),
                array,
            );
        }
        Ok(())
    }

    /// Delivers a Dock reopen callback through the installed delegate.
    #[doc(hidden)]
    pub fn simulate_reopen(&self, has_visible_windows: bool) -> bool {
        let _pool = Pool::new();
        unsafe {
            let delegate = send_id(self.raw.as_ptr(), c"delegate");
            let f: unsafe extern "C" fn(Id, Sel, Id, i8) -> i8 =
                std::mem::transmute(objc_msgSend as *const ());
            f(
                delegate,
                selector(c"applicationShouldHandleReopen:hasVisibleWindows:"),
                self.raw.as_ptr(),
                i8::from(has_visible_windows),
            ) != 0
        }
    }
}

unsafe fn ns_url(url: &str) -> Result<Id, DesktopError> {
    let text = ns_string(url).map_err(|_| DesktopError::InvalidUrl)?;
    unsafe {
        let url = send_id_id(class(c"NSURL")?, c"URLWithString:", text);
        if url.is_null() || send_id(url, c"scheme").is_null() {
            return Err(DesktopError::InvalidUrl);
        }
        Ok(url)
    }
}

/// The application that would open `url` (for example Safari for
/// `https:`), without opening it.
pub fn url_handler(url: &str) -> Result<Option<PathBuf>, DesktopError> {
    let _pool = Pool::new()?;
    unsafe {
        let url = ns_url(url)?;
        let workspace = send_id(class(c"NSWorkspace")?, c"sharedWorkspace");
        let app = send_id_id(workspace, c"URLForApplicationToOpenURL:", url);
        if app.is_null() {
            return Ok(None);
        }
        Ok(ffi::legacy::string(send_id(app, c"path")).map(PathBuf::from))
    }
}

/// Opens `url` with the system's default handler (browser, mail client,
/// Finder for `file:` URLs). Sandboxed apps may open any URL this way.
pub fn open_url(url: &str) -> Result<(), DesktopError> {
    if url_handler(url)?.is_none() {
        return Err(DesktopError::NoHandler);
    }
    let _pool = Pool::new()?;
    unsafe {
        let url = ns_url(url)?;
        let workspace = send_id(class(c"NSWorkspace")?, c"sharedWorkspace");
        if send_bool_id(workspace, c"openURL:", url) == 0 {
            return Err(DesktopError::NoHandler);
        }
    }
    Ok(())
}

/// A menu-bar extra (`NSStatusItem`) with a title, tooltip, and menu.
pub struct StatusItem {
    raw: NonNull<c_void>,
    menu: Option<ContextMenu>,
    _main_thread: PhantomData<Rc<()>>,
}

impl StatusItem {
    /// Creates a variable-length item. Call on the main thread after
    /// `Application::new`.
    pub fn new(title: &str) -> Result<Self, Error> {
        let _pool = Pool::new()?;
        unsafe {
            let bar = send_id(class(c"NSStatusBar")?, c"systemStatusBar");
            // NSVariableStatusItemLength = -1.
            let item = send_id_f64(bar, c"statusItemWithLength:", -1.0);
            let raw = NonNull::new(send_id(item, c"retain")).ok_or(Error::AppKitUnavailable)?;
            let status = Self {
                raw,
                menu: None,
                _main_thread: PhantomData,
            };
            status.set_title(title)?;
            Ok(status)
        }
    }

    fn button(&self) -> Id {
        unsafe { send_id(self.raw.as_ptr(), c"button") }
    }

    pub fn set_title(&self, title: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        unsafe { send_void_id(self.button(), c"setTitle:", ns_string(title)?) };
        Ok(())
    }

    pub fn title(&self) -> String {
        let _pool = Pool::new();
        unsafe { ffi::legacy::string(send_id(self.button(), c"title")).unwrap_or_default() }
    }

    /// Tooltip, also used as the accessibility help text.
    pub fn set_tooltip(&self, tooltip: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        unsafe { send_void_id(self.button(), c"setToolTip:", ns_string(tooltip)?) };
        Ok(())
    }

    pub fn set_visible(&self, visible: bool) {
        let _pool = Pool::new();
        unsafe { send_void_bool(self.raw.as_ptr(), c"setVisible:", i8::from(visible)) }
    }

    pub fn is_visible(&self) -> bool {
        let _pool = Pool::new();
        unsafe { send_bool(self.raw.as_ptr(), c"isVisible") != 0 }
    }

    /// Attaches the menu shown on click. The item owns the menu so its
    /// action target outlives every possible click.
    pub fn set_menu(&mut self, menu: Option<ContextMenu>) {
        let _pool = Pool::new();
        unsafe {
            send_void_id(
                self.raw.as_ptr(),
                c"setMenu:",
                menu.as_ref()
                    .map_or(std::ptr::null_mut(), |menu| ffi::legacy::id(menu.menu())),
            )
        };
        self.menu = menu;
    }

    pub fn menu(&self) -> Option<&ContextMenu> {
        self.menu.as_ref()
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        unsafe {
            send_void_id(self.raw.as_ptr(), c"setMenu:", std::ptr::null_mut());
            if let Ok(bar_class) = class(c"NSStatusBar") {
                let bar = send_id(bar_class, c"systemStatusBar");
                send_void_id(bar, c"removeStatusItem:", self.raw.as_ptr());
            }
            send_void(self.raw.as_ptr(), c"release");
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
    unsafe {
        let Ok(bundle_class) = class(c"NSBundle") else {
            return false;
        };
        let bundle = send_id(bundle_class, c"mainBundle");
        !bundle.is_null() && !send_id(bundle, c"bundleIdentifier").is_null()
    }
}

unsafe fn notification_center() -> Result<Id, DesktopError> {
    if !has_bundle_identifier() {
        return Err(DesktopError::RequiresAppBundle);
    }
    unsafe {
        let center = send_id(
            class(c"UNUserNotificationCenter")?,
            c"currentNotificationCenter",
        );
        if center.is_null() {
            return Err(DesktopError::AppKitUnavailable);
        }
        send_void_id(center, c"setDelegate:", app_delegate()?);
        Ok(center)
    }
}

#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
    signature: *const i8,
}

// SAFETY: the descriptor is immutable and points at a static C string.
unsafe impl Sync for BlockDescriptor {}

#[repr(C)]
struct GlobalBlock {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: unsafe extern "C" fn(*const GlobalBlock, i8, Id),
    descriptor: *const BlockDescriptor,
}

unsafe extern "C" fn authorization_finished(_: *const GlobalBlock, granted: i8, _: Id) {
    PERMISSION.store(if granted != 0 { 2 } else { 3 }, Ordering::SeqCst);
}

/// Asks for alert, sound, and badge permission. The answer arrives
/// asynchronously; poll `notification_permission`.
pub fn request_notification_permission() -> Result<(), DesktopError> {
    static DESCRIPTOR: BlockDescriptor = BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<GlobalBlock>(),
        signature: c"v@?c@".as_ptr(),
    };
    struct SyncBlock(GlobalBlock);
    // SAFETY: the block is immutable after creation and has no captures.
    unsafe impl Sync for SyncBlock {}
    unsafe impl Send for SyncBlock {}
    static BLOCK: OnceLock<SyncBlock> = OnceLock::new();
    let block = BLOCK.get_or_init(|| {
        SyncBlock(GlobalBlock {
            // Only the symbol's address is taken.
            isa: &raw const _NSConcreteGlobalBlock,
            // BLOCK_IS_GLOBAL | BLOCK_HAS_SIGNATURE
            flags: (1 << 28) | (1 << 30),
            reserved: 0,
            invoke: authorization_finished,
            descriptor: &DESCRIPTOR,
        })
    });
    let _pool = Pool::new()?;
    unsafe {
        let center = notification_center()?;
        PERMISSION.store(1, Ordering::SeqCst);
        // UNAuthorizationOptionBadge | Sound | Alert.
        let f: unsafe extern "C" fn(Id, Sel, usize, *const GlobalBlock) =
            std::mem::transmute(objc_msgSend as *const ());
        f(
            center,
            selector(c"requestAuthorizationWithOptions:completionHandler:"),
            (1 << 0) | (1 << 1) | (1 << 2),
            &block.0,
        );
    }
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
    unsafe {
        let center = notification_center()?;
        let content = send_id(class(c"UNMutableNotificationContent")?, c"new");
        if content.is_null() {
            return Err(DesktopError::AppKitUnavailable);
        }
        let result = (|| {
            send_void_id(content, c"setTitle:", ns_string(title)?);
            send_void_id(content, c"setBody:", ns_string(body)?);
            send_void_id(
                content,
                c"setSound:",
                send_id(class(c"UNNotificationSound")?, c"defaultSound"),
            );
            let f: unsafe extern "C" fn(Id, Sel, Id, Id, Id) -> Id =
                std::mem::transmute(objc_msgSend as *const ());
            let request = f(
                class(c"UNNotificationRequest")?,
                selector(c"requestWithIdentifier:content:trigger:"),
                ns_string(identifier)?,
                content,
                std::ptr::null_mut(),
            );
            if request.is_null() {
                return Err(DesktopError::AppKitUnavailable);
            }
            let add: unsafe extern "C" fn(Id, Sel, Id, Id) =
                std::mem::transmute(objc_msgSend as *const ());
            add(
                center,
                selector(c"addNotificationRequest:withCompletionHandler:"),
                request,
                std::ptr::null_mut(),
            );
            Ok(())
        })();
        send_void(content, c"release");
        result
    }
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
