//! The Wayland display server.
//!
//! `Client` owns the connection, the globals, the seat, the outputs, and
//! the windows. Each event of the connection names its object by id, and
//! `routes` maps the id to the role of the object, so `handle` sends the
//! event to its seat, output, or window.

mod data;
mod shm;
mod window;

pub(crate) use data::Contents;
pub(crate) use window::WaylandWindow;

use super::keyboard::{Keyboard, RepeatInfo};
use super::pointer::{self, ClickCounter};
use super::types::{Error, Modifiers, WindowEvent};
use crate::ffi::libc::{self, Ready, Watch};
use crate::ffi::wayland::sys::Interface;
use crate::ffi::wayland::{self as wl, Arg, Connection, Event, ObjectId, Proxy};
use crate::{InputEvent, LinuxInput, WaylandConnection, WaylandGlobal};
use choscorui_core::{CursorShape, Point, PointerDetails, PointerEvent, Rect, Size, WheelUnit};
use shm::Buffers;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// The role of a protocol object, for routing its events.
#[derive(Clone)]
pub(crate) enum Role {
    Registry,
    WmBase,
    Seat,
    Pointer,
    Keyboard,
    Output,
    XdgOutput(ObjectId),
    Surface(Weak<WaylandWindow>),
    XdgSurface(Weak<WaylandWindow>),
    Toplevel(Weak<WaylandWindow>),
    Decoration(Weak<WaylandWindow>),
    FractionalScale(Weak<WaylandWindow>),
    Frame(Weak<WaylandWindow>),
    ActivationToken(Weak<WaylandWindow>),
    DataDevice,
    DataOffer,
    DataSource,
    Popup(Weak<WaylandWindow>),
}

/// The globals that the client binds.
#[derive(Default)]
pub(crate) struct Globals {
    pub(crate) compositor: Option<Proxy>,
    pub(crate) shm: Option<Proxy>,
    pub(crate) wm_base: Option<Proxy>,
    pub(crate) seat: Option<Proxy>,
    pub(crate) decoration_manager: Option<Proxy>,
    pub(crate) fractional_scale: Option<Proxy>,
    pub(crate) viewporter: Option<Proxy>,
    pub(crate) cursor_shape: Option<Proxy>,
    pub(crate) activation: Option<Proxy>,
    pub(crate) text_input_manager: Option<Proxy>,
    pub(crate) data_device_manager: Option<Proxy>,
    pub(crate) pointer_gestures: Option<Proxy>,
    pub(crate) wm_dialog: Option<Proxy>,
    pub(crate) system_bell: Option<Proxy>,
    pub(crate) output_manager: Option<Proxy>,
}

struct Output {
    proxy: Proxy,
    /// The registry name of the global.
    name: u32,
    xdg: Option<Proxy>,
    scale: i32,
    position: Point,
    /// The mode size in pixels.
    pixels: Size,
    /// The logical rectangle from `xdg-output`, when it is known.
    logical: Option<Rect>,
}

impl Output {
    fn rect(&self) -> Rect {
        self.logical.unwrap_or_else(|| {
            let scale = f64::from(self.scale.max(1));
            Rect::new(
                self.position,
                Size::new(self.pixels.width / scale, self.pixels.height / scale),
            )
        })
    }
}

/// Scroll values that wait for `wl_pointer.frame`.
#[derive(Default)]
struct Axis {
    delta: Point,
    /// High-resolution wheel steps, in 120ths of a detent.
    steps: Point,
    discrete: bool,
    any: bool,
}

struct Seat {
    pointer: Option<Proxy>,
    keyboard_proxy: Option<Proxy>,
    input: LinuxInput,
    keyboard: Keyboard,
    pointer_focus: Option<Weak<WaylandWindow>>,
    keyboard_focus: Option<Weak<WaylandWindow>>,
    position: Point,
    enter_serial: u32,
    /// The serial of the latest keyboard focus, key, or button event,
    /// for the selection.
    input_serial: u32,
    /// The serial of the latest button press, for move, resize, and
    /// activation.
    press_serial: Option<u32>,
    pressed: u32,
    clicks: ClickCounter,
    axis: Axis,
    cursor_device: Option<Proxy>,
    cursor_surface: Option<(Proxy, Buffers)>,
}

/// A connection to a Wayland compositor and its windows.
pub(crate) struct Client {
    pub(crate) connection: Connection,
    _registry: Proxy,
    pub(crate) globals: RefCell<Globals>,
    /// The crate's global state machine.
    pub(crate) connection_state: RefCell<WaylandConnection>,
    pub(crate) routes: RefCell<HashMap<ObjectId, Role>>,
    windows: RefCell<Vec<Rc<WaylandWindow>>>,
    outputs: RefCell<Vec<Output>>,
    seat: RefCell<Seat>,
    /// The activation token from the launcher, used by the first window.
    launch_token: RefCell<Option<String>>,
    /// Activation tokens that wait for their `done` event.
    pending_tokens: RefCell<Vec<Proxy>>,
    /// Set when the compositor closed the connection.
    lost: Cell<bool>,
    /// The clipboard and drag-and-drop state.
    data: RefCell<data::DataState>,
}

// Event opcodes.
const REGISTRY_GLOBAL: u16 = 0;
const REGISTRY_GLOBAL_REMOVE: u16 = 1;
const WM_BASE_PING: u16 = 0;
const WM_BASE_PONG: u16 = 3;
const SEAT_CAPABILITIES: u16 = 0;
const SEAT_GET_POINTER: u16 = 0;
const SEAT_GET_KEYBOARD: u16 = 1;
const OUTPUT_GEOMETRY: u16 = 0;
const OUTPUT_MODE: u16 = 1;
const OUTPUT_SCALE: u16 = 3;
const XDG_OUTPUT_LOGICAL_POSITION: u16 = 0;
const XDG_OUTPUT_LOGICAL_SIZE: u16 = 1;
const SURFACE_ENTER: u16 = 0;
const SURFACE_LEAVE: u16 = 1;
const SURFACE_PREFERRED_BUFFER_SCALE: u16 = 2;
const XDG_SURFACE_CONFIGURE: u16 = 0;
const TOPLEVEL_CONFIGURE: u16 = 0;
const TOPLEVEL_CLOSE: u16 = 1;
const POPUP_CONFIGURE: u16 = 0;
const POPUP_DONE: u16 = 1;
const POINTER_ENTER: u16 = 0;
const POINTER_LEAVE: u16 = 1;
const POINTER_MOTION: u16 = 2;
const POINTER_BUTTON: u16 = 3;
const POINTER_AXIS: u16 = 4;
const POINTER_FRAME: u16 = 5;
const POINTER_AXIS_DISCRETE: u16 = 8;
const POINTER_AXIS_VALUE120: u16 = 9;
const POINTER_SET_CURSOR: u16 = 0;
const KEYBOARD_KEYMAP: u16 = 0;
const KEYBOARD_ENTER: u16 = 1;
const KEYBOARD_LEAVE: u16 = 2;
const KEYBOARD_KEY: u16 = 3;
const KEYBOARD_MODIFIERS: u16 = 4;
const KEYBOARD_REPEAT_INFO: u16 = 5;
const CURSOR_SHAPE_GET_POINTER: u16 = 1;
const CURSOR_SHAPE_SET_SHAPE: u16 = 1;
const ACTIVATION_GET_TOKEN: u16 = 1;
const ACTIVATION_ACTIVATE: u16 = 2;
const TOKEN_SET_SERIAL: u16 = 0;
const TOKEN_SET_SURFACE: u16 = 2;
const TOKEN_COMMIT: u16 = 3;
const TOKEN_DONE: u16 = 0;
const OUTPUT_MANAGER_GET_XDG_OUTPUT: u16 = 1;

/// `WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1`.
const KEYMAP_FORMAT_XKB_V1: u32 = 1;

/// The interface and the highest version that the client binds of each
/// global it uses.
fn wanted(interface: &str) -> Option<(&'static Interface, u32)> {
    Some(match interface {
        "wl_compositor" => (&wl::WL_COMPOSITOR, 6),
        "wl_shm" => (&wl::WL_SHM, 1),
        "xdg_wm_base" => (&wl::XDG_WM_BASE, 6),
        "wl_seat" => (&wl::WL_SEAT, 9),
        "wl_output" => (&wl::WL_OUTPUT, 4),
        "zxdg_decoration_manager_v1" => (&wl::ZXDG_DECORATION_MANAGER_V1, 1),
        "wp_fractional_scale_manager_v1" => (&wl::WP_FRACTIONAL_SCALE_MANAGER_V1, 1),
        "wp_viewporter" => (&wl::WP_VIEWPORTER, 1),
        "wp_cursor_shape_manager_v1" => (&wl::WP_CURSOR_SHAPE_MANAGER_V1, 1),
        "xdg_activation_v1" => (&wl::XDG_ACTIVATION_V1, 1),
        "zwp_text_input_manager_v3" => (&wl::ZWP_TEXT_INPUT_MANAGER_V3, 1),
        "wl_data_device_manager" => (&wl::WL_DATA_DEVICE_MANAGER, 3),
        "zwp_pointer_gestures_v1" => (&wl::ZWP_POINTER_GESTURES_V1, 3),
        "xdg_wm_dialog_v1" => (&wl::XDG_WM_DIALOG_V1, 1),
        "xdg_system_bell_v1" => (&wl::XDG_SYSTEM_BELL_V1, 1),
        "zxdg_output_manager_v1" => (&wl::ZXDG_OUTPUT_MANAGER_V1, 3),
        _ => return None,
    })
}

/// The `wp_cursor_shape_device_v1` shape of a core cursor.
fn cursor_shape(shape: &CursorShape) -> u32 {
    match shape {
        CursorShape::Arrow | CursorShape::Custom { .. } => 1,
        CursorShape::PointingHand => 4,
        CursorShape::Crosshair => 8,
        CursorShape::IBeam => 9,
        CursorShape::NotAllowed => 15,
        CursorShape::OpenHand => 16,
        CursorShape::ClosedHand => 17,
        CursorShape::ResizeLeftRight => 26,
        CursorShape::ResizeUpDown => 27,
        CursorShape::ResizeNorthEastSouthWest => 28,
        CursorShape::ResizeNorthWestSouthEast => 29,
    }
}

fn connection_error(error: wl::WaylandError) -> Error {
    match error {
        wl::WaylandError::Library(error) => Error::Library(error),
        wl::WaylandError::ConnectFailed => Error::NoDisplay(error.to_string()),
        error => Error::Connection(error.to_string()),
    }
}

impl Client {
    /// Connects to the compositor of `WAYLAND_DISPLAY` and binds the
    /// globals. Fails when a global that windows need is missing.
    pub(crate) fn connect() -> Result<Rc<Self>, Error> {
        let connection = Connection::connect(None).map_err(connection_error)?;
        let registry = connection.display_request(1).map_err(connection_error)?;
        let mut state = WaylandConnection::new();
        let _ = state.connect();
        let client = Rc::new(Self {
            connection,
            _registry: registry,
            globals: RefCell::new(Globals::default()),
            connection_state: RefCell::new(state),
            routes: RefCell::new(HashMap::new()),
            windows: RefCell::new(Vec::new()),
            outputs: RefCell::new(Vec::new()),
            seat: RefCell::new(Seat {
                pointer: None,
                keyboard_proxy: None,
                input: LinuxInput::new(),
                keyboard: Keyboard::new().map_err(Error::Library)?,
                pointer_focus: None,
                keyboard_focus: None,
                position: Point::new(0.0, 0.0),
                enter_serial: 0,
                input_serial: 0,
                press_serial: None,
                pressed: 0,
                clicks: ClickCounter::default(),
                axis: Axis::default(),
                cursor_device: None,
                cursor_surface: None,
            }),
            launch_token: RefCell::new(
                std::env::var("XDG_ACTIVATION_TOKEN")
                    .ok()
                    .filter(|token| !token.is_empty()),
            ),
            pending_tokens: RefCell::new(Vec::new()),
            lost: Cell::new(false),
            data: RefCell::new(data::DataState::default()),
        });
        client
            .routes
            .borrow_mut()
            .insert(client._registry.id(), Role::Registry);
        // The first roundtrip lists the globals; the second one delivers
        // the first events of the bound globals: seat capabilities,
        // output modes, and the keymap.
        for _ in 0..2 {
            client.connection.roundtrip().map_err(connection_error)?;
            client.handle_events();
        }
        client.check_requirements()?;
        Ok(client)
    }

    /// Checks the globals that windows need: `wl_compositor` version 4 or
    /// later, `wl_shm`, `xdg_wm_base`, and `wl_seat`.
    fn check_requirements(&self) -> Result<(), Error> {
        let globals = self.globals.borrow();
        match &globals.compositor {
            Some(compositor) if compositor.version() >= 4 => {}
            _ => return Err(Error::MissingProtocol("wl_compositor version 4")),
        }
        if globals.shm.is_none() {
            return Err(Error::MissingProtocol("wl_shm"));
        }
        if globals.seat.is_none() {
            return Err(Error::MissingProtocol("wl_seat"));
        }
        self.connection_state
            .borrow_mut()
            .bind_required_globals()
            .map_err(|_| Error::MissingProtocol("xdg_wm_base"))
    }

    fn bind(&self, name: u32, interface: &str, version: u32) {
        let Some((table, highest)) = wanted(interface) else {
            return;
        };
        let version = version.min(highest);
        let Ok(Some(proxy)) = self
            ._registry
            .request(0, &[Arg::Uint(name), Arg::Bind(table, version)])
        else {
            return;
        };
        let id = proxy.id();
        let mut state = self.connection_state.borrow_mut();
        let mut globals = self.globals.borrow_mut();
        let mut routes = self.routes.borrow_mut();
        match interface {
            "wl_compositor" => {
                let _ = state.advertise(WaylandGlobal::Compositor, version);
                globals.compositor = Some(proxy);
            }
            "wl_shm" => globals.shm = Some(proxy),
            "xdg_wm_base" => {
                let _ = state.advertise(WaylandGlobal::XdgWmBase, version);
                routes.insert(id, Role::WmBase);
                globals.wm_base = Some(proxy);
            }
            "wl_seat" if globals.seat.is_none() => {
                let _ = state.advertise(WaylandGlobal::Seat, version);
                routes.insert(id, Role::Seat);
                globals.seat = Some(proxy);
            }
            "wl_output" => {
                routes.insert(id, Role::Output);
                let xdg = globals.output_manager.as_ref().and_then(|manager| {
                    manager
                        .request(
                            OUTPUT_MANAGER_GET_XDG_OUTPUT,
                            &[Arg::NewId, Arg::Object(Some(&proxy))],
                        )
                        .ok()
                        .flatten()
                });
                if let Some(xdg) = &xdg {
                    routes.insert(xdg.id(), Role::XdgOutput(id));
                }
                self.outputs.borrow_mut().push(Output {
                    proxy,
                    name,
                    xdg,
                    scale: 1,
                    position: Point::new(0.0, 0.0),
                    pixels: Size::new(0.0, 0.0),
                    logical: None,
                });
            }
            "zxdg_output_manager_v1" => {
                // Outputs bound before the manager get their xdg-output
                // now.
                for output in self.outputs.borrow_mut().iter_mut() {
                    if output.xdg.is_none() {
                        output.xdg = proxy
                            .request(
                                OUTPUT_MANAGER_GET_XDG_OUTPUT,
                                &[Arg::NewId, Arg::Object(Some(&output.proxy))],
                            )
                            .ok()
                            .flatten();
                        if let Some(xdg) = &output.xdg {
                            routes.insert(xdg.id(), Role::XdgOutput(output.proxy.id()));
                        }
                    }
                }
                globals.output_manager = Some(proxy);
            }
            "zxdg_decoration_manager_v1" => globals.decoration_manager = Some(proxy),
            "wp_fractional_scale_manager_v1" => globals.fractional_scale = Some(proxy),
            "wp_viewporter" => globals.viewporter = Some(proxy),
            "wp_cursor_shape_manager_v1" => globals.cursor_shape = Some(proxy),
            "xdg_activation_v1" => globals.activation = Some(proxy),
            "zwp_text_input_manager_v3" => {
                let _ = state.advertise(WaylandGlobal::TextInputManager, version);
                globals.text_input_manager = Some(proxy);
            }
            "wl_data_device_manager" => {
                let _ = state.advertise(WaylandGlobal::DataDeviceManager, version);
                globals.data_device_manager = Some(proxy);
            }
            "zwp_pointer_gestures_v1" => globals.pointer_gestures = Some(proxy),
            "xdg_wm_dialog_v1" => globals.wm_dialog = Some(proxy),
            "xdg_system_bell_v1" => globals.system_bell = Some(proxy),
            _ => {}
        }
        drop((state, globals, routes));
        if matches!(interface, "wl_seat" | "wl_data_device_manager") {
            self.ensure_data_device();
        }
    }

    /// Creates a window. It maps on `show`.
    pub(crate) fn create_window(
        self: &Rc<Self>,
        shared: Rc<super::window::WindowShared>,
        app_id: &str,
    ) -> Result<Rc<WaylandWindow>, Error> {
        let window = WaylandWindow::new(self, shared, app_id)?;
        self.windows.borrow_mut().push(Rc::clone(&window));
        Ok(window)
    }

    /// Creates a popup of `parent` next to `anchor`. It grabs the input
    /// with the serial of the latest press or key.
    pub(crate) fn create_popup(
        self: &Rc<Self>,
        shared: Rc<super::window::WindowShared>,
        parent: &WaylandWindow,
        anchor: Rect,
    ) -> Result<Rc<WaylandWindow>, Error> {
        let serial = Some(self.seat.borrow().input_serial).filter(|&serial| serial != 0);
        let popup = WaylandWindow::new_popup(self, shared, parent, anchor, serial)?;
        self.windows.borrow_mut().push(Rc::clone(&popup));
        Ok(popup)
    }

    pub(crate) fn window_of(&self, surface: ObjectId) -> Option<Rc<WaylandWindow>> {
        self.windows
            .borrow()
            .iter()
            .find(|window| window.surface.id() == surface)
            .cloned()
    }

    pub(crate) fn forget_window(&self, surface: ObjectId) {
        self.windows
            .borrow_mut()
            .retain(|window| window.surface.id() != surface);
        let mut seat = self.seat.borrow_mut();
        let gone = |focus: &Option<Weak<WaylandWindow>>| {
            focus
                .as_ref()
                .and_then(Weak::upgrade)
                .is_none_or(|window| window.surface.id() == surface)
        };
        if gone(&seat.pointer_focus) {
            seat.pointer_focus = None;
        }
        if gone(&seat.keyboard_focus) {
            seat.keyboard_focus = None;
            seat.keyboard.release_all();
        }
    }

    /// The number of open windows, without the popups.
    pub(crate) fn window_count(&self) -> usize {
        self.windows
            .borrow()
            .iter()
            .filter(|window| !window.is_popup())
            .count()
    }

    pub(crate) fn output_scale(&self, output: ObjectId) -> Option<i32> {
        self.outputs
            .borrow()
            .iter()
            .find(|candidate| candidate.proxy.id() == output)
            .map(|output| output.scale)
    }

    /// The logical rectangles of the outputs.
    pub(crate) fn screens(&self) -> Vec<Rect> {
        self.outputs.borrow().iter().map(Output::rect).collect()
    }

    pub(crate) fn last_press_serial(&self) -> Option<u32> {
        self.seat.borrow().press_serial
    }

    /// Asks the compositor to focus `window`: with the launcher's token
    /// for the first window, else with a new token for the latest input.
    pub(crate) fn activate_window(&self, window: &WaylandWindow) {
        let globals = self.globals.borrow();
        let Some(activation) = &globals.activation else {
            return;
        };
        if let Some(token) = self.launch_token.borrow_mut().take() {
            let _ = activation.request(
                ACTIVATION_ACTIVATE,
                &[Arg::Str(Some(&token)), Arg::Object(Some(&window.surface))],
            );
            return;
        }
        let seat = self.seat.borrow();
        let (Some(serial), Some(seat_proxy)) = (seat.press_serial, &globals.seat) else {
            return;
        };
        let Ok(Some(token)) = activation.request(ACTIVATION_GET_TOKEN, &[Arg::NewId]) else {
            return;
        };
        let _ = token.request(
            TOKEN_SET_SERIAL,
            &[Arg::Uint(serial), Arg::Object(Some(seat_proxy))],
        );
        if let Some(focused) = seat.keyboard_focus.as_ref().and_then(Weak::upgrade) {
            let _ = token.request(TOKEN_SET_SURFACE, &[Arg::Object(Some(&focused.surface))]);
        }
        let _ = token.request(TOKEN_COMMIT, &[]);
        let weak = self
            .window_of(window.surface.id())
            .as_ref()
            .map_or_else(Weak::new, Rc::downgrade);
        self.routes
            .borrow_mut()
            .insert(token.id(), Role::ActivationToken(weak));
        self.pending_tokens.borrow_mut().push(token);
    }

    /// Shows the cursor of the window under the pointer.
    pub(crate) fn set_cursor(&self, surface: ObjectId, shape: &CursorShape) -> Result<(), Error> {
        let over = self
            .seat
            .borrow()
            .pointer_focus
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|window| window.surface.id() == surface);
        if over {
            self.apply_cursor(shape);
        }
        Ok(())
    }

    fn apply_cursor(&self, shape: &CursorShape) {
        let globals = self.globals.borrow();
        let mut seat_state = self.seat.borrow_mut();
        let seat = &mut *seat_state;
        let serial = seat.enter_serial;
        let Some(pointer) = seat.pointer.as_ref() else {
            return;
        };
        if let CursorShape::Custom { image, hotspot } = shape
            && let Some(compositor) = &globals.compositor
            && let Some(shm) = &globals.shm
        {
            let (surface, mut buffers) = match seat.cursor_surface.take() {
                Some(existing) => existing,
                None => match compositor.request(0, &[Arg::NewId]) {
                    Ok(Some(surface)) => (surface, Buffers::default()),
                    _ => return,
                },
            };
            fill_surface(&surface, &mut buffers, shm, image);
            let _ = surface.request(6, &[]);
            let _ = pointer.request(
                POINTER_SET_CURSOR,
                &[
                    Arg::Uint(serial),
                    Arg::Object(Some(&surface)),
                    Arg::Int(hotspot.x.round() as i32),
                    Arg::Int(hotspot.y.round() as i32),
                ],
            );
            seat.cursor_surface = Some((surface, buffers));
            return;
        }
        if seat.cursor_device.is_none()
            && let Some(manager) = &globals.cursor_shape
        {
            seat.cursor_device = manager
                .request(
                    CURSOR_SHAPE_GET_POINTER,
                    &[Arg::NewId, Arg::Object(Some(pointer))],
                )
                .ok()
                .flatten();
        }
        if let Some(device) = &seat.cursor_device {
            let _ = device.request(
                CURSOR_SHAPE_SET_SHAPE,
                &[Arg::Uint(serial), Arg::Uint(cursor_shape(shape))],
            );
        }
    }

    pub(crate) fn text_input_changed(&self, _surface: ObjectId) {}

    /// The deadline of the next key repeat.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.seat.borrow().keyboard.next_repeat()
    }

    /// Paints each window that is ready for a frame.
    pub(crate) fn paint(&self) {
        let windows: Vec<Rc<WaylandWindow>> = self.windows.borrow().clone();
        for window in windows {
            window.paint_if_ready(self);
        }
    }

    /// Paints, sends the requests, waits up to `timeout` for the display
    /// or for one of `extra`, and handles the events that arrived.
    /// Returns the readiness of `extra`.
    pub(crate) fn wait(
        &self,
        extra: &[Watch<'_>],
        timeout: Option<Duration>,
    ) -> Result<Vec<Ready>, Error> {
        if self.lost.get() {
            return Err(Error::Connection(
                "the compositor closed the connection".to_owned(),
            ));
        }
        self.connection
            .dispatch_pending()
            .map_err(|error| self.fail(error))?;
        self.handle_events();
        self.paint();
        let guard = self
            .connection
            .prepare_read()
            .map_err(|error| self.fail(error))?;
        // Preparing the read may dispatch queued events.
        let queued = self.handle_events();
        let flushed = self.connection.flush().map_err(|error| self.fail(error))?;
        let now = Instant::now();
        let mut timeout = if queued {
            Some(Duration::ZERO)
        } else {
            timeout
        };
        if let Some(deadline) = self.next_deadline() {
            let until = deadline.saturating_duration_since(now);
            timeout = Some(timeout.map_or(until, |timeout| timeout.min(until)));
        }
        let mut watches = Vec::with_capacity(extra.len() + 1);
        watches.push(Watch {
            fd: self.connection.fd(),
            read: true,
            write: !flushed,
        });
        watches.extend(extra.iter().map(|watch| Watch {
            fd: watch.fd,
            read: watch.read,
            write: watch.write,
        }));
        let ready =
            libc::wait(&watches, timeout).map_err(|error| Error::Connection(error.to_string()))?;
        let display = ready[0];
        if display.read {
            guard.read().map_err(|error| self.fail(error))?;
        } else {
            drop(guard);
        }
        if display.hangup && !display.read {
            return Err(self.fail(wl::WaylandError::Connection(
                "the compositor hung up".to_owned(),
            )));
        }
        self.connection
            .dispatch_pending()
            .map_err(|error| self.fail(error))?;
        self.handle_events();
        self.repeat_keys();
        Ok(ready[1..].to_vec())
    }

    /// Marks the connection lost and sends `Closing` to every window.
    fn fail(&self, error: wl::WaylandError) -> Error {
        if !self.lost.replace(true) {
            for window in self.windows.borrow().iter() {
                window.shared.push(WindowEvent::Closing);
            }
        }
        connection_error(error)
    }

    pub(crate) fn flush(&self) {
        let _ = self.connection.flush();
    }

    fn repeat_keys(&self) {
        let mut seat = self.seat.borrow_mut();
        let events = seat.keyboard.repeat_due(Instant::now());
        if let Some(window) = seat.keyboard_focus.as_ref().and_then(Weak::upgrade) {
            for event in events {
                window.shared.push(event);
            }
        }
    }

    /// Handles the queued events. Returns true when there were any.
    fn handle_events(&self) -> bool {
        let mut any = false;
        loop {
            let events = self.connection.take_events();
            if events.is_empty() {
                return any;
            }
            any = true;
            for event in events {
                self.handle(event);
            }
        }
    }

    fn handle(&self, mut event: Event) {
        let role = {
            let routes = self.routes.borrow();
            match routes.get(&event.object) {
                Some(role) => role.clone(),
                None => {
                    self.handle_unrouted(&event);
                    return;
                }
            }
        };
        match role {
            Role::Registry => match event.opcode {
                REGISTRY_GLOBAL => {
                    let name = event.arg(0).uint();
                    let interface = event.arg(1).str().to_owned();
                    let version = event.arg(2).uint();
                    self.bind(name, &interface, version);
                }
                REGISTRY_GLOBAL_REMOVE => {
                    let name = event.arg(0).uint();
                    self.outputs
                        .borrow_mut()
                        .retain(|output| output.name != name);
                }
                _ => {}
            },
            Role::WmBase if event.opcode == WM_BASE_PING => {
                if let Some(wm_base) = &self.globals.borrow().wm_base {
                    let _ = wm_base.request(WM_BASE_PONG, &[Arg::Uint(event.arg(0).uint())]);
                }
            }
            Role::Seat if event.opcode == SEAT_CAPABILITIES => {
                self.seat_capabilities(event.arg(0).uint());
            }
            Role::Pointer => self.pointer_event(&event),
            Role::Keyboard => self.keyboard_event(&mut event),
            Role::Output => self.output_event(&event),
            Role::DataDevice => self.data_device_event(&mut event),
            Role::DataOffer => self.data_offer_event(&event),
            Role::DataSource => self.data_source_event(&mut event),
            Role::XdgOutput(output) => {
                let mut outputs = self.outputs.borrow_mut();
                let Some(output) = outputs
                    .iter_mut()
                    .find(|candidate| candidate.proxy.id() == output)
                else {
                    return;
                };
                let mut logical = output.logical.unwrap_or_else(|| output.rect());
                match event.opcode {
                    XDG_OUTPUT_LOGICAL_POSITION => {
                        logical.origin = Point::new(
                            f64::from(event.arg(0).int()),
                            f64::from(event.arg(1).int()),
                        );
                    }
                    XDG_OUTPUT_LOGICAL_SIZE => {
                        logical.size =
                            Size::new(f64::from(event.arg(0).int()), f64::from(event.arg(1).int()));
                    }
                    _ => return,
                }
                output.logical = Some(logical);
            }
            Role::Surface(window) => {
                let Some(window) = window.upgrade() else {
                    return;
                };
                match event.opcode {
                    SURFACE_ENTER | SURFACE_LEAVE => {
                        if let Some(output) = event.arg(0).object() {
                            window.output_changed(output, event.opcode == SURFACE_ENTER, self);
                        }
                    }
                    SURFACE_PREFERRED_BUFFER_SCALE => {
                        window.preferred_buffer_scale(event.arg(0).int())
                    }
                    _ => {}
                }
            }
            Role::XdgSurface(window) if event.opcode == XDG_SURFACE_CONFIGURE => {
                if let Some(window) = window.upgrade() {
                    window.surface_configure(event.arg(0).uint());
                }
            }
            Role::Toplevel(window) => {
                let Some(window) = window.upgrade() else {
                    return;
                };
                match event.opcode {
                    TOPLEVEL_CONFIGURE => {
                        window.toplevel_configure(
                            event.arg(0).int(),
                            event.arg(1).int(),
                            event.arg(2).array(),
                        );
                    }
                    TOPLEVEL_CLOSE => window.shared.push(WindowEvent::Closing),
                    _ => {}
                }
            }
            Role::Popup(window) => {
                let Some(window) = window.upgrade() else {
                    return;
                };
                match event.opcode {
                    POPUP_CONFIGURE => {
                        window.popup_configure(event.arg(2).int(), event.arg(3).int());
                    }
                    POPUP_DONE => window.shared.push(WindowEvent::Closing),
                    _ => {}
                }
            }
            Role::Decoration(window) => {
                if let Some(window) = window.upgrade() {
                    window.decoration_mode(event.arg(0).uint());
                }
            }
            Role::FractionalScale(window) => {
                if let Some(window) = window.upgrade() {
                    window.fractional_scale(event.arg(0).uint());
                }
            }
            Role::Frame(window) => {
                self.routes.borrow_mut().remove(&event.object);
                if let Some(window) = window.upgrade() {
                    window.frame_done(event.object);
                }
            }
            Role::ActivationToken(window) if event.opcode == TOKEN_DONE => {
                self.routes.borrow_mut().remove(&event.object);
                self.pending_tokens
                    .borrow_mut()
                    .retain(|token| token.id() != event.object);
                let token = event.arg(0).str().to_owned();
                let globals = self.globals.borrow();
                if let (Some(window), Some(activation)) = (window.upgrade(), &globals.activation) {
                    let _ = activation.request(
                        ACTIVATION_ACTIVATE,
                        &[Arg::Str(Some(&token)), Arg::Object(Some(&window.surface))],
                    );
                }
            }
            _ => {}
        }
    }

    /// Events of objects without a role: buffer releases.
    fn handle_unrouted(&self, event: &Event) {
        for window in self.windows.borrow().iter() {
            if window.buffer_released(event.object) {
                return;
            }
        }
        if let Some((_, buffers)) = &mut self.seat.borrow_mut().cursor_surface
            && buffers.release(event.object)
        {
            return;
        }
        self.release_drag_buffer(event.object);
    }

    fn seat_capabilities(&self, capabilities: u32) {
        let pointer = capabilities & 1 != 0;
        let keyboard = capabilities & 2 != 0;
        let touch = capabilities & 4 != 0;
        let globals = self.globals.borrow();
        let Some(seat_proxy) = &globals.seat else {
            return;
        };
        let mut seat = self.seat.borrow_mut();
        seat.input.update_seat(keyboard, pointer, touch);
        let mut routes = self.routes.borrow_mut();
        if pointer && seat.pointer.is_none() {
            seat.pointer = seat_proxy
                .request(SEAT_GET_POINTER, &[Arg::NewId])
                .ok()
                .flatten();
            if let Some(proxy) = &seat.pointer {
                routes.insert(proxy.id(), Role::Pointer);
            }
        } else if !pointer {
            seat.cursor_device = None;
            seat.pointer = None;
            seat.pointer_focus = None;
        }
        if keyboard && seat.keyboard_proxy.is_none() {
            seat.keyboard_proxy = seat_proxy
                .request(SEAT_GET_KEYBOARD, &[Arg::NewId])
                .ok()
                .flatten();
            if let Some(proxy) = &seat.keyboard_proxy {
                routes.insert(proxy.id(), Role::Keyboard);
            }
        } else if !keyboard {
            seat.keyboard_proxy = None;
            seat.keyboard_focus = None;
            seat.keyboard.release_all();
        }
    }

    fn output_event(&self, event: &Event) {
        let mut outputs = self.outputs.borrow_mut();
        let Some(output) = outputs
            .iter_mut()
            .find(|output| output.proxy.id() == event.object)
        else {
            return;
        };
        match event.opcode {
            OUTPUT_GEOMETRY => {
                output.position =
                    Point::new(f64::from(event.arg(0).int()), f64::from(event.arg(1).int()));
            }
            // Only the current mode (flag 1) sets the size.
            OUTPUT_MODE if event.arg(0).uint() & 1 != 0 => {
                output.pixels =
                    Size::new(f64::from(event.arg(1).int()), f64::from(event.arg(2).int()));
            }
            OUTPUT_SCALE => output.scale = event.arg(0).int().max(1),
            _ => {}
        }
    }

    fn pointer_window(&self) -> Option<Rc<WaylandWindow>> {
        self.seat
            .borrow()
            .pointer_focus
            .as_ref()
            .and_then(Weak::upgrade)
    }

    fn modifiers(&self) -> Modifiers {
        self.seat.borrow().keyboard.modifiers()
    }

    fn push_pointer(&self, event: PointerEvent, details: PointerDetails) {
        let modifiers = self.modifiers();
        if let Some(window) = self.pointer_window() {
            window.shared.push(WindowEvent::Pointer {
                event,
                modifiers,
                details,
            });
        }
    }

    fn pointer_event(&self, event: &Event) {
        let version = self
            .seat
            .borrow()
            .pointer
            .as_ref()
            .map_or(1, Proxy::version);
        match event.opcode {
            POINTER_ENTER => {
                let window = event
                    .arg(1)
                    .object()
                    .and_then(|surface| self.window_of(surface));
                let position = Point::new(event.arg(2).fixed(), event.arg(3).fixed());
                {
                    let mut seat = self.seat.borrow_mut();
                    seat.enter_serial = event.arg(0).uint();
                    seat.pointer_focus = window.as_ref().map(Rc::downgrade);
                    seat.position = position;
                }
                if let Some(window) = window {
                    let shape = window.shared.cursor.borrow().clone();
                    self.apply_cursor(&shape);
                    self.push_pointer(PointerEvent::Move { position }, PointerDetails::default());
                }
            }
            POINTER_LEAVE => {
                let mut seat = self.seat.borrow_mut();
                seat.pointer_focus = None;
                seat.clicks.reset();
            }
            POINTER_MOTION => {
                let position = Point::new(event.arg(1).fixed(), event.arg(2).fixed());
                let allowed = {
                    let mut seat = self.seat.borrow_mut();
                    seat.position = position;
                    seat.input
                        .dispatch(InputEvent::PointerMotion {
                            x: position.x,
                            y: position.y,
                        })
                        .is_ok()
                };
                if allowed {
                    self.push_pointer(PointerEvent::Move { position }, PointerDetails::default());
                }
            }
            POINTER_BUTTON => {
                let serial = event.arg(0).uint();
                let code = event.arg(2).uint();
                let pressed = event.arg(3).uint() == 1;
                let button = pointer::button(code);
                let (position, count, allowed) = {
                    let mut seat = self.seat.borrow_mut();
                    let allowed = seat
                        .input
                        .dispatch(InputEvent::PointerButton {
                            button: code,
                            pressed,
                        })
                        .is_ok();
                    let position = seat.position;
                    seat.input_serial = serial;
                    let count = if pressed {
                        seat.press_serial = Some(serial);
                        seat.pressed += 1;
                        seat.clicks.press(Instant::now(), position, button)
                    } else {
                        seat.pressed = seat.pressed.saturating_sub(1);
                        seat.clicks.release()
                    };
                    (position, count, allowed)
                };
                if !allowed {
                    return;
                }
                let event = if pressed {
                    PointerEvent::Down { position, button }
                } else {
                    PointerEvent::Up { position, button }
                };
                let details = PointerDetails {
                    click_count: count,
                    ..PointerDetails::default()
                };
                self.push_pointer(event, details);
            }
            POINTER_AXIS => {
                let value = event.arg(2).fixed();
                {
                    let mut seat = self.seat.borrow_mut();
                    let axis = &mut seat.axis;
                    if event.arg(1).uint() == 0 {
                        axis.delta.y += value;
                    } else {
                        axis.delta.x += value;
                    }
                    axis.any = true;
                }
                // Before version 5 there is no frame event.
                if version < 5 {
                    self.flush_axis();
                }
            }
            POINTER_AXIS_DISCRETE | POINTER_AXIS_VALUE120 => {
                let steps = f64::from(event.arg(1).int())
                    * if event.opcode == POINTER_AXIS_DISCRETE {
                        120.0
                    } else {
                        1.0
                    };
                // A compositor that sends value120 sends no discrete.
                if event.opcode == POINTER_AXIS_DISCRETE && version >= 8 {
                    return;
                }
                let mut seat = self.seat.borrow_mut();
                let axis = &mut seat.axis;
                if event.arg(0).uint() == 0 {
                    axis.steps.y += steps;
                } else {
                    axis.steps.x += steps;
                }
                axis.discrete = true;
                axis.any = true;
            }
            POINTER_FRAME => self.flush_axis(),
            _ => {}
        }
    }

    /// Sends the scroll of a pointer frame: lines for wheel detents,
    /// points for smooth scrolling. Positive deltas scroll down and
    /// right, as on macOS.
    fn flush_axis(&self) {
        let (axis, position) = {
            let mut seat = self.seat.borrow_mut();
            (std::mem::take(&mut seat.axis), seat.position)
        };
        if !axis.any {
            return;
        }
        let (delta, unit) = if axis.discrete {
            (
                Point::new(axis.steps.x / 120.0, axis.steps.y / 120.0),
                WheelUnit::Lines,
            )
        } else {
            (axis.delta, WheelUnit::Points)
        };
        if delta.x == 0.0 && delta.y == 0.0 {
            return;
        }
        self.push_pointer(
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            },
            PointerDetails::default(),
        );
    }

    fn keyboard_event(&self, event: &mut Event) {
        match event.opcode {
            KEYBOARD_KEYMAP => {
                let format = event.arg(0).uint();
                let size = event.arg(2).uint() as usize;
                let Some(fd) = event.arg_mut(1).and_then(wl::Value::take_fd) else {
                    return;
                };
                if format != KEYMAP_FORMAT_XKB_V1 {
                    return;
                }
                if let Ok(mapping) = libc::Mapping::read_only(std::os::fd::AsFd::as_fd(&fd), size) {
                    self.seat
                        .borrow_mut()
                        .keyboard
                        .set_keymap_text(mapping.bytes());
                }
            }
            KEYBOARD_ENTER => {
                let window = event
                    .arg(1)
                    .object()
                    .and_then(|surface| self.window_of(surface));
                let mut seat = self.seat.borrow_mut();
                seat.input_serial = event.arg(0).uint();
                seat.keyboard_focus = window.as_ref().map(Rc::downgrade);
            }
            KEYBOARD_LEAVE => {
                let mut seat = self.seat.borrow_mut();
                seat.keyboard_focus = None;
                seat.keyboard.release_all();
            }
            KEYBOARD_KEY => {
                let code = event.arg(2).uint();
                let pressed = event.arg(3).uint() == 1;
                let mut seat = self.seat.borrow_mut();
                seat.input_serial = event.arg(0).uint();
                if seat
                    .input
                    .dispatch(InputEvent::Key { code, pressed })
                    .is_err()
                {
                    return;
                }
                let events = seat.keyboard.key_event(code + 8, pressed, Instant::now());
                if let Some(window) = seat.keyboard_focus.as_ref().and_then(Weak::upgrade) {
                    for event in events {
                        window.shared.push(event);
                    }
                }
            }
            KEYBOARD_MODIFIERS => {
                self.seat.borrow_mut().keyboard.update_modifiers(
                    event.arg(1).uint(),
                    event.arg(2).uint(),
                    event.arg(3).uint(),
                    event.arg(4).uint(),
                );
            }
            KEYBOARD_REPEAT_INFO => {
                let rate = u32::try_from(event.arg(0).int()).unwrap_or(0);
                let delay = u64::try_from(event.arg(1).int()).unwrap_or(0);
                self.seat.borrow_mut().keyboard.set_repeat(RepeatInfo {
                    rate,
                    delay: Duration::from_millis(delay),
                });
            }
            _ => {}
        }
    }
}

/// Attaches `image` to `surface` in a shm buffer, without a commit.
fn fill_surface(
    surface: &Proxy,
    buffers: &mut Buffers,
    shm: &Proxy,
    image: &choscorui_core::Image,
) {
    let everything = Rect::new(
        Point::new(0.0, 0.0),
        Size::new(f64::from(image.width()), f64::from(image.height())),
    );
    let Some(buffer) = buffers.acquire(shm, image.width(), image.height(), everything) else {
        return;
    };
    // Straight RGBA to premultiplied ARGB32 in native order.
    for (target, source) in buffer
        .memory
        .bytes_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(image.pixels().as_chunks::<4>().0)
    {
        let alpha = u16::from(source[3]);
        let scale = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
        target.copy_from_slice(&[
            scale(source[2]),
            scale(source[1]),
            scale(source[0]),
            source[3],
        ]);
    }
    let _ = surface.request(
        1,
        &[Arg::Object(Some(&buffer.proxy)), Arg::Int(0), Arg::Int(0)],
    );
    let _ = surface.request(
        9,
        &[
            Arg::Int(0),
            Arg::Int(0),
            Arg::Int(i32::MAX),
            Arg::Int(i32::MAX),
        ],
    );
}
