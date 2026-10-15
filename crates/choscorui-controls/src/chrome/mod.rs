//! Client-side window decorations: the title bar control, the requests
//! that it gives to the backend, and the geometry of the window frame.
//! A backend without server-side decorations shows them; the facade
//! composes them, so no platform code is here.

mod frame;
mod title_bar;

pub use frame::{ResizeEdge, WindowFrame, WindowRegion};
pub use title_bar::TitleBar;

use choscorui_core::Point;
use std::cell::RefCell;
use std::rc::Rc;

/// A window button of a [`TitleBar`], in logical order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TitleBarButton {
    Minimize,
    /// Maximizes the window, or restores a maximized window.
    Maximize,
    Close,
}

impl TitleBarButton {
    /// Every button, in logical order.
    pub const ALL: [TitleBarButton; 3] = [Self::Minimize, Self::Maximize, Self::Close];

    /// The request that the button makes when it activates.
    pub fn request(self) -> TitleBarRequest {
        match self {
            Self::Minimize => TitleBarRequest::Minimize,
            Self::Maximize => TitleBarRequest::ToggleMaximize,
            Self::Close => TitleBarRequest::Close,
        }
    }

    /// The id of the built-in message that names the button. The
    /// maximize button of a maximized window is the restore button.
    pub fn message_id(self, maximized: bool) -> &'static str {
        match self {
            Self::Minimize => "choscorui.titlebar.minimize",
            Self::Maximize if maximized => "choscorui.titlebar.restore",
            Self::Maximize => "choscorui.titlebar.maximize",
            Self::Close => "choscorui.titlebar.close",
        }
    }
}

/// What a title bar asks the backend to do with its window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TitleBarRequest {
    /// Start an interactive move with the pointer, for example
    /// `xdg_toplevel.move`.
    Move,
    /// Start an interactive resize from `edge`, for example
    /// `xdg_toplevel.resize`.
    Resize(ResizeEdge),
    Minimize,
    /// Maximize the window, or restore it when it is maximized.
    ToggleMaximize,
    Close,
    /// Show the window menu of the system at this point in window
    /// coordinates, for example `xdg_toplevel.show_window_menu`.
    ShowWindowMenu(Point),
}

/// The state that a window shares with its [`TitleBar`]: the title, the
/// maximized and active flags, and the queue of requests. Clones share
/// one state. A change shows at the next
/// [`ControlHost::advance`](crate::ControlHost::advance).
#[derive(Clone, Debug)]
pub struct TitleBarHandle(Rc<RefCell<Shared>>);

#[derive(Debug)]
struct Shared {
    title: String,
    maximized: bool,
    active: bool,
    /// True when the window changed the state since the title bar last
    /// showed it.
    changed: bool,
    requests: Vec<TitleBarRequest>,
}

impl Default for TitleBarHandle {
    /// An active window that is not maximized, with no title.
    fn default() -> Self {
        Self(Rc::new(RefCell::new(Shared {
            title: String::new(),
            maximized: false,
            active: true,
            changed: false,
            requests: Vec::new(),
        })))
    }
}

impl TitleBarHandle {
    /// The state of an active window that is not maximized.
    pub fn new() -> Self {
        Self::default()
    }

    /// The window title.
    pub fn title(&self) -> String {
        self.0.borrow().title.clone()
    }

    /// Shows `title` in the title bar.
    pub fn set_title(&self, title: impl Into<String>) {
        let mut shared = self.0.borrow_mut();
        let title = title.into();
        if shared.title != title {
            shared.title = title;
            shared.changed = true;
        }
    }

    /// True when the window is maximized.
    pub fn is_maximized(&self) -> bool {
        self.0.borrow().maximized
    }

    /// Records whether the window is maximized. The maximize button then
    /// restores, and the frame loses its corners and its shadow.
    pub fn set_maximized(&self, maximized: bool) {
        let mut shared = self.0.borrow_mut();
        if shared.maximized != maximized {
            shared.maximized = maximized;
            shared.changed = true;
        }
    }

    /// True when the window is the active window.
    pub fn is_active(&self) -> bool {
        self.0.borrow().active
    }

    /// Records whether the window is the active window. An inactive
    /// window shows a muted title and a smaller shadow.
    pub fn set_active(&self, active: bool) {
        let mut shared = self.0.borrow_mut();
        if shared.active != active {
            shared.active = active;
            shared.changed = true;
        }
    }

    /// The requests since the last call, oldest first. The backend acts
    /// on each one.
    pub fn take_requests(&self) -> Vec<TitleBarRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

    fn push(&self, request: TitleBarRequest) {
        self.0.borrow_mut().requests.push(request);
    }

    /// Clears the change flag. Returns true when the state changed.
    fn take_changed(&self) -> bool {
        std::mem::take(&mut self.0.borrow_mut().changed)
    }

    fn changed(&self) -> bool {
        self.0.borrow().changed
    }
}
