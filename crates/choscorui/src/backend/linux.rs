//! The Wayland and X11 backend: `choscorui-linux` behind the portable
//! interface.

use crate::{AppContext, AppError, WindowId};
use choscorui_compose::view::ViewHost;
use choscorui_controls::{EstimatedText, ShapedText, ShortcutModifiers, TextMeasure};
use choscorui_core::{
    AnnouncementPriority, BackendCapabilities, CompositionEvent, CursorShape, DisplayList,
    Feedback, MenuBarModel, MenuCommandId, MenuError, NoFeedback, Rect, SemanticChange,
    SemanticEntry, ServiceError, Services, Size, Support, WindowGeometry, WindowOptions,
};
use choscorui_linux::services::{NativeServices, ServiceOptions, TextClipboard};
use choscorui_linux::{
    Application, Error, ImeEvent, LinuxTextShaper, Modifiers, SystemClipboard, TextInputState,
    Window, WindowEvent,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

static SHAPER: LinuxTextShaper = LinuxTextShaper;

/// The backend for Linux: `choscorui-linux` in full, and native access
/// to the windows of an [`AppContext`]. Code that uses it is not
/// portable; prefer the services and capabilities of the prelude.
pub mod platform {
    pub use super::NativeAccess;
    pub use choscorui_linux::*;
}

/// Native access to a running application, for verification harnesses
/// and features that the portable layer does not cover.
pub trait NativeAccess {
    /// The Wayland or X11 application.
    fn native_application(&self) -> &Application;
    /// The native window of `id`, for its paint count or geometry.
    fn native_window(&mut self, id: WindowId) -> Option<&mut Window>;
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
}

/// What the backend provides on the running system: the display server
/// features, and the desktop services on the session bus.
pub(crate) fn capabilities() -> BackendCapabilities {
    // The services are probed once, on a connection of their own.
    static SERVICES: OnceLock<BackendCapabilities> = OnceLock::new();
    let mut capabilities = choscorui_linux::capabilities();
    let services = *SERVICES.get_or_init(|| {
        let probe = NativeServices::new(ServiceOptions::new(""));
        probe.set_base_capabilities(BackendCapabilities::none("linux"));
        choscorui_core::ServiceBackend::capabilities(&probe)
    });
    capabilities.notifications = services.notifications;
    capabilities.status_items = services.status_items;
    capabilities.open_urls = services.open_urls;
    capabilities.system_appearance = services.system_appearance;
    capabilities.native_dialogs = services.native_dialogs;
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

/// The Wayland or X11 application and its desktop services.
pub(crate) struct Platform {
    application: Application,
    services: Rc<NativeServices>,
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

impl Platform {
    pub(crate) fn new(id: Option<&str>) -> Result<Self, AppError> {
        let application = Application::new().map_err(app_error)?;
        if let Some(id) = id {
            application.set_app_id(id);
        }
        let services = NativeServices::new(ServiceOptions::new(id.unwrap_or_default()));
        services.set_base_capabilities(capabilities());
        if capabilities().clipboard.is_supported() {
            services.set_clipboard(Some(Box::new(Selection(application.clipboard()))));
        }
        if let Some(bus) = services.connection() {
            application.attach_bus(bus.clone());
        }
        Ok(Self {
            application,
            services: Rc::new(services),
        })
    }

    /// The text measurer of the windows: HarfBuzz shaping, or estimated
    /// metrics when FreeType, HarfBuzz, or fontconfig does not load.
    pub(crate) fn text(&self) -> Rc<dyn TextMeasure> {
        if choscorui_linux::text_shaping_support().is_supported() {
            Rc::new(ShapedText::new(&SHAPER))
        } else {
            Rc::new(EstimatedText)
        }
    }

    /// The alert sound of the controls.
    pub(crate) fn feedback(&self) -> Rc<dyn Feedback> {
        Rc::new(NoFeedback)
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
        Ok(NativeWindow { window })
    }

    pub(crate) fn install_menu(&self, model: MenuBarModel) -> Result<NativeMenu, MenuError> {
        Ok(NativeMenu {
            model,
            activations: RefCell::new(Vec::new()),
        })
    }

    /// Waits up to `timeout` for a native event and handles it. A lost
    /// display connection gives each window a close request.
    pub(crate) fn wait(&self, timeout: Duration) {
        if let Err(error) = self.application.dispatch_next_event(timeout) {
            eprintln!("choscorui: {error}");
            std::thread::sleep(timeout);
        }
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

/// A Wayland or X11 window.
pub(crate) struct NativeWindow {
    window: Window,
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
        for event in self.window.take_events() {
            match event {
                WindowEvent::Closing => closing = true,
                WindowEvent::Focused(focused) => {
                    if !focused {
                        host.cancel_interaction();
                    }
                    focus = Some(focused);
                }
                WindowEvent::Resized { size, .. } => host.resize(size),
                WindowEvent::Pointer {
                    event,
                    modifiers,
                    details,
                } => {
                    host.pointer_with(event, shortcut_modifiers(modifiers), details);
                }
                WindowEvent::Key { event, modifiers } => {
                    host.key(event, shortcut_modifiers(modifiers));
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
        let _ = self.window.set_cursor(shape);
    }

    /// Without the accessibility bridge, nothing speaks.
    pub(crate) fn announce(&self, _text: &str, _priority: AnnouncementPriority) {}

    /// Without the accessibility bridge, the tree is not published.
    pub(crate) fn publish(
        &self,
        _snapshot: &[SemanticEntry],
        _changes: &[SemanticChange],
    ) -> Result<(), String> {
        Ok(())
    }

    pub(crate) fn geometry(&self) -> WindowGeometry {
        self.window.geometry()
    }

    pub(crate) fn set_geometry(&mut self, geometry: &WindowGeometry) {
        let _ = self.window.set_geometry(geometry);
    }

    pub(crate) fn close(&mut self) {
        self.window.close();
    }
}

/// The command menus. Linux has no global menu bar, so a chosen command
/// waits in `activations` until the application takes it.
pub(crate) struct NativeMenu {
    model: MenuBarModel,
    activations: RefCell<Vec<MenuCommandId>>,
}

impl NativeMenu {
    /// The commands that the user chose since the last call.
    pub(crate) fn take_activations(&self) -> Vec<MenuCommandId> {
        std::mem::take(&mut *self.activations.borrow_mut())
    }

    /// Chooses `command` as a click does: an enabled item queues its
    /// activation. Returns false for a disabled or unknown item.
    pub(crate) fn perform(&self, command: MenuCommandId) -> bool {
        let enabled = self.model.state(command).is_ok_and(|state| state.enabled);
        if enabled {
            self.activations.borrow_mut().push(command);
        }
        enabled
    }
}
