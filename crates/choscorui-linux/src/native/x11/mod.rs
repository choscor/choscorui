//! The X11 display server, over libxcb.
//!
//! `Client` owns the connection, the extensions, the keyboard and the
//! pointer, and the windows. XKB with xkbcommon-x11 reads the keymap and
//! the modifiers, with detectable auto-repeat, so a held key arrives as
//! repeated presses. XInput 2.2 sends the pointer events and the smooth
//! scroll valuators; without it, core events stand in. The scale is
//! `Xft.dpi` over 96, from the resource database on the root window.

mod dnd;
mod protocol;
mod selection;
mod window;

pub(crate) use selection::Contents;
pub(crate) use window::X11Surface;

use super::keyboard::{Keyboard, RepeatInfo};
use super::pointer::ClickCounter;
use super::types::{Error, WindowEvent};
use super::window::WindowShared;
use crate::ffi::libc::{self, Ready, Watch};
use crate::ffi::x11::{
    Bytes, ConnectError, Connection, XCB_ATOM_RESOURCE_MANAGER, XCB_ATOM_STRING, XCB_ATOM_WINDOW,
    XCB_BUTTON_PRESS, XCB_BUTTON_RELEASE, XCB_CLIENT_MESSAGE, XCB_CONFIGURE_NOTIFY,
    XCB_CW_EVENT_MASK, XCB_ENTER_NOTIFY, XCB_EVENT_MASK_PROPERTY_CHANGE,
    XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY, XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT, XCB_EXPOSE,
    XCB_FOCUS_IN, XCB_FOCUS_OUT, XCB_GE_GENERIC, XCB_INPUT_BUTTON_PRESS, XCB_INPUT_BUTTON_RELEASE,
    XCB_INPUT_DEVICE_CHANGED, XCB_INPUT_ENTER, XCB_INPUT_LEAVE, XCB_INPUT_MOTION,
    XCB_INPUT_POINTER_EVENT_FLAGS_POINTER_EMULATED, XCB_KEY_PRESS, XCB_KEY_RELEASE,
    XCB_LEAVE_NOTIFY, XCB_MAP_NOTIFY, XCB_MOTION_NOTIFY, XCB_NOTIFY_DETAIL_INFERIOR,
    XCB_NOTIFY_DETAIL_POINTER, XCB_PROPERTY_NOTIFY, XCB_SHM_COMPLETION, XCB_UNMAP_NOTIFY,
    XCB_VISIBILITY_FULLY_OBSCURED, XCB_VISIBILITY_NOTIFY, XCB_XKB_STATE_NOTIFY,
};
use crate::{InputEvent, LinuxInput};
use choscorui_core::{
    CursorShape, Image, Point, PointerDetails, PointerEvent, Rect, Size, WheelUnit,
};
use protocol::{Button, POINTS_PER_SCROLL_INCREMENT, Scrolling};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// The atoms that the client uses.
pub(crate) struct Atoms {
    pub(crate) wm_protocols: u32,
    pub(crate) wm_delete_window: u32,
    pub(crate) wm_change_state: u32,
    pub(crate) wm_client_machine: u32,
    pub(crate) utf8_string: u32,
    pub(crate) net_wm_name: u32,
    pub(crate) net_wm_pid: u32,
    pub(crate) net_wm_window_type: u32,
    pub(crate) net_wm_window_type_normal: u32,
    pub(crate) net_wm_state: u32,
    pub(crate) net_wm_state_maximized_vert: u32,
    pub(crate) net_wm_state_maximized_horz: u32,
    pub(crate) net_wm_state_fullscreen: u32,
    pub(crate) net_frame_extents: u32,
    pub(crate) net_wm_moveresize: u32,
    pub(crate) net_active_window: u32,
    pub(crate) net_supporting_wm_check: u32,
    pub(crate) gtk_show_window_menu: u32,
}

impl Atoms {
    fn intern(connection: &Connection) -> Self {
        let names = [
            "WM_PROTOCOLS",
            "WM_DELETE_WINDOW",
            "WM_CHANGE_STATE",
            "WM_CLIENT_MACHINE",
            "UTF8_STRING",
            "_NET_WM_NAME",
            "_NET_WM_PID",
            "_NET_WM_WINDOW_TYPE",
            "_NET_WM_WINDOW_TYPE_NORMAL",
            "_NET_WM_STATE",
            "_NET_WM_STATE_MAXIMIZED_VERT",
            "_NET_WM_STATE_MAXIMIZED_HORZ",
            "_NET_WM_STATE_FULLSCREEN",
            "_NET_FRAME_EXTENTS",
            "_NET_WM_MOVERESIZE",
            "_NET_ACTIVE_WINDOW",
            "_NET_SUPPORTING_WM_CHECK",
            "_GTK_SHOW_WINDOW_MENU",
        ];
        let atoms = connection.intern_atoms(&names);
        let atom = |index: usize| atoms.get(index).copied().unwrap_or(0);
        Self {
            wm_protocols: atom(0),
            wm_delete_window: atom(1),
            wm_change_state: atom(2),
            wm_client_machine: atom(3),
            utf8_string: atom(4),
            net_wm_name: atom(5),
            net_wm_pid: atom(6),
            net_wm_window_type: atom(7),
            net_wm_window_type_normal: atom(8),
            net_wm_state: atom(9),
            net_wm_state_maximized_vert: atom(10),
            net_wm_state_maximized_horz: atom(11),
            net_wm_state_fullscreen: atom(12),
            net_frame_extents: atom(13),
            net_wm_moveresize: atom(14),
            net_active_window: atom(15),
            net_supporting_wm_check: atom(16),
            gtk_show_window_menu: atom(17),
        }
    }
}

/// The extensions that the server offers.
struct Extensions {
    /// The first event of XKB, after xkbcommon set it up.
    xkb_event: Option<u8>,
    /// The major opcode of XInput 2.2.
    xinput: Option<u8>,
    /// The major opcode and the first event of MIT-SHM.
    shm: Option<(u8, u8)>,
    /// True when XKB sends repeated presses without releases.
    detectable_repeat: bool,
}

struct Seat {
    input: LinuxInput,
    keyboard: Keyboard,
    pointer_focus: Option<Weak<X11Surface>>,
    keyboard_focus: Option<Weak<X11Surface>>,
    /// The pointer position in the focus window, in points.
    position: Point,
    /// The pointer position on the root window, in pixels.
    root: (i32, i32),
    /// The XInput device of the latest pointer event.
    device: u16,
    clicks: ClickCounter,
    scrolling: Scrolling,
    /// True while the left button is down, which a drag needs.
    left_down: bool,
}

/// A connection to an X server and its windows.
pub(crate) struct Client {
    pub(crate) connection: Connection,
    pub(crate) atoms: Atoms,
    extensions: Extensions,
    shm: Cell<bool>,
    scale: Cell<f64>,
    windows: RefCell<Vec<Rc<X11Surface>>>,
    seat: RefCell<Seat>,
    /// The cursors of the shapes, by `cursor_index`.
    cursors: RefCell<HashMap<usize, u32>>,
    /// The ARGB32 picture format of RENDER, for custom cursors.
    argb_format: OnceCell<Option<u32>>,
    /// Set when the connection failed.
    lost: Cell<bool>,
    transfer_atoms: selection::TransferAtoms,
    /// The hidden window that owns the selections and receives their data.
    selection_window: u32,
    selections: RefCell<selection::Selections>,
    dnd: RefCell<dnd::DndState>,
    atom_ids: RefCell<HashMap<String, u32>>,
    atom_names: RefCell<HashMap<u32, String>>,
    /// The events that arrived during a nested wait, for the loop.
    deferred: RefCell<VecDeque<Bytes>>,
    /// The server time of the latest input or property event.
    time: Cell<u32>,
}

/// The X pointer of the virtual core pointer, before any XInput event.
const VIRTUAL_CORE_POINTER: u16 = 2;

/// The index of a cursor shape, and the theme names and the core cursor
/// font glyph of the shapes that are not custom.
fn cursor_names(shape: &CursorShape) -> Option<(usize, [&'static str; 2], u16)> {
    Some(match shape {
        CursorShape::Arrow => (0, ["default", "left_ptr"], 68),
        CursorShape::IBeam => (1, ["text", "xterm"], 152),
        CursorShape::PointingHand => (2, ["pointer", "hand2"], 60),
        CursorShape::ClosedHand => (3, ["grabbing", "fleur"], 52),
        CursorShape::OpenHand => (4, ["grab", "hand1"], 58),
        CursorShape::Crosshair => (5, ["crosshair", "crosshair"], 34),
        CursorShape::NotAllowed => (6, ["not-allowed", "circle"], 24),
        CursorShape::ResizeLeftRight => (7, ["ew-resize", "sb_h_double_arrow"], 108),
        CursorShape::ResizeUpDown => (8, ["ns-resize", "sb_v_double_arrow"], 116),
        CursorShape::ResizeNorthWestSouthEast => (9, ["nwse-resize", "bottom_right_corner"], 14),
        CursorShape::ResizeNorthEastSouthWest => (10, ["nesw-resize", "bottom_left_corner"], 12),
        CursorShape::Custom { .. } => return None,
    })
}

/// The scale of the resource database on the root window.
fn read_scale(connection: &Connection) -> f64 {
    connection
        .get_property(
            connection.root(),
            XCB_ATOM_RESOURCE_MANAGER,
            XCB_ATOM_STRING,
        )
        .map_or(1.0, |(_, bytes)| {
            protocol::xft_scale(&String::from_utf8_lossy(&bytes))
        })
}

impl Client {
    /// Connects to the server of `DISPLAY` and sets up the extensions.
    pub(crate) fn connect() -> Result<Rc<Self>, Error> {
        let connection = Connection::connect().map_err(|error| match error {
            ConnectError::Library(error) => Error::Library(error),
            ConnectError::Refused => {
                Error::NoDisplay("the X server of DISPLAY refused the connection".to_owned())
            }
        })?;
        let mut keyboard = Keyboard::new().map_err(Error::Library)?;
        let atoms = Atoms::intern(&connection);
        let xkb_event = connection.xkb_setup();
        let mut detectable_repeat = false;
        if xkb_event.is_some() {
            connection.xkb_select_events();
            detectable_repeat = connection.xkb_detectable_auto_repeat();
            keyboard.set_state(connection.xkb_state(keyboard.context()));
        }
        // The server repeats keys, so the keyboard does not.
        keyboard.set_repeat(RepeatInfo {
            rate: 0,
            ..RepeatInfo::default()
        });
        let xinput = connection
            .query_extension("XInputExtension")
            .filter(|_| connection.xi_query_version())
            .map(|(opcode, _)| opcode);
        let shm = connection
            .query_extension("MIT-SHM")
            .filter(|_| connection.shm_available());
        let scrolling = if xinput.is_some() {
            connection
                .xi_query_devices()
                .map(|reply| Scrolling::from_reply(&reply))
                .unwrap_or_default()
        } else {
            Scrolling::default()
        };
        // The root window reports changes of the resource database.
        connection.change_window_attributes(
            connection.root(),
            XCB_CW_EVENT_MASK,
            &[XCB_EVENT_MASK_PROPERTY_CHANGE],
        );
        let scale = read_scale(&connection);
        let transfer_atoms = selection::TransferAtoms::intern(&connection);
        let selection_window = connection.generate_id();
        connection.create_window(
            selection_window,
            connection.root(),
            (1, 1),
            XCB_CW_EVENT_MASK,
            &[XCB_EVENT_MASK_PROPERTY_CHANGE],
        );
        let mut input = LinuxInput::new();
        input.update_seat(true, true, false);
        connection.flush();
        Ok(Rc::new(Self {
            connection,
            atoms,
            extensions: Extensions {
                xkb_event,
                xinput,
                shm,
                detectable_repeat,
            },
            shm: Cell::new(shm.is_some()),
            scale: Cell::new(scale),
            windows: RefCell::new(Vec::new()),
            seat: RefCell::new(Seat {
                input,
                keyboard,
                pointer_focus: None,
                keyboard_focus: None,
                position: Point::new(0.0, 0.0),
                root: (0, 0),
                device: VIRTUAL_CORE_POINTER,
                clicks: ClickCounter::default(),
                scrolling,
                left_down: false,
            }),
            cursors: RefCell::new(HashMap::new()),
            argb_format: OnceCell::new(),
            lost: Cell::new(false),
            transfer_atoms,
            selection_window,
            selections: RefCell::new(selection::Selections::default()),
            dnd: RefCell::new(dnd::DndState::default()),
            atom_ids: RefCell::new(HashMap::new()),
            atom_names: RefCell::new(HashMap::new()),
            deferred: RefCell::new(VecDeque::new()),
            time: Cell::new(0),
        }))
    }

    pub(crate) fn create_window(
        self: &Rc<Self>,
        shared: Rc<WindowShared>,
        app_id: &str,
    ) -> Result<Rc<X11Surface>, Error> {
        let surface = X11Surface::new(self, shared, app_id)?;
        self.windows.borrow_mut().push(Rc::clone(&surface));
        Ok(surface)
    }

    pub(crate) fn window_count(&self) -> usize {
        self.windows.borrow().len()
    }

    pub(crate) fn forget_window(&self, id: u32) {
        self.windows.borrow_mut().retain(|window| window.id != id);
        let mut seat = self.seat.borrow_mut();
        let gone = |focus: &Option<Weak<X11Surface>>| {
            focus
                .as_ref()
                .and_then(Weak::upgrade)
                .is_none_or(|window| window.id == id)
        };
        if gone(&seat.pointer_focus) {
            seat.pointer_focus = None;
        }
        if gone(&seat.keyboard_focus) {
            seat.keyboard_focus = None;
        }
    }

    fn window(&self, id: u32) -> Option<Rc<X11Surface>> {
        self.windows
            .borrow()
            .iter()
            .find(|window| window.id == id)
            .cloned()
    }

    pub(crate) fn scale(&self) -> f64 {
        self.scale.get()
    }

    pub(crate) fn has_xinput(&self) -> bool {
        self.extensions.xinput.is_some()
    }

    pub(crate) fn shm_enabled(&self) -> bool {
        self.shm.get()
    }

    pub(crate) fn root_position(&self) -> (i32, i32) {
        self.seat.borrow().root
    }

    pub(crate) fn pointer_device(&self) -> u16 {
        self.seat.borrow().device
    }

    /// Rings the XKB bell of the core keyboard for the focused window.
    /// Returns false when the server has no XKB.
    pub(crate) fn ring_bell(&self) -> bool {
        if self.extensions.xkb_event.is_none() {
            return false;
        }
        let window = self
            .seat
            .borrow()
            .keyboard_focus
            .as_ref()
            .and_then(Weak::upgrade)
            .map_or(0, |window| window.id);
        let rung = self.connection.xkb_bell(window);
        self.connection.flush();
        rung
    }

    /// Sends a client message to the window manager.
    pub(crate) fn send_to_root(&self, message: &[u8; 32]) {
        self.connection.send_event(
            self.connection.root(),
            XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT | XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY,
            message,
        );
    }

    /// True when an EWMH window manager runs.
    fn has_window_manager(&self) -> bool {
        !self
            .connection
            .get_property32(
                self.connection.root(),
                self.atoms.net_supporting_wm_check,
                XCB_ATOM_WINDOW,
            )
            .is_empty()
    }

    /// Activates a window that just mapped: through the window manager,
    /// or by taking the input focus when no window manager runs.
    pub(crate) fn activate(&self, window: u32) {
        if self.has_window_manager() {
            let message =
                protocol::client_message(window, self.atoms.net_active_window, [1, 0, 0, 0, 0]);
            self.send_to_root(&message);
        } else {
            self.connection.set_input_focus(window);
        }
    }

    /// The cursor of `shape`: a theme cursor, a core cursor font glyph,
    /// or, for a custom image, a RENDER cursor that the caller frees.
    pub(crate) fn cursor(&self, shape: &CursorShape) -> Option<u32> {
        if let CursorShape::Custom { image, hotspot } = shape {
            return self.custom_cursor(image, *hotspot);
        }
        let (index, names, glyph) = cursor_names(shape)?;
        if let Some(cursor) = self.cursors.borrow().get(&index) {
            return Some(*cursor);
        }
        let cursor = names
            .iter()
            .find_map(|name| self.connection.themed_cursor(name))
            .unwrap_or_else(|| self.connection.glyph_cursor(glyph));
        self.cursors.borrow_mut().insert(index, cursor);
        Some(cursor)
    }

    /// A cursor from straight RGBA pixels, through a 32-bit pixmap in
    /// the ARGB32 format of RENDER.
    fn custom_cursor(&self, image: &Image, hotspot: Point) -> Option<u32> {
        let format = (*self.argb_format.get_or_init(|| {
            self.connection
                .render_pict_formats()
                .and_then(|reply| protocol::argb32_format(&reply))
        }))?;
        let width = u16::try_from(image.width()).ok()?;
        let height = u16::try_from(image.height()).ok()?;
        // Straight RGBA to premultiplied BGRA, which is ARGB32 on
        // little-endian machines.
        let mut pixels = Vec::with_capacity(image.pixels().len());
        for source in image.pixels().as_chunks::<4>().0 {
            let alpha = u16::from(source[3]);
            let scale = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
            pixels.extend_from_slice(&[
                scale(source[2]),
                scale(source[1]),
                scale(source[0]),
                source[3],
            ]);
        }
        let connection = &self.connection;
        let pixmap = connection.generate_id();
        connection.create_pixmap(32, pixmap, (width, height));
        let gc = connection.generate_id();
        connection.create_gc(gc, pixmap, 0, &[]);
        window::put_rows(
            connection,
            (pixmap, gc),
            32,
            (&pixels, usize::from(width) * 4),
            (0, 0, width, height),
        );
        connection.free_gc(gc);
        let hotspot = (hotspot.x.round() as u16, hotspot.y.round() as u16);
        let cursor = connection.render_cursor(pixmap, format, hotspot);
        connection.free_pixmap(pixmap);
        cursor
    }

    /// The logical rectangles of the RandR monitors, or of the root
    /// window without RandR.
    pub(crate) fn screens(&self) -> Vec<Rect> {
        let scale = self.scale.get();
        let mut rects = self
            .connection
            .randr_monitors()
            .map(|reply| protocol::monitors(&reply))
            .unwrap_or_default();
        if rects.is_empty() {
            let screen = self.connection.screen();
            rects.push(Rect::new(
                Point::new(0.0, 0.0),
                Size::new(
                    f64::from(screen.width_in_pixels),
                    f64::from(screen.height_in_pixels),
                ),
            ));
        }
        rects
            .into_iter()
            .map(|rect| {
                Rect::new(
                    Point::new(rect.origin.x / scale, rect.origin.y / scale),
                    Size::new(rect.size.width / scale, rect.size.height / scale),
                )
            })
            .collect()
    }

    /// Paints each window that has a pending region.
    pub(crate) fn paint(&self) {
        let windows: Vec<Rc<X11Surface>> = self.windows.borrow().clone();
        for window in windows {
            window.paint_if_ready(self);
        }
    }

    pub(crate) fn flush(&self) {
        self.connection.flush();
    }

    /// Handles the events, paints, sends the requests, waits up to
    /// `timeout` for the server or for one of `extra`, and handles the
    /// events that arrived. Returns the readiness of `extra`.
    pub(crate) fn wait(
        &self,
        extra: &[Watch<'_>],
        timeout: Option<Duration>,
    ) -> Result<Vec<Ready>, Error> {
        self.check()?;
        self.handle_all();
        self.paint();
        self.connection.flush();
        let mut queued = false;
        while let Some(event) = self.connection.poll_queued_event() {
            self.handle(&event);
            queued = true;
        }
        queued |= self.handle_deferred();
        self.check()?;
        let timeout = match (queued, self.expire_drag()) {
            (true, _) => Some(Duration::ZERO),
            (false, Some(left)) => Some(timeout.map_or(left, |timeout| timeout.min(left))),
            (false, None) => timeout,
        };
        let mut watches = Vec::with_capacity(extra.len() + 1);
        watches.push(Watch {
            fd: self.connection.fd(),
            read: true,
            write: false,
        });
        watches.extend(extra.iter().map(|watch| Watch {
            fd: watch.fd,
            read: watch.read,
            write: watch.write,
        }));
        let ready =
            libc::wait(&watches, timeout).map_err(|error| Error::Connection(error.to_string()))?;
        drop(watches);
        self.handle_all();
        self.expire_drag();
        self.check()?;
        Ok(ready[1..].to_vec())
    }

    /// Handles the events that arrived, and those that a nested wait kept.
    fn handle_all(&self) {
        loop {
            self.handle_deferred();
            let Some(event) = self.connection.poll_event() else {
                break;
            };
            self.handle(&event);
        }
        self.handle_deferred();
    }

    /// Handles the events that a nested wait kept. Returns true when there
    /// were any.
    fn handle_deferred(&self) -> bool {
        let mut handled = false;
        loop {
            let event = self.deferred.borrow_mut().pop_front();
            let Some(event) = event else {
                return handled;
            };
            self.handle(&event);
            handled = true;
        }
    }

    /// Fails when the connection broke. The first failure sends
    /// `Closing` to every window.
    fn check(&self) -> Result<(), Error> {
        if !self.connection.has_error() {
            return Ok(());
        }
        if !self.lost.replace(true) {
            for window in self.windows.borrow().iter() {
                window.shared.push(WindowEvent::Closing);
            }
        }
        Err(Error::Connection(
            "the X server closed the connection".to_owned(),
        ))
    }

    fn handle(&self, event: &Bytes) {
        let kind = event.u8(0) & 0x7f;
        if kind == 0 {
            self.error(event);
            return;
        }
        self.note_time(kind, event);
        if self.selection_event(event) {
            return;
        }
        if Some(kind) == self.extensions.xkb_event {
            self.xkb_event(event);
            return;
        }
        if let Some((_, first)) = self.extensions.shm
            && kind == first.wrapping_add(XCB_SHM_COMPLETION)
        {
            if let Some(window) = self.window(event.u32(4)) {
                window.completed();
            }
            return;
        }
        match kind {
            XCB_KEY_PRESS | XCB_KEY_RELEASE => self.key(event, kind == XCB_KEY_PRESS),
            XCB_BUTTON_PRESS | XCB_BUTTON_RELEASE => {
                self.set_root(i32::from(event.i16(20)), i32::from(event.i16(22)));
                self.button(
                    event.u32(12),
                    u32::from(event.u8(1)),
                    kind == XCB_BUTTON_PRESS,
                    (f64::from(event.i16(24)), f64::from(event.i16(26))),
                );
            }
            XCB_MOTION_NOTIFY => {
                self.set_root(i32::from(event.i16(20)), i32::from(event.i16(22)));
                self.motion(
                    event.u32(12),
                    (f64::from(event.i16(24)), f64::from(event.i16(26))),
                );
            }
            XCB_ENTER_NOTIFY => {
                self.enter(
                    event.u32(12),
                    (f64::from(event.i16(24)), f64::from(event.i16(26))),
                );
            }
            XCB_LEAVE_NOTIFY => self.leave(event.u32(12)),
            XCB_FOCUS_IN | XCB_FOCUS_OUT => {
                let detail = event.u8(1);
                if detail != XCB_NOTIFY_DETAIL_INFERIOR && detail != XCB_NOTIFY_DETAIL_POINTER {
                    self.focus(event.u32(4), kind == XCB_FOCUS_IN);
                }
            }
            XCB_EXPOSE => {
                if let Some(window) = self.window(event.u32(4)) {
                    window.exposed((event.u16(8), event.u16(10), event.u16(12), event.u16(14)));
                }
            }
            XCB_CONFIGURE_NOTIFY => {
                if let Some(window) = self.window(event.u32(8)) {
                    window.configured(event.u16(20), event.u16(22));
                }
            }
            XCB_MAP_NOTIFY => {
                if let Some(window) = self.window(event.u32(8)) {
                    window.mapped(self);
                }
            }
            XCB_UNMAP_NOTIFY => {
                if let Some(window) = self.window(event.u32(8)) {
                    window.unmapped();
                }
            }
            XCB_VISIBILITY_NOTIFY => {
                if let Some(window) = self.window(event.u32(4)) {
                    window.visibility(event.u8(8) == XCB_VISIBILITY_FULLY_OBSCURED);
                }
            }
            XCB_PROPERTY_NOTIFY => {
                if event.u32(4) == self.connection.root()
                    && event.u32(8) == XCB_ATOM_RESOURCE_MANAGER
                {
                    self.rescale();
                }
            }
            XCB_CLIENT_MESSAGE if event.u8(1) == 32 && self.dnd_message(event) => {}
            XCB_CLIENT_MESSAGE => {
                let protocols = event.u8(1) == 32
                    && event.u32(8) == self.atoms.wm_protocols
                    && event.u32(12) == self.atoms.wm_delete_window;
                if protocols && let Some(window) = self.window(event.u32(4)) {
                    window.shared.push(WindowEvent::Closing);
                }
            }
            XCB_GE_GENERIC if self.extensions.xinput == Some(event.u8(1)) => {
                self.xinput_event(event);
            }
            _ => {}
        }
    }

    /// Keeps the server time of an input or property event.
    fn note_time(&self, kind: u8, event: &Bytes) {
        let time = match kind {
            XCB_KEY_PRESS..=XCB_LEAVE_NOTIFY => event.u32(4),
            XCB_PROPERTY_NOTIFY => event.u32(12),
            XCB_GE_GENERIC if self.extensions.xinput == Some(event.u8(1)) => event.u32(12),
            _ => return,
        };
        if time != 0 {
            self.time.set(time);
        }
    }

    /// Test support: moves the pointer to `position` of `window` and,
    /// with `button`, presses or releases the left button, through XTEST.
    pub(crate) fn simulate_pointer(
        &self,
        window: u32,
        position: Point,
        button: Option<bool>,
    ) -> Result<(), Error> {
        let connection = &self.connection;
        let scale = self.scale.get();
        let (x, y) = connection
            .translate(window, connection.root())
            .ok_or(Error::Closed)?;
        let x = f64::from(x) + (position.x * scale).round();
        let y = f64::from(y) + (position.y * scale).round();
        let point = (x as i16, y as i16);
        connection
            .fake_input(XCB_MOTION_NOTIFY, 0, point)
            .map_err(Error::Library)?;
        if let Some(pressed) = button {
            let kind = if pressed {
                XCB_BUTTON_PRESS
            } else {
                XCB_BUTTON_RELEASE
            };
            connection
                .fake_input(kind, 1, point)
                .map_err(Error::Library)?;
        }
        connection.flush();
        Ok(())
    }

    /// Handles an error of a request. A failed MIT-SHM request turns
    /// shared images off; the other errors concern windows that are
    /// already gone.
    fn error(&self, event: &Bytes) {
        let major = event.u8(10);
        if self
            .extensions
            .shm
            .is_some_and(|(opcode, _)| opcode == major)
            && self.shm.replace(false)
        {
            for window in self.windows.borrow().iter() {
                window.drop_shared_image();
            }
        }
    }

    fn rescale(&self) {
        let scale = read_scale(&self.connection);
        if self.scale.replace(scale) == scale {
            return;
        }
        let windows: Vec<Rc<X11Surface>> = self.windows.borrow().clone();
        for window in windows {
            window.scale_changed(self, scale);
        }
    }

    fn xkb_event(&self, event: &Bytes) {
        let mut seat = self.seat.borrow_mut();
        if event.u8(1) == XCB_XKB_STATE_NOTIFY {
            seat.keyboard.update_modifiers(
                u32::from(event.u8(10)),
                u32::from(event.u8(11)),
                u32::from(event.u8(12)),
                u32::from(event.u8(18)),
            );
        } else {
            // A new keyboard or a new keymap.
            let state = self.connection.xkb_state(seat.keyboard.context());
            seat.keyboard.set_state(state);
        }
    }

    fn key(&self, event: &Bytes, pressed: bool) {
        let code = u32::from(event.u8(1));
        let mut seat = self.seat.borrow_mut();
        let repeat = pressed
            && self.extensions.detectable_repeat
            && seat.input.pressed_keys().contains(&code);
        if seat
            .input
            .dispatch(InputEvent::Key { code, pressed })
            .is_err()
        {
            return;
        }
        let events = if repeat {
            seat.keyboard.repeat_event(code)
        } else {
            seat.keyboard.key_event(code, pressed, Instant::now())
        };
        let window = seat
            .keyboard_focus
            .as_ref()
            .and_then(Weak::upgrade)
            .or_else(|| self.window(event.u32(12)));
        drop(seat);
        if let Some(window) = window {
            for event in events {
                window.shared.push(event);
            }
        }
    }

    fn focus(&self, id: u32, focused: bool) {
        let Some(window) = self.window(id) else {
            return;
        };
        {
            let mut seat = self.seat.borrow_mut();
            if focused {
                seat.keyboard_focus = Some(Rc::downgrade(&window));
            } else {
                seat.keyboard_focus = None;
                seat.keyboard.release_all();
                // The releases of the held keys go to the next focus.
                seat.input.update_seat(false, true, false);
                seat.input.update_seat(true, true, false);
            }
        }
        if window.shared.focused.replace(focused) != focused {
            window.shared.push(WindowEvent::Focused(focused));
        }
    }

    fn set_root(&self, x: i32, y: i32) {
        self.seat.borrow_mut().root = (x, y);
    }

    fn push_pointer(&self, window: &X11Surface, event: PointerEvent, details: PointerDetails) {
        let modifiers = self.seat.borrow().keyboard.modifiers();
        window.shared.push(WindowEvent::Pointer {
            event,
            modifiers,
            details,
        });
    }

    /// The window of a pointer event and the logical position of
    /// (`x`, `y`) pixels in it.
    fn pointer_target(&self, id: u32, (x, y): (f64, f64)) -> Option<(Rc<X11Surface>, Point)> {
        let window = self.window(id)?;
        let scale = window.shared.scale.get();
        Some((window, Point::new(x / scale, y / scale)))
    }

    fn enter(&self, id: u32, pixels: (f64, f64)) {
        let Some((window, position)) = self.pointer_target(id, pixels) else {
            return;
        };
        {
            let mut seat = self.seat.borrow_mut();
            seat.pointer_focus = Some(Rc::downgrade(&window));
            seat.position = position;
        }
        self.push_pointer(
            &window,
            PointerEvent::Move { position },
            PointerDetails::default(),
        );
    }

    fn leave(&self, id: u32) {
        let mut seat = self.seat.borrow_mut();
        if seat
            .pointer_focus
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|window| window.id == id)
        {
            seat.pointer_focus = None;
            seat.clicks.reset();
        }
    }

    fn motion(&self, id: u32, pixels: (f64, f64)) {
        if self.drag_motion() {
            return;
        }
        let Some((window, position)) = self.pointer_target(id, pixels) else {
            return;
        };
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
            self.push_pointer(
                &window,
                PointerEvent::Move { position },
                PointerDetails::default(),
            );
        }
    }

    fn button(&self, id: u32, number: u32, pressed: bool, pixels: (f64, f64)) {
        if number == 1 {
            self.seat.borrow_mut().left_down = pressed;
            if !pressed && self.drag_release() {
                return;
            }
        }
        let Some((window, position)) = self.pointer_target(id, pixels) else {
            return;
        };
        let button = match protocol::button(number) {
            Button::Wheel(lines) => {
                if pressed {
                    self.wheel(&window, position, lines, WheelUnit::Lines);
                }
                return;
            }
            Button::Press(button) => button,
        };
        let (count, allowed) = {
            let mut seat = self.seat.borrow_mut();
            seat.position = position;
            let allowed = seat
                .input
                .dispatch(InputEvent::PointerButton {
                    button: number,
                    pressed,
                })
                .is_ok();
            let count = if pressed {
                seat.clicks.press(Instant::now(), position, button)
            } else {
                seat.clicks.release()
            };
            (count, allowed)
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
        self.push_pointer(&window, event, details);
    }

    fn wheel(&self, window: &X11Surface, position: Point, delta: Point, unit: WheelUnit) {
        self.push_pointer(
            window,
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            },
            PointerDetails::default(),
        );
    }

    /// Reads the scroll classes and the valuator values again, after a
    /// device change or when the pointer enters a window.
    fn refresh_scrolling(&self) {
        if let Some(reply) = self.connection.xi_query_devices() {
            self.seat.borrow_mut().scrolling = Scrolling::from_reply(&reply);
        }
    }

    /// Handles an XInput 2 event in wire format.
    fn xinput_event(&self, event: &Bytes) {
        let device = event.u16(10);
        let window = event.u32(24);
        let pixels = (event.fp1616(40), event.fp1616(44));
        match event.u16(8) {
            XCB_INPUT_MOTION | XCB_INPUT_BUTTON_PRESS | XCB_INPUT_BUTTON_RELEASE => {
                {
                    let mut seat = self.seat.borrow_mut();
                    seat.device = device;
                    seat.root = (
                        event.fp1616(32).round() as i32,
                        event.fp1616(36).round() as i32,
                    );
                }
                let kind = event.u16(8);
                if kind == XCB_INPUT_MOTION {
                    self.xinput_motion(device, window, pixels, event);
                    return;
                }
                let emulated = event.u32(56) & XCB_INPUT_POINTER_EVENT_FLAGS_POINTER_EMULATED != 0;
                let number = event.u32(16);
                // The server emulates wheel buttons from the scroll
                // valuators, which the motion events carry.
                if emulated && (4..=7).contains(&number) {
                    return;
                }
                self.button(window, number, kind == XCB_INPUT_BUTTON_PRESS, pixels);
            }
            XCB_INPUT_ENTER => {
                self.seat.borrow_mut().device = device;
                self.refresh_scrolling();
                self.enter(window, pixels);
            }
            XCB_INPUT_LEAVE => self.leave(window),
            XCB_INPUT_DEVICE_CHANGED => self.refresh_scrolling(),
            _ => {}
        }
    }

    fn xinput_motion(&self, device: u16, id: u32, pixels: (f64, f64), event: &Bytes) {
        let scroll = {
            let mut seat = self.seat.borrow_mut();
            seat.scrolling.motion(device, &protocol::valuators(event))
        };
        let Some((window, position)) = self.pointer_target(id, pixels) else {
            return;
        };
        if let Some(steps) = scroll {
            let delta = Point::new(
                steps.x * POINTS_PER_SCROLL_INCREMENT,
                steps.y * POINTS_PER_SCROLL_INCREMENT,
            );
            self.wheel(&window, position, delta, WheelUnit::Points);
            // A scroll-only event does not move the pointer.
            if self.seat.borrow().position == position {
                return;
            }
        }
        self.motion(id, pixels);
    }
}
