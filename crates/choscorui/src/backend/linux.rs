//! The Wayland and X11 backend: `choscorui-linux` behind the portable
//! interface.

mod chrome;
mod context_menu;

use crate::{AppContext, AppError, WindowId};
use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, ViewHost};
use choscorui_controls::{EstimatedText, ShapedText, ShortcutModifiers, TextMeasure};
use choscorui_core::{
    AnnouncementPriority, BackendCapabilities, CompositionEvent, CursorShape, DisplayList,
    Feedback, MenuBarModel, MenuCommandId, MenuError, NoFeedback, Point, Rect, SemanticChange,
    SemanticEntry, Services, Size, Support, WindowGeometry, WindowOptions,
};
use choscorui_linux::{
    Application, ContextMenu, Error, ImeEvent, LinuxTextShaper, MainMenu, Modifiers,
    TextInputState, Window, WindowEvent,
};
use chrome::{Chrome, Menus};
use std::rc::Rc;
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

/// What the backend provides on the running system.
pub(crate) fn capabilities() -> BackendCapabilities {
    choscorui_linux::capabilities()
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

/// The Wayland or X11 application, and the menus that the windows draw.
pub(crate) struct Platform {
    application: Application,
    menus: Menus,
}

impl Platform {
    pub(crate) fn new(id: Option<&str>) -> Result<Self, AppError> {
        let application = Application::new().map_err(app_error)?;
        if let Some(id) = id {
            application.set_app_id(id);
        }
        Ok(Self {
            application,
            menus: Menus::default(),
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
        Services::none()
    }

    pub(crate) fn open_window(&self, options: &WindowOptions) -> Result<NativeWindow, AppError> {
        let window = self
            .application
            .create_window(options.clone())
            .map_err(app_error)?;
        Ok(NativeWindow {
            window,
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

/// A Wayland or X11 window and the chrome that the toolkit draws in it.
pub(crate) struct NativeWindow {
    window: Window,
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
        if let Some(chrome) = &self.chrome {
            closing |= chrome.sync(&self.window);
        }
        let input = host.text_input().unwrap_or_default();
        self.window.set_text_input_state(TextInputState {
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
