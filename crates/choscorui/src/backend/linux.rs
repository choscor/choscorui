//! The Wayland and X11 backend: `choscorui-linux` behind the portable
//! interface.

mod chrome;
mod context_menu;

use crate::{AppContext, AppError, WindowId};
use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, Env, Live, ViewHost};
use choscorui_controls::{EstimatedText, ShapedText, ShortcutModifiers, TextMeasure};
use choscorui_core::{
    AnnouncementPriority, BackendCapabilities, CompositionEvent, CursorShape, DesignTokens,
    DisplayList, Feedback, MenuBarModel, MenuCommandId, MenuError, Point, Rect, SemanticChange,
    SemanticEntry, ServiceError, Services, Size, Support, WindowGeometry, WindowOptions,
};
use choscorui_linux::atspi::{AccessibleWindow, Bridge};
use choscorui_linux::services::{NativeServices, ServiceOptions, TextClipboard};
use choscorui_linux::{
    Application, ContextMenu, Error, ImeEvent, LinuxTextShaper, MainMenu, Modifiers,
    SystemClipboard, TextInputState, Window, WindowEvent,
};
use chrome::{Chrome, Menus};
use std::rc::{Rc, Weak};
use std::sync::OnceLock;
use std::time::Duration;

mod modal;

static SHAPER: LinuxTextShaper = LinuxTextShaper;

/// The backend for Linux: `choscorui-linux` in full, and native access
/// to the windows of an [`AppContext`]. Code that uses it is not
/// portable; prefer the services and capabilities of the prelude.
pub mod platform {
    pub use super::NativeAccess;
    pub use super::modal::{DrawnDialogs, ModalDriver, ModalTurn};
    pub use choscorui_core::{AlertButton, AlertResponse, AlertSpec, AlertStyle, DialogError};
    pub use choscorui_linux::*;
}

/// Native access to a running application, for verification harnesses
/// and features that the portable layer does not cover.
pub trait NativeAccess {
    /// The Wayland or X11 application.
    fn native_application(&self) -> &Application;
    /// The native window of `id`, for its paint count or geometry.
    fn native_window(&mut self, id: WindowId) -> Option<&mut Window>;
    /// The menus of the command menus, which each window's menu bar
    /// shows.
    fn native_menu(&self) -> Option<&MainMenu>;
    /// Shows `menu` in a popup at `at`, in the content coordinates of
    /// window `id`. It runs a nested event loop until the popup closes.
    /// Returns the chosen command, which also waits in the menu's
    /// `take_activations`. Returns None when the user dismisses the
    /// menu, or when the display server cannot show popups (X11).
    fn show_context_menu(
        &mut self,
        id: WindowId,
        menu: &ContextMenu,
        at: Point,
    ) -> Option<MenuCommandId>;
}

impl NativeAccess for AppContext {
    fn native_application(&self) -> &Application {
        &self.platform.application
    }

    fn native_window(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| &mut entry.native.window)
    }

    fn native_menu(&self) -> Option<&MainMenu> {
        let commands = self.commands.as_ref()?;
        commands.native.as_ref().map(|native| &native.menu)
    }

    fn show_context_menu(
        &mut self,
        id: WindowId,
        menu: &ContextMenu,
        at: Point,
    ) -> Option<MenuCommandId> {
        context_menu::show(self, id, menu, at)
    }
}

/// What the backend provides on the running system: the display server
/// features, and the desktop services on the session bus.
pub(crate) fn capabilities() -> BackendCapabilities {
    // The services are probed once, on a connection of their own.
    static SERVICES: OnceLock<BackendCapabilities> = OnceLock::new();
    let mut capabilities = choscorui_linux::capabilities();
    // Without a display server the application cannot start, so it
    // reports no service either.
    if !capabilities.multiple_windows.is_supported() {
        return capabilities;
    }
    let services = *SERVICES.get_or_init(|| {
        let probe = NativeServices::new(ServiceOptions::new(""));
        probe.set_base_capabilities(BackendCapabilities::none("linux"));
        choscorui_core::ServiceBackend::capabilities(&probe)
    });
    capabilities.notifications = services.notifications;
    capabilities.status_items = services.status_items;
    capabilities.open_urls = services.open_urls;
    capabilities.system_appearance = services.system_appearance;
    // Without a portal, the toolkit draws the file dialog.
    capabilities.native_dialogs = Support::Supported;
    // The accessibility bus can start or stop at any time, so each call
    // asks again.
    capabilities.accessibility = choscorui_linux::atspi::accessibility_support();
    capabilities
}

/// The `multiple_windows` capability of [`capabilities`].
pub(crate) fn window_support() -> Support {
    capabilities().multiple_windows
}

fn app_error(error: Error) -> AppError {
    match error {
        Error::WrongThread => AppError::WrongThread,
        Error::InvalidSize | Error::InvalidTitle => AppError::InvalidWindowOptions,
        Error::NoDisplay(_) | Error::MissingProtocol(_) | Error::Library(_) => {
            AppError::Unsupported
        }
        _ => AppError::Unavailable,
    }
}

/// The Wayland or X11 application, its desktop services, and its
/// AT-SPI bridge when the accessibility bus is reachable, and the menus
/// that the windows draw.
pub(crate) struct Platform {
    application: Rc<Application>,
    services: Rc<NativeServices>,
    modal: Rc<modal::Modal>,
    accessibility: Option<Rc<Bridge>>,
    /// The menus that the windows draw.
    menus: Menus,
}

/// The text clipboard of the services: the selection of the display
/// server.
struct Selection(SystemClipboard);

impl TextClipboard for Selection {
    fn text(&self) -> Result<Option<String>, ServiceError> {
        self.0
            .read_text()
            .map_err(|_| ServiceError::Failed(choscorui_core::Service::Clipboard))
    }

    fn set_text(&self, text: &str) -> Result<(), ServiceError> {
        self.0.write_text(text).map_err(|error| match error {
            Error::InvalidClipboardText => ServiceError::InvalidText,
            _ => ServiceError::Failed(choscorui_core::Service::Clipboard),
        })
    }
}

/// The alert sound of the application.
struct Bell(Weak<Application>);

impl Feedback for Bell {
    fn beep(&self) {
        if let Some(application) = self.0.upgrade() {
            application.beep();
        }
    }
}

impl Platform {
    pub(crate) fn new(id: Option<&str>) -> Result<Self, AppError> {
        let application = Rc::new(Application::new().map_err(app_error)?);
        if let Some(id) = id {
            application.set_app_id(id);
        }
        let services = NativeServices::new(ServiceOptions::new(id.unwrap_or_default()));
        services.set_base_capabilities(capabilities());
        if capabilities().clipboard.is_supported() {
            services.set_clipboard(Some(Box::new(Selection(application.clipboard()))));
        }
        services.open_launch_arguments(std::env::args_os().skip(1));
        if let Some(bus) = services.connection() {
            application.attach_bus(bus.clone());
        }
        // Without the accessibility bus, the application runs without
        // the bridge.
        let accessibility = Bridge::connect(id.unwrap_or("ChoscorUI application")).ok();
        if let Some(bridge) = &accessibility {
            application.attach_bus(bridge.connection().clone());
        }
        let modal = Rc::new(modal::Modal::new(
            application.clone(),
            accessibility.clone(),
            Self::text_measure(),
        ));
        services.set_file_dialog_fallback(Some(modal::file_dialog_fallback(&modal)));
        Ok(Self {
            application,
            services: Rc::new(services),
            modal,
            accessibility,
            menus: Menus::default(),
        })
    }

    /// Gives the drawn dialogs the runtime, the environment, and the
    /// tokens of the application.
    pub(crate) fn attach(&self, runtime: &Runtime, env: &Env, tokens: &Live<DesignTokens>) {
        self.modal.attach(runtime, env, tokens);
    }

    /// The text measurer of the windows: HarfBuzz shaping, or estimated
    /// metrics when FreeType, HarfBuzz, or fontconfig does not load.
    pub(crate) fn text(&self) -> Rc<dyn TextMeasure> {
        Self::text_measure()
    }

    fn text_measure() -> Rc<dyn TextMeasure> {
        if choscorui_linux::text_shaping_support().is_supported() {
            Rc::new(ShapedText::new(&SHAPER))
        } else {
            Rc::new(EstimatedText)
        }
    }

    /// The alert sound of the controls.
    pub(crate) fn feedback(&self) -> Rc<dyn Feedback> {
        Rc::new(Bell(Rc::downgrade(&self.application)))
    }

    /// The desktop services.
    pub(crate) fn services(&self) -> Services {
        Services::new(self.services.clone())
    }

    pub(crate) fn open_window(&self, options: &WindowOptions) -> Result<NativeWindow, AppError> {
        let window = self
            .application
            .create_window(options.clone())
            .map_err(app_error)?;
        let accessible = self
            .accessibility
            .as_ref()
            .map(|bridge| bridge.add_window(&window));
        Ok(NativeWindow {
            window,
            accessible,
            chrome: None,
            waiting: Vec::new(),
        })
    }

    /// Puts the chrome of `native` above `content`: a title bar when the
    /// display server draws no decorations, and the menu bar.
    pub(crate) fn frame(
        &self,
        native: &mut NativeWindow,
        content: AnyView,
        runtime: &Runtime,
    ) -> AnyView {
        let (chrome, view) = Chrome::compose(&native.window, &self.menus, runtime, content);
        native.chrome = Some(chrome);
        view
    }

    /// Installs the menus that each window's menu bar shows.
    pub(crate) fn install_menu(&self, model: MenuBarModel) -> Result<NativeMenu, MenuError> {
        let menu = self.application.install_main_menu(model)?;
        self.menus.install(menu.clone());
        Ok(NativeMenu { menu })
    }

    /// Waits up to `timeout` for a native event and handles it. A lost
    /// display connection gives each window a close request.
    pub(crate) fn wait(&self, timeout: Duration) {
        if let Err(error) = self.application.dispatch_next_event(timeout) {
            eprintln!("choscorui: {error}");
            std::thread::sleep(timeout);
        }
        self.services.set_active(self.application.is_active());
        self.services
            .set_has_visible_windows(self.application.has_visible_windows());
        // Portal dialogs open over the focused window.
        if let Some(handle) = self.application.focused_window_handle() {
            self.services.set_parent_window(handle);
        }
    }

    /// The compositor or window manager decides about focus; new windows
    /// ask for it when they show.
    pub(crate) fn activate(&self) {}

    /// The logical rectangles of the screens.
    pub(crate) fn screens(&self) -> Vec<Rect> {
        self.application.screens()
    }
}

fn shortcut_modifiers(modifiers: Modifiers) -> ShortcutModifiers {
    ShortcutModifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        option: modifiers.option,
        command: modifiers.command,
    }
}

fn composition(event: ImeEvent) -> CompositionEvent {
    match event {
        ImeEvent::SetMarkedText {
            text,
            selected_utf16,
            replacement_utf16,
        } => CompositionEvent::Mark {
            text,
            selected_utf16,
            replacement_utf16,
        },
        ImeEvent::CommitText {
            text,
            replacement_utf16,
        } => CompositionEvent::Commit {
            text,
            replacement_utf16,
        },
        ImeEvent::Unmark => CompositionEvent::Unmark,
    }
}

/// A Wayland or X11 window and its accessible side.
pub(crate) struct NativeWindow {
    window: Window,
    accessible: Option<AccessibleWindow>,
    chrome: Option<Chrome>,
    /// Events that arrived while a popup ran its loop.
    waiting: Vec<WindowEvent>,
}

impl NativeWindow {
    pub(crate) fn content_size(&self) -> Size {
        self.window.content_size()
    }

    pub(crate) fn show(&mut self) {
        self.window.show();
    }

    /// Routes the window's events to `host`. Returns whether the window
    /// asked to close, and its new focus state, if it changed.
    pub(crate) fn deliver(&mut self, host: &mut ViewHost) -> (bool, Option<bool>) {
        let (mut closing, mut focus) = (false, None);
        let mut events = std::mem::take(&mut self.waiting);
        events.extend(self.window.take_events());
        for event in events {
            match event {
                WindowEvent::Closing => closing = true,
                WindowEvent::Focused(focused) => {
                    if !focused {
                        host.cancel_interaction();
                    }
                    if let Some(accessible) = &self.accessible {
                        accessible.set_active(focused);
                    }
                    if let Some(chrome) = &mut self.chrome {
                        chrome.set_active(focused);
                    }
                    focus = Some(focused);
                }
                WindowEvent::Resized { size, .. } => host.resize(size),
                WindowEvent::Pointer {
                    event,
                    modifiers,
                    details,
                } => {
                    let framed = (self.chrome.as_mut())
                        .is_some_and(|chrome| chrome.pointer(&self.window, host, event));
                    if !framed {
                        host.pointer_with(event, shortcut_modifiers(modifiers), details);
                    }
                }
                WindowEvent::Key { event, modifiers } => {
                    let modifiers = shortcut_modifiers(modifiers);
                    let used = (self.chrome.as_ref())
                        .is_some_and(|chrome| chrome.key(host, event, modifiers));
                    if !used {
                        host.key(event, modifiers);
                    }
                }
                WindowEvent::Gesture { event, modifiers } => {
                    host.gesture(event, shortcut_modifiers(modifiers));
                }
                WindowEvent::Text { text } => {
                    host.text(&text);
                }
                WindowEvent::Ime(event) => {
                    host.composition(composition(event));
                }
                WindowEvent::Occluded(occluded) => host.set_window_visible(!occluded),
                _ => {}
            }
        }
        let requests = self
            .accessible
            .as_ref()
            .map(AccessibleWindow::take_actions)
            .unwrap_or_default();
        for request in requests {
            match request.part {
                Some(part) => host.perform_part(request.node, part, request.action),
                None => host.perform(request.node, request.action, request.value.as_deref()),
            };
        }
        if let Some(chrome) = &self.chrome {
            closing |= chrome.sync(&self.window);
        }
        let input = host.text_input();
        let active = input.is_some();
        let input = input.unwrap_or_default();
        self.window.set_text_input_state(TextInputState {
            active,
            text: input.text,
            selected_utf16: input.selected_utf16,
            marked_utf16: input.marked_utf16,
            caret: input.caret,
            captures_keys: host.captures_keys(),
        });
        (closing, focus)
    }

    pub(crate) fn title(&self) -> String {
        self.window.title()
    }

    pub(crate) fn set_title(&self, title: &str) {
        let _ = self.window.set_title(title);
    }

    pub(crate) fn present(&self, list: DisplayList) {
        self.window.set_display_list(list);
    }

    /// Shows `shape` over the content. A shape that the server cannot
    /// show keeps the cursor that shows.
    pub(crate) fn set_cursor(&self, shape: CursorShape) {
        let shape = match &self.chrome {
            Some(chrome) => chrome.cursor(shape),
            None => shape,
        };
        let _ = self.window.set_cursor(shape);
    }

    /// Asks the screen reader to speak `text` through AT-SPI. Without
    /// the accessibility bus, nothing speaks.
    pub(crate) fn announce(&self, text: &str, priority: AnnouncementPriority) {
        if let Some(accessible) = &self.accessible {
            accessible.announce(text, priority);
        }
    }

    /// Publishes the semantic tree on the accessibility bus. The bridge
    /// finds the changes itself, so it does not read `_changes`.
    pub(crate) fn publish(
        &self,
        snapshot: &[SemanticEntry],
        _changes: &[SemanticChange],
    ) -> Result<(), String> {
        if let Some(accessible) = &self.accessible {
            accessible.publish(snapshot);
        }
        Ok(())
    }

    pub(crate) fn geometry(&self) -> WindowGeometry {
        self.window.geometry()
    }

    pub(crate) fn set_geometry(&mut self, geometry: &WindowGeometry) {
        let _ = self.window.set_geometry(geometry);
    }

    pub(crate) fn close(&mut self) {
        self.accessible = None;
        self.window.close();
    }
}

/// The command menus. Linux has no global menu bar: each window draws a
/// menu bar, and a chosen command waits in the queue of `menu` until the
/// application takes it.
pub(crate) struct NativeMenu {
    menu: MainMenu,
}

impl NativeMenu {
    /// The commands that the user chose since the last call.
    pub(crate) fn take_activations(&self) -> Vec<MenuCommandId> {
        self.menu.take_activations()
    }

    /// Chooses `command` as a click does: an enabled item queues its
    /// activation. Returns false for a disabled or unknown item.
    pub(crate) fn perform(&self, command: MenuCommandId) -> bool {
        self.menu.perform_action(command).unwrap_or(false)
    }
}
