//! The target has no backend. The headless platform has no windows on
//! screen and no event loop, so `Platform::new` returns
//! `AppError::Unsupported`. Its windows and menus keep their state in
//! memory and show nothing.

use crate::AppError;
use choscorui_compose::view::ViewHost;
use choscorui_controls::{EstimatedText, TextMeasure};
use choscorui_core::{
    BackendCapabilities, CursorShape, DisplayList, Feedback, MenuBarModel, MenuCommandId,
    MenuError, NoFeedback, Point, Rect, SemanticChange, SemanticEntry, Services, Size, Support,
    WindowGeometry, WindowOptions,
};
use std::rc::Rc;
use std::time::Duration;

/// The backend for the current operating system. This target has no
/// backend, so the module is empty.
pub mod platform {}

/// Without a backend, every capability is unsupported.
pub(crate) fn capabilities() -> BackendCapabilities {
    BackendCapabilities::none("headless")
}

/// The headless platform.
pub(crate) struct Platform;

impl Platform {
    /// Refuses to start: the headless platform cannot show windows.
    pub(crate) fn new() -> Result<Self, AppError> {
        let platform = Self;
        match platform.capabilities().multiple_windows {
            Support::Supported => Ok(platform),
            _ => Err(AppError::Unsupported),
        }
    }

    fn capabilities(&self) -> BackendCapabilities {
        capabilities()
    }

    pub(crate) fn text(&self) -> Rc<dyn TextMeasure> {
        Rc::new(EstimatedText)
    }

    /// No alert sound.
    pub(crate) fn feedback(&self) -> Rc<dyn Feedback> {
        Rc::new(NoFeedback)
    }

    pub(crate) fn services(&self) -> Services {
        Services::none()
    }

    pub(crate) fn open_window(&self, options: &WindowOptions) -> Result<NativeWindow, AppError> {
        Ok(NativeWindow {
            title: options.title().to_owned(),
            frame: Rect::new(
                Point::new(0.0, 0.0),
                Size::new(options.width(), options.height()),
            ),
            cursor: CursorShape::Arrow,
        })
    }

    pub(crate) fn install_menu(&self, model: MenuBarModel) -> Result<NativeMenu, MenuError> {
        Ok(NativeMenu { model })
    }

    pub(crate) fn wait(&self, timeout: Duration) {
        std::thread::sleep(timeout);
    }

    pub(crate) fn activate(&self) {}

    pub(crate) fn screens(&self) -> Vec<Rect> {
        Vec::new()
    }
}

/// A window that keeps its title, frame, and cursor in memory.
pub(crate) struct NativeWindow {
    title: String,
    frame: Rect,
    cursor: CursorShape,
}

impl NativeWindow {
    pub(crate) fn content_size(&self) -> Size {
        self.frame.size
    }

    pub(crate) fn show(&mut self) {}

    pub(crate) fn deliver(&mut self, _host: &mut ViewHost) -> (bool, Option<bool>) {
        (false, None)
    }

    pub(crate) fn title(&self) -> String {
        self.title.clone()
    }

    pub(crate) fn set_title(&mut self, title: &str) {
        title.clone_into(&mut self.title);
    }

    pub(crate) fn present(&self, _list: DisplayList) {}

    pub(crate) fn set_cursor(&mut self, shape: CursorShape) {
        self.cursor = shape;
    }

    pub(crate) fn publish(
        &self,
        _snapshot: &[SemanticEntry],
        _changes: &[SemanticChange],
    ) -> Result<(), String> {
        Ok(())
    }

    pub(crate) fn geometry(&self) -> WindowGeometry {
        WindowGeometry {
            frame: self.frame,
            zoomed: false,
            full_screen: false,
        }
    }

    pub(crate) fn set_geometry(&mut self, geometry: &WindowGeometry) {
        self.frame = geometry.frame;
    }

    pub(crate) fn close(&mut self) {}
}

/// A menu bar that keeps its model and takes no input.
pub(crate) struct NativeMenu {
    model: MenuBarModel,
}

impl NativeMenu {
    pub(crate) fn take_activations(&self) -> Vec<MenuCommandId> {
        Vec::new()
    }

    pub(crate) fn perform(&self, command: MenuCommandId) -> bool {
        self.model.state(command).is_ok_and(|state| state.enabled)
    }
}
