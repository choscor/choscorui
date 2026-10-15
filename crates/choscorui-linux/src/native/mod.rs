//! The native backend: windows, input, and the event loop on Wayland
//! (and X11). It compiles only for Linux.
//!
//! One UI thread owns everything. `Application::dispatch_next_event`
//! paints the windows that are ready, sends the requests, and polls the
//! display socket, the eventfd of `Poster`, and the timerfd of the
//! earliest timer. It then handles what arrived.

mod event_loop;
mod keyboard;
mod paint;
mod pointer;
mod types;
mod wayland;
mod window;

pub use event_loop::{PostError, Poster, Timer};
pub use types::{
    Error, ImeEvent, Modifiers, PaintTiming, Painter, ResizeEdge, TextInputState, WindowEvent,
};
pub use window::Window;

use crate::ffi::libc::Watch;
use crate::{BackendKind, BackendSelectionError, select_backend};
use choscorui_core::{BackendCapabilities, KeyConvention, Rect, Size, Support, WindowOptions};
use event_loop::{Posted, Timers};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use window::WindowShared;

/// The display server that `CHOSCORUI_BACKEND` requests, if any.
fn requested_backend() -> Option<BackendKind> {
    match std::env::var("CHOSCORUI_BACKEND").ok()?.as_str() {
        "wayland" => Some(BackendKind::Wayland),
        "x11" => Some(BackendKind::X11),
        _ => None,
    }
}

fn selection_error(error: BackendSelectionError) -> Error {
    Error::NoDisplay(match error {
        BackendSelectionError::NoDisplayServer => {
            "neither WAYLAND_DISPLAY nor DISPLAY is set".to_owned()
        }
        BackendSelectionError::RequestedUnavailable(BackendKind::Wayland) => {
            "CHOSCORUI_BACKEND=wayland, but WAYLAND_DISPLAY is not set".to_owned()
        }
        BackendSelectionError::RequestedUnavailable(BackendKind::X11) => {
            "CHOSCORUI_BACKEND=x11, but DISPLAY is not set".to_owned()
        }
    })
}

/// The connected display server.
enum Display {
    Wayland(Rc<wayland::Client>),
}

impl Display {
    /// Connects to the display server that the environment selects. When
    /// Wayland is selected without an explicit request and its connection
    /// fails, it falls back to X11.
    fn connect() -> Result<Self, Error> {
        let requested = requested_backend();
        let wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let x11 = std::env::var("DISPLAY").ok();
        let kind = select_backend(requested, wayland.as_deref(), x11.as_deref())
            .map_err(selection_error)?;
        match kind {
            BackendKind::Wayland => match wayland::Client::connect() {
                Ok(client) => Ok(Self::Wayland(client)),
                Err(error)
                    if requested.is_none() && x11.as_deref().is_some_and(|x| !x.is_empty()) =>
                {
                    Self::connect_x11().map_err(|_| error)
                }
                Err(error) => Err(error),
            },
            BackendKind::X11 => Self::connect_x11(),
        }
    }

    fn connect_x11() -> Result<Self, Error> {
        Err(Error::NoDisplay(
            "the X11 backend is not available".to_owned(),
        ))
    }

    fn kind(&self) -> BackendKind {
        match self {
            Self::Wayland(_) => BackendKind::Wayland,
        }
    }
}

/// The UI-thread application. It is neither `Send` nor `Sync`.
pub struct Application {
    display: Display,
    posted: Posted,
    timers: Timers,
    windows: RefCell<Vec<std::rc::Weak<WindowShared>>>,
    app_id: RefCell<String>,
    stopped: Cell<bool>,
    _ui_thread: PhantomData<Rc<()>>,
}

impl Application {
    /// Connects to the display server. Fails without one, when a required
    /// library does not load, or when the server lacks a protocol that
    /// windows need.
    pub fn new() -> Result<Self, Error> {
        let display = Display::connect()?;
        let io = |error: std::io::Error| Error::Connection(error.to_string());
        Ok(Self {
            display,
            posted: Posted::new().map_err(io)?,
            timers: Timers::new().map_err(io)?,
            windows: RefCell::new(Vec::new()),
            app_id: RefCell::new(String::new()),
            stopped: Cell::new(false),
            _ui_thread: PhantomData,
        })
    }

    /// The display server in use.
    pub fn backend(&self) -> BackendKind {
        self.display.kind()
    }

    /// Sets the application id: the Wayland `app_id` and the X11
    /// `WM_CLASS` of the windows that open later.
    pub fn set_app_id(&self, id: &str) {
        *self.app_id.borrow_mut() = id.to_owned();
    }

    /// The painter of new windows: Cairo when `CHOSCORUI_PAINTER=cairo`
    /// is set or the GPU painter is not available.
    fn painter() -> Painter {
        Painter::Cairo
    }

    /// Creates a hidden window.
    pub fn create_window(&self, options: WindowOptions) -> Result<Window, Error> {
        let (width, height) = (options.width(), options.height());
        if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
            return Err(Error::InvalidSize);
        }
        if options.title().contains('\0') {
            return Err(Error::InvalidTitle);
        }
        let shared = WindowShared::new(
            options.title(),
            Size::new(width, height),
            options.minimum(),
            Self::painter(),
        );
        let surface: Rc<dyn window::Surface> = match &self.display {
            Display::Wayland(client) => {
                client.create_window(Rc::clone(&shared), &self.app_id.borrow())?
            }
        };
        let mut windows = self.windows.borrow_mut();
        windows.retain(|window| window.upgrade().is_some_and(|window| !window.closed.get()));
        windows.push(Rc::downgrade(&shared));
        Ok(Window { shared, surface })
    }

    /// The number of open native windows.
    pub fn window_count(&self) -> usize {
        match &self.display {
            Display::Wayland(client) => client.window_count(),
        }
    }

    /// The sender of work to the UI thread.
    pub fn poster(&self) -> Poster {
        self.posted.poster()
    }

    /// Calls `action` once after `delay`.
    pub fn schedule_once(
        &self,
        delay: Duration,
        action: impl FnOnce() + 'static,
    ) -> Result<Timer, Error> {
        Ok(self.timers.once(delay, action))
    }

    /// Calls `action` every `interval` until the timer is canceled or
    /// dropped.
    pub fn schedule_repeating(
        &self,
        interval: Duration,
        action: impl FnMut() + 'static,
    ) -> Result<Timer, Error> {
        if interval.is_zero() {
            return Err(Error::InvalidInterval);
        }
        Ok(self.timers.repeating(interval, action))
    }

    /// The number of posted closures that panicked.
    pub fn posted_panic_count(&self) -> usize {
        self.posted.panics()
    }

    /// The number of timer callbacks that panicked.
    pub fn timer_panic_count(&self) -> usize {
        self.timers.panics()
    }

    /// Paints the windows that are ready, waits up to `timeout` for an
    /// event, and handles what arrived. Returns true when something was
    /// handled. Fails when the display connection is lost; the windows
    /// then have a `Closing` event.
    pub fn dispatch_next_event(&self, timeout: Duration) -> Result<bool, Error> {
        let now = Instant::now();
        let timeout = match self.timers.next_deadline() {
            Some(deadline) => timeout.min(deadline.saturating_duration_since(now)),
            None => timeout,
        };
        let extra = [
            Watch {
                fd: std::os::fd::AsFd::as_fd(self.posted.wake()),
                read: true,
                write: false,
            },
            Watch {
                fd: std::os::fd::AsFd::as_fd(self.timers.fd()),
                read: true,
                write: false,
            },
        ];
        let ready = match &self.display {
            Display::Wayland(client) => client.wait(&extra, Some(timeout))?,
        };
        let mut handled = ready.iter().any(|ready| ready.read);
        handled |= self.posted.run();
        if self
            .timers
            .next_deadline()
            .is_some_and(|deadline| deadline <= Instant::now())
            || ready.get(1).is_some_and(|ready| ready.read)
        {
            handled |= self.timers.run_due();
        }
        if self.posted.take_stop() {
            self.stopped.set(true);
        }
        match &self.display {
            Display::Wayland(client) => {
                client.paint();
                client.flush();
            }
        }
        Ok(handled)
    }

    /// Handles events until `stop` or `Poster::request_stop`, or until
    /// the display connection is lost.
    pub fn run(&self) -> Result<(), Error> {
        self.stopped.set(false);
        while !self.stopped.get() {
            self.dispatch_next_event(Duration::from_secs(3600))?;
        }
        self.stopped.set(false);
        Ok(())
    }

    /// Makes `run` return after the current event.
    pub fn stop(&self) {
        self.stopped.set(true);
        self.posted.stop();
    }

    /// The logical rectangles of the screens.
    pub fn screens(&self) -> Vec<Rect> {
        match &self.display {
            Display::Wayland(client) => client.screens(),
        }
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.timers.cancel_all();
    }
}

/// What the backend offers on the running system. The result is probed
/// once per process: it connects to the display server without opening
/// a window.
pub fn capabilities() -> BackendCapabilities {
    static PROBED: OnceLock<BackendCapabilities> = OnceLock::new();
    *PROBED.get_or_init(probe)
}

fn probe() -> BackendCapabilities {
    let display = match Display::connect() {
        Ok(display) => display,
        // The server is there, but it cannot show windows.
        Err(Error::MissingProtocol(_)) => return BackendCapabilities::none("wayland"),
        Err(_) => return BackendCapabilities::none("headless"),
    };
    let mut capabilities = BackendCapabilities::none(match display.kind() {
        BackendKind::Wayland => "wayland",
        BackendKind::X11 => "x11",
    });
    capabilities.multiple_windows = Support::Supported;
    capabilities.keyboard = KeyConvention::Pc;
    capabilities
}
