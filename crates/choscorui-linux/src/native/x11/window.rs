//! An X11 window: its properties, its image, and its geometry.
//!
//! The window paints into a client-side image and copies the damaged
//! region to the server, through MIT-SHM when the server shares memory
//! with the client and with `PutImage` otherwise. A shared image stays
//! busy until its completion event. The image is a Z pixmap of the root
//! depth: 32 bits per pixel in BGRX order, which is the layout of the
//! paint buffer on little-endian machines.

use super::Client;
use super::protocol::{self, MOVERESIZE_MOVE};
use crate::X11Window;
use crate::ffi::libc::SysvSegment;
use crate::ffi::x11::{
    Connection, XCB_ATOM_ATOM, XCB_ATOM_CARDINAL, XCB_ATOM_STRING, XCB_ATOM_WM_CLASS,
    XCB_ATOM_WM_NAME, XCB_ATOM_WM_NORMAL_HINTS, XCB_ATOM_WM_SIZE_HINTS, XCB_CONFIG_WINDOW_HEIGHT,
    XCB_CONFIG_WINDOW_WIDTH, XCB_CONFIG_WINDOW_X, XCB_CONFIG_WINDOW_Y, XCB_CW_BACK_PIXEL,
    XCB_CW_BIT_GRAVITY, XCB_CW_CURSOR, XCB_CW_EVENT_MASK, XCB_EVENT_MASK_BUTTON_PRESS,
    XCB_EVENT_MASK_BUTTON_RELEASE, XCB_EVENT_MASK_ENTER_WINDOW, XCB_EVENT_MASK_EXPOSURE,
    XCB_EVENT_MASK_FOCUS_CHANGE, XCB_EVENT_MASK_KEY_PRESS, XCB_EVENT_MASK_KEY_RELEASE,
    XCB_EVENT_MASK_LEAVE_WINDOW, XCB_EVENT_MASK_POINTER_MOTION, XCB_EVENT_MASK_PROPERTY_CHANGE,
    XCB_EVENT_MASK_STRUCTURE_NOTIFY, XCB_EVENT_MASK_VISIBILITY_CHANGE, XCB_GC_GRAPHICS_EXPOSURES,
    XCB_GRAVITY_NORTH_WEST, XCB_INPUT_DEVICE_ALL_MASTER, XCB_INPUT_XI_EVENT_MASK_BUTTON_PRESS,
    XCB_INPUT_XI_EVENT_MASK_BUTTON_RELEASE, XCB_INPUT_XI_EVENT_MASK_DEVICE_CHANGED,
    XCB_INPUT_XI_EVENT_MASK_ENTER, XCB_INPUT_XI_EVENT_MASK_LEAVE, XCB_INPUT_XI_EVENT_MASK_MOTION,
};
use crate::native::paint::{self, Pixels};
use crate::native::types::{Error, ResizeEdge, WindowEvent};
use crate::native::window::{Surface, WindowShared};
use choscorui_core::{CursorShape, Point, Rect, Size, WindowGeometry};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Instant;

/// The memory of a window image.
enum Memory {
    /// A System V segment that the server attached as `id`.
    Shared {
        segment: SysvSegment,
        id: u32,
    },
    Local(Vec<u8>),
}

/// The client-side image of a window, in pixels.
struct Image {
    memory: Memory,
    width: u32,
    height: u32,
}

impl Image {
    fn bytes_mut(&mut self) -> &mut [u8] {
        match &mut self.memory {
            Memory::Shared { segment, .. } => segment.bytes_mut(),
            Memory::Local(bytes) => bytes,
        }
    }
}

struct State {
    /// The crate's window state machine.
    machine: X11Window,
    shown: bool,
    mapped: bool,
    /// True after the first `MapNotify`.
    was_mapped: bool,
    /// True when the application placed the window before it showed.
    placed: bool,
    /// The size of the window in pixels.
    pixels: (u32, u32),
    image: Option<Image>,
    /// True while the server reads the shared image.
    busy: bool,
    occluded: bool,
    destroyed: bool,
}

/// A native X11 window.
pub(crate) struct X11Surface {
    pub(crate) shared: Rc<WindowShared>,
    client: Weak<Client>,
    pub(crate) id: u32,
    gc: u32,
    state: RefCell<State>,
}

/// The pixel size of a logical size, within the 16 bits of X.
fn pixel_size(size: Size, scale: f64) -> (u32, u32) {
    let side = |points: f64| (points * scale).round().clamp(1.0, f64::from(u16::MAX)) as u32;
    (side(size.width), side(size.height))
}

/// The 16-bit value of a pixel count.
fn u16_of(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// The smallest pixel rectangle that covers a logical region, within
/// `width` x `height`: x, y, width, height.
fn pixel_rect(region: Rect, scale: f64, (width, height): (u32, u32)) -> (u16, u16, u16, u16) {
    let clamp = |value: f64, limit: u32| value.clamp(0.0, f64::from(limit)) as u32;
    let x = clamp((region.origin.x * scale).floor(), width);
    let y = clamp((region.origin.y * scale).floor(), height);
    let right = clamp(
        ((region.origin.x + region.size.width) * scale).ceil(),
        width,
    );
    let bottom = clamp(
        ((region.origin.y + region.size.height) * scale).ceil(),
        height,
    );
    (
        u16_of(x),
        u16_of(y),
        u16_of(right.saturating_sub(x)),
        u16_of(bottom.saturating_sub(y)),
    )
}

/// Copies a rectangle of `image` to a drawable with `PutImage`, in as
/// many requests as the maximum request length needs.
pub(crate) fn put_rows(
    connection: &Connection,
    target: (u32, u32),
    depth: u8,
    (bytes, stride): (&[u8], usize),
    (x, y, width, height): (u16, u16, u16, u16),
) {
    let row = usize::from(width) * 4;
    if row == 0 || height == 0 {
        return;
    }
    // The request header of `PutImage` is 24 bytes.
    let rows = (connection.maximum_request_bytes().saturating_sub(24) / row).max(1);
    let mut top = usize::from(y);
    let bottom = usize::from(y) + usize::from(height);
    let mut chunk = Vec::with_capacity(row * rows.min(usize::from(height)));
    while top < bottom {
        let count = rows.min(bottom - top);
        chunk.clear();
        for line in top..top + count {
            let start = line * stride + usize::from(x) * 4;
            let Some(source) = bytes.get(start..start + row) else {
                return;
            };
            chunk.extend_from_slice(source);
        }
        let position = (x.cast_signed(), u16_of(top as u32).cast_signed());
        connection.put_image(
            target,
            position,
            (width, u16_of(count as u32)),
            depth,
            &chunk,
        );
        top += count;
    }
}

impl X11Surface {
    /// Creates the window and sets its properties. It shows on `show`.
    pub(crate) fn new(
        client: &Rc<Client>,
        shared: Rc<WindowShared>,
        app_id: &str,
    ) -> Result<Rc<Self>, Error> {
        let connection = &client.connection;
        let scale = client.scale();
        shared.scale.set(scale);
        let pixels = pixel_size(shared.size.get(), scale);
        let mut machine = X11Window::new();
        machine
            .connect()
            .and_then(|()| machine.create(pixels.0, pixels.1))
            .map_err(|_| Error::InvalidSize)?;
        let id = connection.generate_id();
        let mut events = XCB_EVENT_MASK_EXPOSURE
            | XCB_EVENT_MASK_STRUCTURE_NOTIFY
            | XCB_EVENT_MASK_FOCUS_CHANGE
            | XCB_EVENT_MASK_VISIBILITY_CHANGE
            | XCB_EVENT_MASK_PROPERTY_CHANGE
            | XCB_EVENT_MASK_KEY_PRESS
            | XCB_EVENT_MASK_KEY_RELEASE;
        if !client.has_xinput() {
            events |= XCB_EVENT_MASK_BUTTON_PRESS
                | XCB_EVENT_MASK_BUTTON_RELEASE
                | XCB_EVENT_MASK_POINTER_MOTION
                | XCB_EVENT_MASK_ENTER_WINDOW
                | XCB_EVENT_MASK_LEAVE_WINDOW;
        }
        let screen = connection.screen();
        connection.create_window(
            id,
            screen.root,
            (u16_of(pixels.0), u16_of(pixels.1)),
            XCB_CW_BACK_PIXEL | XCB_CW_BIT_GRAVITY | XCB_CW_EVENT_MASK,
            &[screen.white_pixel, XCB_GRAVITY_NORTH_WEST, events],
        );
        if client.has_xinput() {
            connection.xi_select_events(
                id,
                XCB_INPUT_DEVICE_ALL_MASTER,
                XCB_INPUT_XI_EVENT_MASK_MOTION
                    | XCB_INPUT_XI_EVENT_MASK_BUTTON_PRESS
                    | XCB_INPUT_XI_EVENT_MASK_BUTTON_RELEASE
                    | XCB_INPUT_XI_EVENT_MASK_ENTER
                    | XCB_INPUT_XI_EVENT_MASK_LEAVE
                    | XCB_INPUT_XI_EVENT_MASK_DEVICE_CHANGED,
            );
        }
        let gc = connection.generate_id();
        connection.create_gc(gc, id, XCB_GC_GRAPHICS_EXPOSURES, &[0]);
        let surface = Rc::new(Self {
            shared,
            client: Rc::downgrade(client),
            id,
            gc,
            state: RefCell::new(State {
                machine,
                shown: false,
                mapped: false,
                was_mapped: false,
                placed: false,
                pixels,
                image: None,
                busy: false,
                occluded: false,
                destroyed: false,
            }),
        });
        surface.set_properties(client, app_id);
        client.make_aware(id);
        *surface.shared.parent_handle.borrow_mut() = Some(format!("x11:{id:x}"));
        if let Some(cursor) = client.cursor(&CursorShape::Arrow) {
            connection.change_window_attributes(id, XCB_CW_CURSOR, &[cursor]);
        }
        Ok(surface)
    }

    fn set_properties(&self, client: &Client, app_id: &str) {
        let connection = &client.connection;
        let atoms = &client.atoms;
        connection.set_property32(
            self.id,
            atoms.wm_protocols,
            XCB_ATOM_ATOM,
            &[atoms.wm_delete_window],
        );
        self.write_title(client, &self.shared.title.borrow());
        // `WM_CLASS` is the instance and the class, each NUL-terminated.
        let name = if app_id.is_empty() {
            std::env::args()
                .next()
                .and_then(|path| {
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "choscorui".to_owned())
        } else {
            app_id.to_owned()
        };
        let class = format!("{name}\0{name}\0");
        connection.set_property8(
            self.id,
            XCB_ATOM_WM_CLASS,
            XCB_ATOM_STRING,
            class.as_bytes(),
        );
        connection.set_property32(
            self.id,
            atoms.net_wm_pid,
            XCB_ATOM_CARDINAL,
            &[std::process::id()],
        );
        // `_NET_WM_PID` needs the host name in `WM_CLIENT_MACHINE`.
        if let Ok(host) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
            connection.set_property8(
                self.id,
                atoms.wm_client_machine,
                XCB_ATOM_STRING,
                host.trim().as_bytes(),
            );
        }
        connection.set_property32(
            self.id,
            atoms.net_wm_window_type,
            XCB_ATOM_ATOM,
            &[atoms.net_wm_window_type_normal],
        );
        self.write_hints(client);
    }

    fn write_title(&self, client: &Client, title: &str) {
        let connection = &client.connection;
        connection.set_property8(
            self.id,
            client.atoms.net_wm_name,
            client.atoms.utf8_string,
            title.as_bytes(),
        );
        connection.set_property8(
            self.id,
            XCB_ATOM_WM_NAME,
            client.atoms.utf8_string,
            title.as_bytes(),
        );
    }

    fn write_hints(&self, client: &Client) {
        let minimum = self.shared.minimum;
        let minimum = if minimum.width > 0.0 && minimum.height > 0.0 {
            pixel_size(minimum, self.shared.scale.get())
        } else {
            (1, 1)
        };
        let hints = protocol::size_hints(minimum, self.state.borrow().placed);
        client.connection.set_property32(
            self.id,
            XCB_ATOM_WM_NORMAL_HINTS,
            XCB_ATOM_WM_SIZE_HINTS,
            &hints,
        );
    }

    /// Paints the pending region and copies it to the server, unless
    /// the window is unmapped or its shared image is busy.
    pub(crate) fn paint_if_ready(&self, client: &Client) {
        if self.shared.closed.get() {
            return;
        }
        {
            let state = self.state.borrow();
            if state.destroyed || !state.mapped || state.busy {
                return;
            }
        }
        let Some(region) = self.shared.take_pending() else {
            return;
        };
        let start = Instant::now();
        let scale = self.shared.scale.get();
        let mut state = self.state.borrow_mut();
        let state = &mut *state;
        let (width, height) = state.pixels;
        let bounds = self.shared.bounds();
        let shm = client.shm_enabled();
        let reuse = state.image.as_ref().is_some_and(|image| {
            image.width == width
                && image.height == height
                && matches!(image.memory, Memory::Shared { .. }) == shm
        });
        let region = if reuse {
            region.intersection(bounds).unwrap_or(bounds)
        } else {
            if let Some(Image {
                memory: Memory::Shared { id, .. },
                ..
            }) = state.image.take()
            {
                client.connection.shm_detach(id);
            }
            state.image = Some(Self::new_image(client, width, height, shm));
            bounds
        };
        let Some(image) = state.image.as_mut() else {
            return;
        };
        let list = Rc::clone(&self.shared.paint.borrow().list);
        let stride = width as usize * 4;
        let mut pixels = Pixels {
            bytes: image.bytes_mut(),
            width,
            height,
            stride,
        };
        paint::paint(&list, &mut pixels, scale, region);
        let rect = pixel_rect(region, scale, (width, height));
        let depth = client.connection.screen().root_depth;
        match &image.memory {
            Memory::Shared { id, .. } => {
                client.connection.shm_put_image(
                    (self.id, self.gc),
                    (u16_of(width), u16_of(height)),
                    rect,
                    depth,
                    *id,
                );
                state.busy = true;
            }
            Memory::Local(bytes) => {
                put_rows(
                    &client.connection,
                    (self.id, self.gc),
                    depth,
                    (bytes, stride),
                    rect,
                );
            }
        }
        self.shared.visible.set(true);
        self.shared.painted(start);
    }

    /// A new image: shared when MIT-SHM works and a segment is available.
    fn new_image(client: &Client, width: u32, height: u32, shm: bool) -> Image {
        let length = width as usize * height as usize * 4;
        let shared = shm.then(|| SysvSegment::new(length).ok()).flatten();
        let memory = match shared {
            Some(segment) => {
                let id = client.connection.generate_id();
                client.connection.shm_attach(id, segment.id());
                Memory::Shared { segment, id }
            }
            None => Memory::Local(vec![0; length]),
        };
        Image {
            memory,
            width,
            height,
        }
    }

    /// Handles the completion of a shared `PutImage`.
    pub(crate) fn completed(&self) {
        self.state.borrow_mut().busy = false;
    }

    /// Drops the shared image after MIT-SHM failed, and repaints.
    pub(crate) fn drop_shared_image(&self) {
        let mut state = self.state.borrow_mut();
        state.busy = false;
        if state
            .image
            .as_ref()
            .is_some_and(|image| matches!(image.memory, Memory::Shared { .. }))
        {
            state.image = None;
        }
        drop(state);
        self.shared.invalidate();
    }

    /// Handles `Expose`: the region needs its pixels again.
    pub(crate) fn exposed(&self, (x, y, width, height): (u16, u16, u16, u16)) {
        let scale = self.shared.scale.get();
        let region = Rect::new(
            Point::new(f64::from(x) / scale, f64::from(y) / scale),
            Size::new(f64::from(width) / scale, f64::from(height) / scale),
        );
        let mut paint = self.shared.paint.borrow_mut();
        paint.pending = Some(match paint.pending {
            Some(pending) => pending.union(region),
            None => region,
        });
    }

    /// Handles `ConfigureNotify` with the size in pixels.
    pub(crate) fn configured(&self, width: u16, height: u16) {
        let pixels = (u32::from(width), u32::from(height));
        let scale = self.shared.scale.get();
        {
            let mut state = self.state.borrow_mut();
            if state.pixels == pixels {
                return;
            }
            let whole = scale.round().max(1.0) as u32;
            if state
                .machine
                .configure_notify(pixels.0, pixels.1, whole)
                .is_err()
            {
                return;
            }
            state.pixels = pixels;
        }
        let size = Size::new(f64::from(width) / scale, f64::from(height) / scale);
        if self.shared.size.get() != size {
            self.shared.size.set(size);
            self.shared.push(WindowEvent::Resized { size, scale });
        }
        self.shared.invalidate();
    }

    /// Handles `MapNotify`. The first one reports the size and activates
    /// the window.
    pub(crate) fn mapped(&self, client: &Client) {
        let first = {
            let mut state = self.state.borrow_mut();
            state.mapped = true;
            !std::mem::replace(&mut state.was_mapped, true)
        };
        if first {
            self.shared.push(WindowEvent::Resized {
                size: self.shared.size.get(),
                scale: self.shared.scale.get(),
            });
            client.activate(self.id);
        }
        self.shared.invalidate();
    }

    /// Handles `UnmapNotify`, as when the window is minimized.
    pub(crate) fn unmapped(&self) {
        self.state.borrow_mut().mapped = false;
        self.shared.visible.set(false);
    }

    /// Handles `VisibilityNotify`.
    pub(crate) fn visibility(&self, occluded: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            std::mem::replace(&mut state.occluded, occluded) != occluded
        };
        if changed {
            self.shared.push(WindowEvent::Occluded(occluded));
        }
    }

    /// Applies a new scale: the window keeps its logical size, so its
    /// pixel size changes.
    pub(crate) fn scale_changed(&self, client: &Client, scale: f64) {
        let shared = &self.shared;
        shared.scale.set(scale);
        shared.push(WindowEvent::ScaleChanged { scale });
        shared.push(WindowEvent::Resized {
            size: shared.size.get(),
            scale,
        });
        let (width, height) = pixel_size(shared.size.get(), scale);
        client.connection.configure_window(
            self.id,
            XCB_CONFIG_WINDOW_WIDTH | XCB_CONFIG_WINDOW_HEIGHT,
            &[width, height],
        );
        self.write_hints(client);
        shared.invalidate();
    }

    fn client(&self) -> Option<Rc<Client>> {
        if self.state.borrow().destroyed {
            return None;
        }
        self.client.upgrade()
    }

    /// The frame extents of the window manager: left, right, top, and
    /// bottom, in pixels.
    fn extents(&self, client: &Client) -> [u32; 4] {
        let values = client.connection.get_property32(
            self.id,
            client.atoms.net_frame_extents,
            XCB_ATOM_CARDINAL,
        );
        match values.as_slice() {
            [left, right, top, bottom, ..] => [*left, *right, *top, *bottom],
            _ => [0; 4],
        }
    }

    /// Adds or removes `_NET_WM_STATE` atoms: through the window manager
    /// when the window is mapped, and in the property before.
    fn change_state(&self, client: &Client, add: bool, atoms: [u32; 2]) {
        let connection = &client.connection;
        let property = client.atoms.net_wm_state;
        if self.state.borrow().shown {
            let message = protocol::client_message(
                self.id,
                property,
                [u32::from(add), atoms[0], atoms[1], 1, 0],
            );
            client.send_to_root(&message);
            return;
        }
        let mut states = connection.get_property32(self.id, property, XCB_ATOM_ATOM);
        states.retain(|state| !atoms.contains(state));
        if add {
            states.extend(atoms.iter().copied().filter(|atom| *atom != 0));
        }
        connection.set_property32(self.id, property, XCB_ATOM_ATOM, &states);
    }

    /// Asks the window manager to move or resize the window with the
    /// pointer.
    fn move_resize(&self, direction: u32) {
        let Some(client) = self.client() else {
            return;
        };
        let (x, y) = client.root_position();
        client.connection.ungrab_pointer();
        let message = protocol::client_message(
            self.id,
            client.atoms.net_wm_moveresize,
            [x.cast_unsigned(), y.cast_unsigned(), direction, 1, 1],
        );
        client.send_to_root(&message);
    }
}

impl Surface for X11Surface {
    fn show(&self) {
        let Some(client) = self.client() else {
            return;
        };
        {
            let mut state = self.state.borrow_mut();
            if state.shown || state.machine.map().is_err() {
                return;
            }
            state.shown = true;
        }
        client.connection.map_window(self.id);
    }

    fn set_title(&self, title: &str) {
        if let Some(client) = self.client() {
            self.write_title(&client, title);
        }
    }

    fn set_cursor(&self, shape: &CursorShape) -> Result<(), Error> {
        let client = self.client().ok_or(Error::Closed)?;
        let custom = matches!(shape, CursorShape::Custom { .. });
        let cursor = client
            .cursor(shape)
            .or_else(|| client.cursor(&CursorShape::Arrow));
        if let Some(cursor) = cursor {
            client
                .connection
                .change_window_attributes(self.id, XCB_CW_CURSOR, &[cursor]);
            // The window keeps its cursor after the free.
            if custom {
                client.connection.free_cursor(cursor);
            }
        }
        Ok(())
    }

    fn request_paint(&self) {
        // The loop paints the pending region before it waits again.
    }

    fn set_content_size(&self, size: Size) {
        let Some(client) = self.client() else {
            return;
        };
        let shared = &self.shared;
        let size = Size::new(
            size.width.max(shared.minimum.width),
            size.height.max(shared.minimum.height),
        );
        let (width, height) = pixel_size(size, shared.scale.get());
        // `ConfigureNotify` reports the size that applies.
        client.connection.configure_window(
            self.id,
            XCB_CONFIG_WINDOW_WIDTH | XCB_CONFIG_WINDOW_HEIGHT,
            &[width, height],
        );
    }

    fn geometry(&self) -> WindowGeometry {
        let Some(client) = self.client() else {
            return WindowGeometry {
                frame: self.shared.bounds(),
                zoomed: false,
                full_screen: false,
            };
        };
        let connection = &client.connection;
        let scale = self.shared.scale.get();
        let (x, y) = connection
            .translate(self.id, connection.root())
            .unwrap_or((0, 0));
        let (width, height) = connection
            .get_size(self.id)
            .map_or(self.state.borrow().pixels, |(width, height)| {
                (u32::from(width), u32::from(height))
            });
        let [left, right, top, bottom] = self.extents(&client);
        let frame = Rect::new(
            Point::new(
                (f64::from(x) - f64::from(left)) / scale,
                (f64::from(y) - f64::from(top)) / scale,
            ),
            Size::new(
                f64::from(width + left + right) / scale,
                f64::from(height + top + bottom) / scale,
            ),
        );
        let atoms = &client.atoms;
        let states = connection.get_property32(self.id, atoms.net_wm_state, XCB_ATOM_ATOM);
        WindowGeometry {
            frame,
            zoomed: states.contains(&atoms.net_wm_state_maximized_vert)
                && states.contains(&atoms.net_wm_state_maximized_horz),
            full_screen: states.contains(&atoms.net_wm_state_fullscreen),
        }
    }

    fn set_geometry(&self, geometry: &WindowGeometry) {
        let Some(client) = self.client() else {
            return;
        };
        let scale = self.shared.scale.get();
        let [left, right, top, bottom] = self.extents(&client);
        let frame = geometry.frame;
        let side = |points: f64, decorations: u32| {
            ((points * scale).round() - f64::from(decorations)).clamp(1.0, f64::from(u16::MAX))
                as u32
        };
        let width = side(frame.size.width, left + right);
        let height = side(frame.size.height, top + bottom);
        // With north-west gravity, the window manager puts the top-left
        // corner of the frame at the position.
        let x = (frame.origin.x * scale).round() as i32;
        let y = (frame.origin.y * scale).round() as i32;
        client.connection.configure_window(
            self.id,
            XCB_CONFIG_WINDOW_X
                | XCB_CONFIG_WINDOW_Y
                | XCB_CONFIG_WINDOW_WIDTH
                | XCB_CONFIG_WINDOW_HEIGHT,
            &[x.cast_unsigned(), y.cast_unsigned(), width, height],
        );
        let shown = {
            let mut state = self.state.borrow_mut();
            state.placed = true;
            state.shown
        };
        if !shown {
            self.write_hints(&client);
        }
        let atoms = &client.atoms;
        let current = self.geometry();
        if current.zoomed != geometry.zoomed {
            self.change_state(
                &client,
                geometry.zoomed,
                [
                    atoms.net_wm_state_maximized_vert,
                    atoms.net_wm_state_maximized_horz,
                ],
            );
        }
        if current.full_screen != geometry.full_screen {
            self.change_state(
                &client,
                geometry.full_screen,
                [atoms.net_wm_state_fullscreen, 0],
            );
        }
    }

    fn text_input_changed(&self) {
        if let Some(client) = self.client() {
            client.text_input_changed(self);
        }
    }

    fn close(&self) {
        let Some(client) = self.client() else {
            return;
        };
        let image = {
            let mut state = self.state.borrow_mut();
            state.destroyed = true;
            state.mapped = false;
            state.image.take()
        };
        let connection = &client.connection;
        if let Some(Image {
            memory: Memory::Shared { id, .. },
            ..
        }) = image
        {
            connection.shm_detach(id);
        }
        connection.free_gc(self.gc);
        connection.destroy_window(self.id);
        client.forget_window(self.id);
        connection.flush();
    }

    fn begin_move(&self) {
        self.move_resize(MOVERESIZE_MOVE);
    }

    fn begin_resize(&self, edge: ResizeEdge) {
        self.move_resize(protocol::moveresize_direction(edge));
    }

    /// Sends `_GTK_SHOW_WINDOW_MENU`, which GNOME, KDE, and other window
    /// managers handle. Others ignore it, and no menu shows.
    fn show_window_menu(&self, position: Point) {
        let Some(client) = self.client() else {
            return;
        };
        let connection = &client.connection;
        let scale = self.shared.scale.get();
        let (x, y) = connection
            .translate(self.id, connection.root())
            .unwrap_or((0, 0));
        let x = i32::from(x) + (position.x * scale).round() as i32;
        let y = i32::from(y) + (position.y * scale).round() as i32;
        connection.ungrab_pointer();
        let message = protocol::client_message(
            self.id,
            client.atoms.gtk_show_window_menu,
            [
                u32::from(client.pointer_device()),
                x.cast_unsigned(),
                y.cast_unsigned(),
                0,
                0,
            ],
        );
        client.send_to_root(&message);
    }

    fn set_maximized(&self, maximized: bool) {
        if let Some(client) = self.client() {
            let atoms = &client.atoms;
            self.change_state(
                &client,
                maximized,
                [
                    atoms.net_wm_state_maximized_vert,
                    atoms.net_wm_state_maximized_horz,
                ],
            );
        }
    }

    /// Asks the window manager to iconify the window with
    /// `WM_CHANGE_STATE`.
    fn minimize(&self) {
        if let Some(client) = self.client() {
            const ICONIC_STATE: u32 = 3;
            let message = protocol::client_message(
                self.id,
                client.atoms.wm_change_state,
                [ICONIC_STATE, 0, 0, 0, 0],
            );
            client.send_to_root(&message);
        }
    }

    fn server_decorations(&self) -> bool {
        // X11 window managers draw the frame of a normal window.
        true
    }

    /// Starts an XDND drag. The pointer shows no preview image.
    fn begin_drag(
        &self,
        payload: crate::native::transfer::Payload,
        allowed: choscorui_core::DragOperations,
        _preview: Option<&choscorui_core::Image>,
    ) -> Result<(), Error> {
        let client = self.client().ok_or(Error::Closed)?;
        client.begin_drag(self, payload, allowed)
    }

    fn simulate_pointer(&self, position: Point, button: Option<bool>) -> Result<(), Error> {
        let client = self.client().ok_or(Error::Closed)?;
        client.simulate_pointer(self.id, position, button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rects_round_out_and_clamp() {
        let region = Rect::new(Point::new(10.2, 5.0), Size::new(20.0, 10.6));
        assert_eq!(pixel_rect(region, 1.0, (100, 100)), (10, 5, 21, 11));
        assert_eq!(pixel_rect(region, 2.0, (40, 100)), (20, 10, 20, 22));
    }

    #[test]
    fn pixel_sizes_round_and_stay_positive() {
        assert_eq!(pixel_size(Size::new(320.0, 200.0), 1.5), (480, 300));
        assert_eq!(pixel_size(Size::new(0.1, 0.1), 1.0), (1, 1));
    }
}
