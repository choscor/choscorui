//! The native backend: windows, input, and the event loop on Wayland
//! and X11. It compiles only for Linux.
//!
//! One UI thread owns everything. `Application::dispatch_next_event`
//! paints the windows that are ready, sends the requests, and polls the
//! display socket, the eventfd of `Poster`, and the timerfd of the
//! earliest timer. It then handles what arrived.

mod clipboard;
mod event_loop;
mod keyboard;
mod menu;
mod paint;
mod pointer;
mod transfer;
mod types;
mod wayland;
mod window;
mod x11;

pub use clipboard::SystemClipboard;
pub use event_loop::{PostError, Poster, Timer};
pub use menu::{ContextMenu, MainMenu};
pub use types::{
    Error, ImeEvent, Modifiers, PaintTiming, Painter, ResizeEdge, TextInputState, WindowEvent,
};
pub use window::Window;
pub(crate) use window::{Surface, WindowShared};

use crate::ffi::libc::Watch;
use crate::{BackendKind, BackendSelectionError, select_backend};
use choscorui_core::{
    BackendCapabilities, KeyConvention, MenuBarModel, MenuError, Rect, Size, Support, WindowOptions,
};
use event_loop::{Posted, Timers};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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
#[derive(Clone)]
enum Display {
    Wayland(Rc<wayland::Client>),
    X11(Rc<x11::Client>),
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
        x11::Client::connect().map(Self::X11)
    }

    fn kind(&self) -> BackendKind {
        match self {
            Self::Wayland(_) => BackendKind::Wayland,
            Self::X11(_) => BackendKind::X11,
        }
    }

    /// True when the server has a clipboard and drag and drop.
    fn has_data_transfer(&self) -> bool {
        match self {
            Self::Wayland(client) => client.has_data_device(),
            Self::X11(_) => true,
        }
    }

    /// True when the server has an input method protocol.
    fn has_input_methods(&self) -> bool {
        match self {
            Self::Wayland(client) => client.has_text_input(),
            Self::X11(client) => client.has_input_method(),
        }
    }

    fn set_selection(&self, payload: transfer::Payload) -> Result<(), Error> {
        match self {
            Self::Wayland(client) => client.set_selection(payload),
            Self::X11(client) => client.set_selection(payload),
        }
    }

    fn selection_formats(&self) -> Vec<choscorui_core::DragFormat> {
        match self {
            Self::Wayland(client) => match client.selection() {
                wayland::Contents::Empty => Vec::new(),
                wayland::Contents::Local(payload) => payload.formats(),
                wayland::Contents::Remote(id, mimes) => {
                    transfer::formats(&mimes, &mut |mime| client.receive(id, mime))
                }
            },
            Self::X11(client) => match client.selection() {
                x11::Contents::Empty => Vec::new(),
                x11::Contents::Local(payload) => payload.formats(),
                x11::Contents::Remote(mimes) => {
                    transfer::formats(&mimes, &mut |mime| client.receive(mime))
                }
            },
        }
    }

    fn read_selection(
        &self,
        formats: &[choscorui_core::DragFormat],
    ) -> Vec<choscorui_core::DragItem> {
        match self {
            Self::Wayland(client) => match client.selection() {
                wayland::Contents::Empty => Vec::new(),
                wayland::Contents::Local(payload) => payload.read(formats),
                wayland::Contents::Remote(id, mimes) => {
                    transfer::read(formats, &mimes, &mut |mime| client.receive(id, mime))
                }
            },
            Self::X11(client) => match client.selection() {
                x11::Contents::Empty => Vec::new(),
                x11::Contents::Local(payload) => payload.read(formats),
                x11::Contents::Remote(mimes) => {
                    transfer::read(formats, &mimes, &mut |mime| client.receive(mime))
                }
            },
        }
    }

    /// The painter of new windows: GLES when its device opened and no
    /// GPU failure happened, else Cairo.
    fn painter(&self) -> Painter {
        let gles = match self {
            Self::Wayland(client) => client.gles().is_some(),
            Self::X11(client) => client.gles().is_some(),
        };
        if gles { Painter::Gles } else { Painter::Cairo }
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
    /// The D-Bus connections that the loop serves too: the session bus of
    /// the desktop services and the accessibility bus.
    buses: RefCell<Vec<crate::dbus::Connection>>,
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
            buses: RefCell::new(Vec::new()),
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
            self.display.painter(),
        );
        let surface: Rc<dyn window::Surface> = match &self.display {
            Display::Wayland(client) => {
                client.create_window(Rc::clone(&shared), &self.app_id.borrow())?
            }
            Display::X11(client) => {
                client.create_window(Rc::clone(&shared), &self.app_id.borrow())?
            }
        };
        let mut windows = self.windows.borrow_mut();
        windows.retain(|window| window.upgrade().is_some_and(|window| !window.closed.get()));
        windows.push(Rc::downgrade(&shared));
        Ok(Window { shared, surface })
    }

    /// Installs the menus of the menu bar. Linux has no global menu bar:
    /// the application draws `MainMenu` in its windows, and a chosen
    /// command waits in its queue.
    pub fn install_main_menu(&self, model: MenuBarModel) -> Result<MainMenu, MenuError> {
        Ok(MainMenu::new(model))
    }

    /// The number of open native windows, without the popups.
    pub fn window_count(&self) -> usize {
        match &self.display {
            Display::Wayland(client) => client.window_count(),
            Display::X11(client) => client.window_count(),
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

    /// Plays the alert sound: the bell of the display server, else the
    /// event sound `bell` of libcanberra, else nothing.
    pub fn beep(&self) {
        let rung = match &self.display {
            Display::Wayland(client) => client.ring_bell(),
            Display::X11(client) => client.ring_bell(),
        };
        if !rung {
            crate::ffi::canberra::play_bell();
        }
    }

    /// True when a window of the application has the keyboard focus.
    pub fn is_active(&self) -> bool {
        self.live_windows().any(|window| window.focused.get())
    }

    /// True when a window of the application shows.
    pub fn has_visible_windows(&self) -> bool {
        self.live_windows().any(|window| window.visible.get())
    }

    /// The windows that are open.
    fn live_windows(&self) -> impl Iterator<Item = Rc<WindowShared>> {
        self.windows
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|window| !window.closed.get())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// The handle of the focused window for the parent of portal dialogs,
    /// such as `wayland:HANDLE` or `x11:XID`.
    pub fn focused_window_handle(&self) -> Option<String> {
        self.windows
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .find(|window| window.focused.get() && !window.closed.get())
            .and_then(|window| window.parent_handle.borrow().clone())
    }

    /// Makes `window` the modal window of the application until the
    /// session drops. The other windows drop their input events but
    /// still paint. Unless `Window::set_modal_parent` gave it a parent,
    /// `window` becomes the modal dialog of the focused window, or else
    /// of the newest visible window.
    pub fn begin_modal(&self, window: &Window) -> ModalSession {
        let others: Vec<Rc<WindowShared>> = self
            .windows
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|other| !Rc::ptr_eq(other, &window.shared) && !other.closed.get())
            .collect();
        if !window.shared.has_parent.get() {
            let parent = others
                .iter()
                .find(|other| other.focused.get())
                .or_else(|| others.iter().rev().find(|other| other.visible.get()));
            if let Some(parent) = parent {
                window.surface.set_modal_parent(parent);
                window.shared.has_parent.set(true);
            }
        }
        let blocked = others
            .iter()
            .map(|other| (Rc::downgrade(other), other.blocked.replace(true)))
            .collect();
        ModalSession { blocked }
    }

    /// Makes the event loop serve `connection` too: it waits on the
    /// descriptors of the connection and dispatches its messages. The
    /// loop serves each attached connection until it is lost.
    pub fn attach_bus(&self, connection: crate::dbus::Connection) {
        self.buses.borrow_mut().push(connection);
    }

    /// Paints the windows that are ready, waits up to `timeout` for an
    /// event, and handles what arrived. Returns true when something was
    /// handled. Fails when the display connection is lost; the windows
    /// then have a `Closing` event.
    pub fn dispatch_next_event(&self, timeout: Duration) -> Result<bool, Error> {
        let now = Instant::now();
        let mut timeout = match self.timers.next_deadline() {
            Some(deadline) => timeout.min(deadline.saturating_duration_since(now)),
            None => timeout,
        };
        let buses = self.buses.borrow().clone();
        let bus_watches: Vec<Vec<crate::dbus::WatchFd>> =
            buses.iter().map(crate::dbus::Connection::watches).collect();
        for own in buses
            .iter()
            .filter_map(crate::dbus::Connection::next_timeout)
        {
            timeout = timeout.min(own);
        }
        let mut extra = vec![
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
        extra.extend(bus_watches.iter().flatten().map(|watch| Watch {
            fd: std::os::fd::AsFd::as_fd(watch),
            read: watch.read(),
            write: watch.write(),
        }));
        let ready = match &self.display {
            Display::Wayland(client) => client.wait(&extra, Some(timeout))?,
            Display::X11(client) => client.wait(&extra, Some(timeout))?,
        };
        drop(extra);
        let mut handled = ready.iter().any(|ready| ready.read);
        let mut seen = ready.iter().skip(2);
        let mut lost = Vec::new();
        for (index, (bus, watches)) in buses.iter().zip(&bus_watches).enumerate() {
            for (watch, ready) in watches.iter().zip(seen.by_ref()) {
                bus.handle(
                    watch,
                    crate::dbus::Readiness {
                        read: ready.read,
                        write: ready.write,
                        hangup: ready.hangup,
                    },
                );
            }
            // A lost bus ends its services, not the application.
            if bus.dispatch().is_err() {
                lost.push(index);
            }
        }
        if !lost.is_empty() {
            let mut index = 0;
            self.buses.borrow_mut().retain(|_| {
                let keep = !lost.contains(&index);
                index += 1;
                keep
            });
        }
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
            Display::X11(client) => {
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

    /// The clipboard of the display server.
    pub fn clipboard(&self) -> SystemClipboard {
        SystemClipboard::general(self.display.clone())
    }

    /// The logical rectangles of the screens.
    pub fn screens(&self) -> Vec<Rect> {
        match &self.display {
            Display::Wayland(client) => client.screens(),
            Display::X11(client) => client.screens(),
        }
    }
}

/// A modal window of the application, from `Application::begin_modal`.
/// The drop gives the other windows their input again.
#[must_use = "the other windows take input again when the session drops"]
pub struct ModalSession {
    blocked: Vec<(std::rc::Weak<WindowShared>, bool)>,
}

impl Drop for ModalSession {
    fn drop(&mut self) {
        for (window, was_blocked) in &self.blocked {
            if let Some(window) = window.upgrade() {
                window.blocked.set(*was_blocked);
            }
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
    let mut capabilities = *PROBED.get_or_init(probe);
    // A GPU failure after the probe switches every window to Cairo.
    if crate::gles::failed() {
        capabilities.gpu_painting = Support::Unsupported;
    }
    capabilities
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
    capabilities.text_shaping = crate::text_shaping_support();
    capabilities.image_codecs = crate::image_codecs_support();
    capabilities.keyboard = KeyConvention::Pc;
    if display.has_input_methods() {
        capabilities.input_methods = Support::Supported;
    }
    // The application draws the menu bar in the window, and the context
    // menus in popups: `xdg_popup` on Wayland, and override-redirect
    // windows on X11.
    capabilities.native_menus = Support::Supported;
    capabilities.context_menus = Support::Supported;
    if display.has_data_transfer() {
        capabilities.clipboard = Support::Supported;
        capabilities.drag_and_drop = Support::Supported;
    }
    // The display opened the GLES painter and compiled its shaders,
    // unless `CHOSCORUI_PAINTER=cairo` is set.
    capabilities.gpu_painting = match display.painter() {
        Painter::Gles => Support::Supported,
        Painter::Cairo => Support::Unsupported,
    };
    capabilities
}
