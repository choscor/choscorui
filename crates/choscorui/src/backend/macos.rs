//! The AppKit backend: `choscorui-macos` behind the portable interface.

use crate::{AppContext, AppError, WindowId};
use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, ViewHost};
use choscorui_controls::{ShapedText, ShortcutModifiers, TextMeasure};
use choscorui_core::{
    AnnouncementPriority, BackendCapabilities, CompositionEvent, CursorShape, DisplayList,
    Feedback, MenuBarModel, MenuCommandId, MenuError, Rect, SemanticChange, SemanticEntry,
    Services, Size, Support, WindowGeometry, WindowOptions,
};
use choscorui_macos::{
    Application, CoreTextShaper, Error, ImeEvent, MainMenu, Modifiers, NativeServices,
    SystemFeedback, TextInputState, Window, WindowEvent,
};
use std::rc::Rc;
use std::time::Duration;

static SHAPER: CoreTextShaper = CoreTextShaper;

/// The backend for macOS: `choscorui-macos` in full, and native access
/// to the windows of an [`AppContext`]. Code that uses it is not
/// portable; prefer the services and capabilities of the prelude.
pub mod platform {
    pub use super::NativeAccess;
    pub use choscorui_macos::*;
}

/// Native access to a running application, for verification harnesses
/// and features that the portable layer does not cover.
pub trait NativeAccess {
    /// The AppKit application.
    fn native_application(&self) -> &Application;
    /// The AppKit window of `id`, for its paint timing or geometry.
    fn native_window(&mut self, id: WindowId) -> Option<&mut Window>;
    /// The installed AppKit menu bar of the command menus.
    fn native_menu(&self) -> Option<&MainMenu>;
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
}

/// What the AppKit backend provides on the running system.
pub(crate) fn capabilities() -> BackendCapabilities {
    choscorui_macos::capabilities()
}

/// The `multiple_windows` capability of [`capabilities`], without its
/// GPU probe, so that `App::start` does not wait for the GPU: the probe
/// then overlaps the start of the application.
pub(crate) fn window_support() -> Support {
    Support::Supported
}

fn app_error(error: Error) -> AppError {
    match error {
        Error::WrongThread => AppError::WrongThread,
        Error::InvalidSize | Error::InvalidTitle => AppError::InvalidWindowOptions,
        _ => AppError::Unavailable,
    }
}

/// The AppKit application.
pub(crate) struct Platform {
    application: Application,
}

impl Platform {
    pub(crate) fn new(_id: Option<&str>) -> Result<Self, AppError> {
        Ok(Self {
            application: Application::new().map_err(app_error)?,
        })
    }

    /// The text measurer of the windows.
    /// The application state that drawn dialogs use. AppKit shows its own dialogs, so it keeps nothing.
    pub(crate) fn attach(
        &self,
        _runtime: &choscorui_compose::Runtime,
        _env: &choscorui_compose::view::Env,
        _tokens: &choscorui_compose::view::Live<choscorui_core::DesignTokens>,
    ) {
    }

    pub(crate) fn text(&self) -> Rc<dyn TextMeasure> {
        Rc::new(ShapedText::new(&SHAPER))
    }

    /// The alert sound of the controls: the system beep.
    pub(crate) fn feedback(&self) -> Rc<dyn Feedback> {
        Rc::new(SystemFeedback)
    }

    /// The desktop services.
    pub(crate) fn services(&self) -> Services {
        Services::new(Rc::new(NativeServices::default()))
    }

    pub(crate) fn open_window(&self, options: &WindowOptions) -> Result<NativeWindow, AppError> {
        let window = self
            .application
            .create_window(options.clone())
            .map_err(app_error)?;
        Ok(NativeWindow { window })
    }

    /// The system draws the title bar and the menu bar, so the content
    /// is the whole window.
    pub(crate) fn frame(
        &self,
        _native: &mut NativeWindow,
        content: AnyView,
        _runtime: &Runtime,
    ) -> AnyView {
        content
    }

    pub(crate) fn install_menu(&self, model: MenuBarModel) -> Result<NativeMenu, MenuError> {
        Ok(NativeMenu {
            menu: self.application.install_main_menu(model)?,
        })
    }

    /// Waits up to `timeout` for a native event and handles it.
    pub(crate) fn wait(&self, timeout: Duration) {
        self.application.dispatch_next_event(timeout);
    }

    /// Brings the application to the front.
    pub(crate) fn activate(&self) {
        self.application.activate();
    }

    /// The visible frames of the screens, main screen first.
    pub(crate) fn screens(&self) -> Vec<Rect> {
        choscorui_macos::screen_visible_frames().unwrap_or_default()
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

/// An AppKit window.
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

    /// Routes the window's events and accessibility requests to `host`.
    /// Returns whether the window asked to close, and its new focus
    /// state, if it changed.
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
                // A Command-key combination arrives here only when no menu
                // item took it, or while the focused control captures keys.
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
        for request in self.window.take_accessibility_actions() {
            match request.part {
                Some(part) => host.perform_part(request.node, part, request.action),
                None => host.perform(request.node, request.action, request.value.as_deref()),
            };
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

    /// Shows `shape` over the content. A shape that AppKit cannot show
    /// keeps the cursor that shows.
    pub(crate) fn set_cursor(&self, shape: CursorShape) {
        let _ = self.window.set_cursor(shape);
    }

    /// Asks VoiceOver to speak `text`. A failure only loses the speech.
    pub(crate) fn announce(&self, text: &str, priority: AnnouncementPriority) {
        if let Err(error) = self.window.announce(text, priority) {
            eprintln!("choscorui: announcement failed: {error}");
        }
    }

    pub(crate) fn publish(
        &self,
        snapshot: &[SemanticEntry],
        changes: &[SemanticChange],
    ) -> Result<(), String> {
        self.window
            .set_accessibility(snapshot, changes)
            .map_err(|error| error.to_string())
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

/// The installed AppKit menu bar.
pub(crate) struct NativeMenu {
    menu: MainMenu,
}

impl NativeMenu {
    /// The commands that the user chose since the last call.
    pub(crate) fn take_activations(&self) -> Vec<MenuCommandId> {
        self.menu.take_activations()
    }

    /// Chooses `command` through the native action path.
    pub(crate) fn perform(&self, command: MenuCommandId) -> bool {
        self.menu.perform_action(command).unwrap_or(false)
    }
}
