//! An `xdg_toplevel` window.
//!
//! `WaylandSurface` (the crate's configure state machine) decides when a
//! buffer may be committed: after the initial commit, and only for the
//! latest acknowledged configure. `FrameScheduler` paces the paints with
//! `wl_surface.frame`.

use super::shm::Buffers;
use super::{Client, Role};
use crate::ffi::wayland::{Arg, ObjectId, Proxy};
use crate::gles::{self, GlesError, GlesWindow};
use crate::native::paint::{self, Pixels};
use crate::native::types::{Error, Painter, ResizeEdge, WindowEvent};
use crate::native::window::{Surface, WindowShared};
use crate::{FrameScheduler, FrameToken, WaylandSurface};
use choscorui_core::{CursorShape, Point, Rect, Size, WindowGeometry};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

// wl_surface requests.
const SURFACE_ATTACH: u16 = 1;
const SURFACE_FRAME: u16 = 3;
const SURFACE_COMMIT: u16 = 6;
const SURFACE_SET_BUFFER_SCALE: u16 = 8;
const SURFACE_DAMAGE_BUFFER: u16 = 9;
// xdg_surface requests.
const XDG_SURFACE_DESTROY: u16 = 0;
const XDG_SURFACE_GET_TOPLEVEL: u16 = 1;
const XDG_SURFACE_SET_WINDOW_GEOMETRY: u16 = 3;
const XDG_SURFACE_ACK_CONFIGURE: u16 = 4;
// xdg_toplevel requests.
const TOPLEVEL_DESTROY: u16 = 0;
const TOPLEVEL_SET_PARENT: u16 = 1;
const TOPLEVEL_SET_TITLE: u16 = 2;
const TOPLEVEL_SET_APP_ID: u16 = 3;
const TOPLEVEL_SHOW_WINDOW_MENU: u16 = 4;
const TOPLEVEL_MOVE: u16 = 5;
const TOPLEVEL_RESIZE: u16 = 6;
const TOPLEVEL_SET_MIN_SIZE: u16 = 8;
const TOPLEVEL_SET_MAXIMIZED: u16 = 9;
const TOPLEVEL_UNSET_MAXIMIZED: u16 = 10;
const TOPLEVEL_SET_FULLSCREEN: u16 = 11;
const TOPLEVEL_UNSET_FULLSCREEN: u16 = 12;
const TOPLEVEL_SET_MINIMIZED: u16 = 13;
// xdg_toplevel states.
const STATE_MAXIMIZED: u32 = 1;
const STATE_FULLSCREEN: u32 = 2;
const STATE_ACTIVATED: u32 = 4;
// zxdg_toplevel_decoration_v1.
const DECORATION_DESTROY: u16 = 0;
const DECORATION_SET_MODE: u16 = 1;
const MODE_SERVER_SIDE: u32 = 2;
// xdg_wm_dialog_v1 and xdg_dialog_v1.
const WM_DIALOG_GET_XDG_DIALOG: u16 = 1;
const DIALOG_DESTROY: u16 = 0;
const DIALOG_SET_MODAL: u16 = 1;
// wp_viewport.
const VIEWPORT_SET_DESTINATION: u16 = 2;

/// A size and states from `xdg_toplevel.configure`, waiting for the
/// `xdg_surface.configure` that applies them.
#[derive(Clone, Copy, Default)]
struct PendingConfigure {
    width: i32,
    height: i32,
    maximized: bool,
    full_screen: bool,
    activated: bool,
}

pub(crate) struct Inner {
    protocol: WaylandSurface,
    frames: FrameScheduler,
    frame: Option<(Proxy, FrameToken)>,
    buffers: Buffers,
    pending: PendingConfigure,
    maximized: bool,
    full_screen: bool,
    pub(crate) server_side: bool,
    /// The scale from `wp_fractional_scale_v1`, in 120ths.
    fractional: Option<u32>,
    /// The integer scale from `preferred_buffer_scale` or the outputs.
    integer: i32,
    /// The outputs that the surface is on.
    outputs: Vec<ObjectId>,
    /// The committed buffer scale, to avoid sending it again.
    buffer_scale: i32,
    mapped: bool,
}

pub(crate) struct WaylandWindow {
    me: Weak<WaylandWindow>,
    client: Weak<Client>,
    pub(crate) shared: Rc<WindowShared>,
    /// The EGL surface while the window paints with GLES.
    gles: RefCell<Option<GlesWindow>>,
    pub(crate) surface: Rc<Proxy>,
    xdg_surface: RefCell<Option<Proxy>>,
    pub(crate) toplevel: RefCell<Option<Proxy>>,
    decoration: RefCell<Option<Proxy>>,
    fractional: RefCell<Option<Proxy>>,
    /// The `zxdg_exported_v2` of the toplevel, for the parent of portal
    /// dialogs.
    pub(crate) exported: RefCell<Option<Proxy>>,
    /// The `xdg_dialog_v1` of a modal dialog window.
    dialog: RefCell<Option<Proxy>>,
    viewport: RefCell<Option<Proxy>>,
    pub(crate) inner: RefCell<Inner>,
    /// Set by `request_paint`; the loop paints at the next frame.
    wants_paint: Cell<bool>,
}

fn protocol_error(error: impl std::fmt::Display) -> Error {
    Error::Connection(error.to_string())
}

impl WaylandWindow {
    pub(crate) fn new(
        client: &Rc<Client>,
        shared: Rc<WindowShared>,
        app_id: &str,
    ) -> Result<Rc<Self>, Error> {
        let globals = client.globals.borrow();
        let protocol = client
            .connection_state
            .borrow()
            .create_toplevel()
            .map_err(|error| Error::Connection(format!("{error:?}")))?;
        let compositor = globals
            .compositor
            .as_ref()
            .ok_or(Error::MissingProtocol("wl_compositor"))?;
        let wm_base = globals
            .wm_base
            .as_ref()
            .ok_or(Error::MissingProtocol("xdg_wm_base"))?;
        let surface = compositor
            .request(0, &[Arg::NewId])
            .map_err(protocol_error)?
            .ok_or(Error::MissingProtocol("wl_surface"))?;
        let xdg_surface = wm_base
            .request(2, &[Arg::NewId, Arg::Object(Some(&surface))])
            .map_err(protocol_error)?
            .ok_or(Error::MissingProtocol("xdg_surface"))?;
        let toplevel = xdg_surface
            .request(XDG_SURFACE_GET_TOPLEVEL, &[Arg::NewId])
            .map_err(protocol_error)?
            .ok_or(Error::MissingProtocol("xdg_toplevel"))?;
        let title = shared.title.borrow().clone();
        toplevel
            .request(TOPLEVEL_SET_TITLE, &[Arg::Str(Some(&title))])
            .map_err(protocol_error)?;
        if !app_id.is_empty() {
            toplevel
                .request(TOPLEVEL_SET_APP_ID, &[Arg::Str(Some(app_id))])
                .map_err(protocol_error)?;
        }
        let minimum = shared.minimum;
        if minimum.width > 0.0 || minimum.height > 0.0 {
            toplevel
                .request(
                    TOPLEVEL_SET_MIN_SIZE,
                    &[
                        Arg::Int(minimum.width.ceil() as i32),
                        Arg::Int(minimum.height.ceil() as i32),
                    ],
                )
                .map_err(protocol_error)?;
        }
        // Server-side decorations when the compositor grants them, unless
        // the environment asks for client-side ones.
        let wants_client =
            std::env::var("CHOSCORUI_DECORATIONS").is_ok_and(|value| value == "client");
        let decoration = match (&globals.decoration_manager, wants_client) {
            (Some(manager), false) => {
                let decoration = manager
                    .request(1, &[Arg::NewId, Arg::Object(Some(&toplevel))])
                    .map_err(protocol_error)?;
                if let Some(decoration) = &decoration {
                    decoration
                        .request(DECORATION_SET_MODE, &[Arg::Uint(MODE_SERVER_SIDE)])
                        .map_err(protocol_error)?;
                }
                decoration
            }
            _ => None,
        };
        let fractional = match &globals.fractional_scale {
            Some(manager) if globals.viewporter.is_some() => manager
                .request(1, &[Arg::NewId, Arg::Object(Some(&surface))])
                .map_err(protocol_error)?,
            _ => None,
        };
        let viewport = match (&fractional, &globals.viewporter) {
            (Some(_), Some(viewporter)) => viewporter
                .request(1, &[Arg::NewId, Arg::Object(Some(&surface))])
                .map_err(protocol_error)?,
            _ => None,
        };
        drop(globals);
        let window = Rc::new_cyclic(|me| Self {
            me: me.clone(),
            client: Rc::downgrade(client),
            shared,
            gles: RefCell::new(None),
            surface: Rc::new(surface),
            xdg_surface: RefCell::new(Some(xdg_surface)),
            toplevel: RefCell::new(Some(toplevel)),
            decoration: RefCell::new(decoration),
            fractional: RefCell::new(fractional),
            exported: RefCell::new(None),
            dialog: RefCell::new(None),
            viewport: RefCell::new(viewport),
            inner: RefCell::new(Inner {
                protocol,
                frames: FrameScheduler::new(),
                frame: None,
                buffers: Buffers::default(),
                pending: PendingConfigure::default(),
                maximized: false,
                full_screen: false,
                server_side: false,
                fractional: None,
                integer: 1,
                outputs: Vec::new(),
                buffer_scale: 1,
                mapped: false,
            }),
            wants_paint: Cell::new(true),
        });
        let mut routes = client.routes.borrow_mut();
        let weak = Rc::downgrade(&window);
        routes.insert(window.surface.id(), Role::Surface(weak.clone()));
        if let Some(proxy) = &*window.xdg_surface.borrow() {
            routes.insert(proxy.id(), Role::XdgSurface(weak.clone()));
        }
        if let Some(proxy) = &*window.toplevel.borrow() {
            routes.insert(proxy.id(), Role::Toplevel(weak.clone()));
        }
        if let Some(proxy) = &*window.decoration.borrow() {
            routes.insert(proxy.id(), Role::Decoration(weak.clone()));
        }
        if let Some(proxy) = &*window.fractional.borrow() {
            routes.insert(proxy.id(), Role::FractionalScale(weak));
        }
        Ok(window)
    }

    /// The scale of the buffers.
    pub(crate) fn scale(&self) -> f64 {
        let inner = self.inner.borrow();
        match inner.fractional {
            Some(scale) => f64::from(scale) / 120.0,
            None => f64::from(inner.integer.max(1)),
        }
    }

    /// Handles `xdg_toplevel.configure`.
    pub(crate) fn toplevel_configure(&self, width: i32, height: i32, states: &[u8]) {
        let mut pending = PendingConfigure {
            width,
            height,
            ..PendingConfigure::default()
        };
        for state in states.as_chunks::<4>().0 {
            match u32::from_ne_bytes(*state) {
                STATE_MAXIMIZED => pending.maximized = true,
                STATE_FULLSCREEN => pending.full_screen = true,
                STATE_ACTIVATED => pending.activated = true,
                _ => {}
            }
        }
        self.inner.borrow_mut().pending = pending;
    }

    /// Handles `xdg_surface.configure`: applies the pending size and
    /// states, and acknowledges the serial.
    pub(crate) fn surface_configure(&self, serial: u32) {
        let shared = &self.shared;
        let current = shared.size.get();
        let mut inner = self.inner.borrow_mut();
        let pending = inner.pending;
        // A zero size lets the client choose: keep the current size.
        let width = if pending.width > 0 {
            f64::from(pending.width)
        } else {
            current.width
        };
        let height = if pending.height > 0 {
            f64::from(pending.height)
        } else {
            current.height
        };
        let size = Size::new(
            width.max(shared.minimum.width).max(1.0),
            height.max(shared.minimum.height).max(1.0),
        );
        let integer = u32::try_from(inner.integer.max(1)).unwrap_or(1);
        let accepted = inner
            .protocol
            .receive_configure(serial, size.width as u32, size.height as u32, integer)
            .and_then(|()| inner.protocol.ack_configure(serial));
        if accepted.is_err() {
            return;
        }
        inner.maximized = pending.maximized;
        inner.full_screen = pending.full_screen;
        let first = !inner.mapped;
        inner.mapped = true;
        drop(inner);
        if let Some(xdg_surface) = &*self.xdg_surface.borrow() {
            let _ = xdg_surface.request(XDG_SURFACE_ACK_CONFIGURE, &[Arg::Uint(serial)]);
        }
        if first || size != current {
            shared.size.set(size);
            shared.push(WindowEvent::Resized {
                size,
                scale: self.scale(),
            });
            shared.invalidate();
        }
        if shared.focused.get() != pending.activated {
            shared.focused.set(pending.activated);
            shared.push(WindowEvent::Focused(pending.activated));
        }
        self.wants_paint.set(true);
    }

    /// Handles `zxdg_toplevel_decoration_v1.configure`.
    pub(crate) fn decoration_mode(&self, mode: u32) {
        self.inner.borrow_mut().server_side = mode == MODE_SERVER_SIDE;
    }

    /// Handles `wp_fractional_scale_v1.preferred_scale`.
    pub(crate) fn fractional_scale(&self, scale: u32) {
        if scale == 0 {
            return;
        }
        let changed = self.inner.borrow_mut().fractional.replace(scale) != Some(scale);
        if changed {
            self.scale_changed();
        }
    }

    /// Handles `wl_surface.preferred_buffer_scale`.
    pub(crate) fn preferred_buffer_scale(&self, scale: i32) {
        let changed = {
            let mut inner = self.inner.borrow_mut();
            let old = std::mem::replace(&mut inner.integer, scale.max(1));
            old != inner.integer && inner.fractional.is_none()
        };
        if changed {
            self.scale_changed();
        }
    }

    /// Handles `wl_surface.enter` and `leave`: without the preferred
    /// scale event, the integer scale is the largest of the outputs.
    pub(crate) fn output_changed(&self, output: ObjectId, entered: bool, client: &Client) {
        let changed = {
            let mut inner = self.inner.borrow_mut();
            if entered {
                inner.outputs.push(output);
            } else {
                inner.outputs.retain(|&id| id != output);
            }
            if self.surface.version() >= 6 {
                return;
            }
            let scale = inner
                .outputs
                .iter()
                .filter_map(|&id| client.output_scale(id))
                .max()
                .unwrap_or(1);
            let old = std::mem::replace(&mut inner.integer, scale);
            old != scale && inner.fractional.is_none()
        };
        if changed {
            self.scale_changed();
        }
    }

    fn scale_changed(&self) {
        let scale = self.scale();
        let shared = &self.shared;
        shared.scale.set(scale);
        shared.push(WindowEvent::ScaleChanged { scale });
        shared.push(WindowEvent::Resized {
            size: shared.size.get(),
            scale,
        });
        shared.invalidate();
        self.wants_paint.set(true);
    }

    /// Handles `wl_callback.done` of the frame callback.
    pub(crate) fn frame_done(&self, callback: ObjectId) {
        let mut inner = self.inner.borrow_mut();
        if let Some((proxy, token)) = inner.frame.take() {
            if proxy.id() == callback {
                let _ = inner.frames.presented(token);
            } else {
                inner.frame = Some((proxy, token));
            }
        }
    }

    /// Handles `wl_buffer.release`.
    pub(crate) fn buffer_released(&self, buffer: ObjectId) -> bool {
        self.inner.borrow_mut().buffers.release(buffer)
    }

    /// Paints when content is pending, a configure was acknowledged, no
    /// frame is pending, and a buffer is free. Returns true when it
    /// committed.
    pub(crate) fn paint_if_ready(&self, client: &Client) -> bool {
        if self.shared.closed.get() {
            return false;
        }
        // A GPU failure in another window switches this one at its next
        // paint.
        let device = match self.shared.paint.borrow().painter {
            Painter::Gles => client.gles(),
            Painter::Cairo => None,
        };
        if device.is_none() && self.shared.paint.borrow().painter == Painter::Gles {
            self.use_cairo();
        }
        {
            let inner = self.inner.borrow();
            let buffer_free = device.is_some() || inner.buffers.has_free();
            if !inner.mapped || inner.frames.is_pending() || !buffer_free {
                return false;
            }
        }
        let wants = self.wants_paint.get() || self.shared.paint.borrow().pending.is_some();
        if !wants {
            return false;
        }
        let Some(region) = self.shared.take_pending() else {
            // A configure without new content: commit to apply it.
            self.wants_paint.set(false);
            return self.commit_without_paint();
        };
        self.wants_paint.set(false);
        match device {
            Some(device) => self.paint_gles(client, &device, region),
            None => self.paint(client, region),
        }
    }

    fn commit_without_paint(&self) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.protocol.commit_buffer().is_err() {
            return false;
        }
        drop(inner);
        let _ = self.surface.request(SURFACE_COMMIT, &[]);
        true
    }

    /// The buffer size in pixels: the logical size times the scale.
    fn pixel_size(&self) -> (u32, u32) {
        let (scale, size) = (self.scale(), self.shared.size.get());
        (
            (size.width * scale).round().max(1.0) as u32,
            (size.height * scale).round().max(1.0) as u32,
        )
    }

    /// Switches the window to Cairo for the rest of its life, and asks
    /// for a full repaint.
    fn use_cairo(&self) {
        self.gles.borrow_mut().take();
        self.shared.paint.borrow_mut().painter = Painter::Cairo;
        // A frame callback of a failed swap never fires.
        let mut inner = self.inner.borrow_mut();
        if let Some((_, token)) = inner.frame.take() {
            let _ = inner.frames.presented(token);
        }
        drop(inner);
        self.shared.invalidate();
        self.wants_paint.set(true);
    }

    /// Paints with GLES. EGL attaches the buffer, damages it, and commits
    /// the surface in the swap.
    fn paint_gles(&self, client: &Client, device: &gles::Device, region: Rect) -> bool {
        let start = Instant::now();
        let (width, height) = self.pixel_size();
        if self.inner.borrow_mut().protocol.commit_buffer().is_err() {
            // Put the region back for the next configure.
            self.shared.paint.borrow_mut().pending = Some(region);
            return false;
        }
        let bounds = self.shared.bounds();
        let region = region.intersection(bounds).unwrap_or(bounds);
        let list = Rc::clone(&self.shared.paint.borrow().list);
        let mut slot = self.gles.borrow_mut();
        if slot.is_none() {
            match device.window(&self.surface, width, height) {
                Ok(window) => *slot = Some(window),
                Err(error) => {
                    drop(slot);
                    gles::mark_failed(&error);
                    self.use_cairo();
                    return false;
                }
            }
        }
        let Some(window) = slot.as_mut() else {
            return false;
        };
        let size = self.shared.size.get();
        let result =
            device.paint_window(window, &list, self.scale(), (width, height), region, || {
                self.frame_requests(client, &mut self.inner.borrow_mut(), size)
            });
        drop(slot);
        match result {
            Ok(()) => {}
            Err(GlesError::TextUnavailable) => {
                eprintln!("choscorui: painting failed: {}", GlesError::TextUnavailable);
            }
            Err(_) => {
                self.use_cairo();
                return false;
            }
        }
        self.shared.visible.set(true);
        self.shared.painted(start);
        true
    }

    /// Sends the size, the scale, and the frame callback of a frame. The
    /// commit that follows applies them.
    fn frame_requests(&self, client: &Client, inner: &mut Inner, size: Size) {
        match &*self.viewport.borrow() {
            Some(viewport) => {
                let _ = viewport.request(
                    VIEWPORT_SET_DESTINATION,
                    &[
                        Arg::Int(size.width.round() as i32),
                        Arg::Int(size.height.round() as i32),
                    ],
                );
            }
            None => {
                let scale = inner.integer.max(1);
                if scale != inner.buffer_scale {
                    inner.buffer_scale = scale;
                    let _ = self
                        .surface
                        .request(SURFACE_SET_BUFFER_SCALE, &[Arg::Int(scale)]);
                }
            }
        }
        if let Some(xdg_surface) = &*self.xdg_surface.borrow() {
            let _ = xdg_surface.request(
                XDG_SURFACE_SET_WINDOW_GEOMETRY,
                &[
                    Arg::Int(0),
                    Arg::Int(0),
                    Arg::Int(size.width.round() as i32),
                    Arg::Int(size.height.round() as i32),
                ],
            );
        }
        if let Ok(token) = inner.frames.request() {
            match self.surface.request(SURFACE_FRAME, &[Arg::NewId]) {
                Ok(Some(callback)) => {
                    client
                        .routes
                        .borrow_mut()
                        .insert(callback.id(), Role::Frame(self.me.clone()));
                    inner.frame = Some((callback, token));
                }
                _ => {
                    let _ = inner.frames.presented(token);
                }
            }
        }
    }

    /// Paints with Cairo into a shared-memory buffer.
    fn paint(&self, client: &Client, region: Rect) -> bool {
        let start = Instant::now();
        let scale = self.scale();
        let size = self.shared.size.get();
        let (width, height) = self.pixel_size();
        let globals = client.globals.borrow();
        let Some(shm) = globals.shm.as_ref() else {
            return false;
        };
        let bounds = self.shared.bounds();
        let mut inner = self.inner.borrow_mut();
        if inner.protocol.commit_buffer().is_err() {
            // Put the region back for the next configure.
            drop(inner);
            self.shared.paint.borrow_mut().pending = Some(region);
            return false;
        }
        let inner = &mut *inner;
        let Some(buffer) = inner.buffers.acquire(shm, width, height, bounds) else {
            self.shared.paint.borrow_mut().pending = Some(region);
            return false;
        };
        let region = match buffer.stale.take() {
            Some(stale) => stale.union(region),
            None => region,
        };
        let region = region.intersection(bounds).unwrap_or(bounds);
        let list = std::rc::Rc::clone(&self.shared.paint.borrow().list);
        let stride = buffer.stride();
        let mut pixels = Pixels {
            bytes: buffer.memory.bytes_mut(),
            width,
            height,
            stride,
        };
        paint::paint(&list, &mut pixels, scale, region);
        buffer.busy = true;
        let id = buffer.proxy.id();
        let _ = self.surface.request(
            SURFACE_ATTACH,
            &[Arg::Object(Some(&buffer.proxy)), Arg::Int(0), Arg::Int(0)],
        );
        inner.buffers.mark_stale(region, id);
        // Damage in buffer pixels, rounded out.
        let x = (region.origin.x * scale).floor() as i32;
        let y = (region.origin.y * scale).floor() as i32;
        let right = ((region.origin.x + region.size.width) * scale).ceil() as i32;
        let bottom = ((region.origin.y + region.size.height) * scale).ceil() as i32;
        let _ = self.surface.request(
            SURFACE_DAMAGE_BUFFER,
            &[
                Arg::Int(x),
                Arg::Int(y),
                Arg::Int(right - x),
                Arg::Int(bottom - y),
            ],
        );
        self.frame_requests(client, inner, size);
        drop(globals);
        let _ = self.surface.request(SURFACE_COMMIT, &[]);
        self.shared.visible.set(true);
        self.shared.painted(start);
        true
    }

    /// The serial of the most recent press, for move and resize.
    fn press_serial(&self) -> Option<(u32, Rc<Client>)> {
        let client = self.client.upgrade()?;
        let serial = client.last_press_serial()?;
        Some((serial, client))
    }

    /// Destroys the native objects in protocol order.
    fn destroy(&self) {
        if let Some(client) = self.client.upgrade() {
            client.forget_window(self.surface.id());
        }
        // EGL lets go of the surface before it is destroyed.
        self.gles.borrow_mut().take();
        self.inner.borrow_mut().frame = None;
        self.viewport.borrow_mut().take();
        self.fractional.borrow_mut().take();
        if let Some(decoration) = self.decoration.borrow_mut().take() {
            let _ = decoration.destroy(DECORATION_DESTROY, &[]);
        }
        if let Some(dialog) = self.dialog.borrow_mut().take() {
            let _ = dialog.destroy(DIALOG_DESTROY, &[]);
        }
        if let Some(toplevel) = self.toplevel.borrow_mut().take() {
            let _ = toplevel.destroy(TOPLEVEL_DESTROY, &[]);
        }
        if let Some(xdg_surface) = self.xdg_surface.borrow_mut().take() {
            let _ = xdg_surface.destroy(XDG_SURFACE_DESTROY, &[]);
        }
        let _ = self.surface.request(
            SURFACE_ATTACH,
            &[Arg::Object(None), Arg::Int(0), Arg::Int(0)],
        );
        let _ = self.surface.request(SURFACE_COMMIT, &[]);
        self.inner.borrow_mut().buffers.clear();
        if let Some(client) = self.client.upgrade() {
            let _ = client.connection.flush();
        }
    }
}

impl Surface for WaylandWindow {
    fn show(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner.protocol.initial_commit_sent() {
            return;
        }
        if inner.protocol.commit_initial().is_ok() {
            drop(inner);
            let _ = self.surface.request(SURFACE_COMMIT, &[]);
            if let Some(client) = self.client.upgrade() {
                client.activate_window(self);
                client.export_window(self);
            }
        }
    }

    fn set_title(&self, title: &str) {
        if let Some(toplevel) = &*self.toplevel.borrow() {
            let _ = toplevel.request(TOPLEVEL_SET_TITLE, &[Arg::Str(Some(title))]);
        }
    }

    fn set_cursor(&self, shape: &CursorShape) -> Result<(), Error> {
        match self.client.upgrade() {
            Some(client) => client.set_cursor(self.surface.id(), shape),
            None => Err(Error::Closed),
        }
    }

    fn request_paint(&self) {
        // The loop paints before it waits again.
        self.wants_paint.set(true);
    }

    fn set_content_size(&self, size: Size) {
        // Wayland lets the client choose its size outside of maximized,
        // full-screen, and tiled states.
        let shared = &self.shared;
        let size = Size::new(
            size.width.max(shared.minimum.width),
            size.height.max(shared.minimum.height),
        );
        if shared.size.get() == size {
            return;
        }
        shared.size.set(size);
        shared.push(WindowEvent::Resized {
            size,
            scale: self.scale(),
        });
        shared.invalidate();
        self.wants_paint.set(true);
    }

    fn geometry(&self) -> WindowGeometry {
        let inner = self.inner.borrow();
        WindowGeometry {
            frame: Rect::new(Point::new(0.0, 0.0), self.shared.size.get()),
            zoomed: inner.maximized,
            full_screen: inner.full_screen,
        }
    }

    fn set_geometry(&self, geometry: &WindowGeometry) {
        // Wayland clients cannot place their windows, so the origin is
        // ignored.
        self.set_content_size(geometry.frame.size);
        let (maximized, full_screen) = {
            let inner = self.inner.borrow();
            (inner.maximized, inner.full_screen)
        };
        if let Some(toplevel) = &*self.toplevel.borrow() {
            if geometry.zoomed != maximized {
                let opcode = if geometry.zoomed {
                    TOPLEVEL_SET_MAXIMIZED
                } else {
                    TOPLEVEL_UNSET_MAXIMIZED
                };
                let _ = toplevel.request(opcode, &[]);
            }
            if geometry.full_screen != full_screen {
                let _ = if geometry.full_screen {
                    toplevel.request(TOPLEVEL_SET_FULLSCREEN, &[Arg::Object(None)])
                } else {
                    toplevel.request(TOPLEVEL_UNSET_FULLSCREEN, &[])
                };
            }
        }
    }

    fn text_input_changed(&self) {
        if let Some(client) = self.client.upgrade() {
            client.text_input_changed(self.surface.id());
        }
    }

    fn close(&self) {
        self.destroy();
    }

    /// Starts an interactive move, from a pointer press on a client-side
    /// title bar.
    fn begin_move(&self) {
        let Some((serial, client)) = self.press_serial() else {
            return;
        };
        let seat = client.globals.borrow();
        if let (Some(toplevel), Some(seat)) = (&*self.toplevel.borrow(), &seat.seat) {
            let _ = toplevel.request(TOPLEVEL_MOVE, &[Arg::Object(Some(seat)), Arg::Uint(serial)]);
        }
    }

    fn begin_resize(&self, edge: ResizeEdge) {
        let Some((serial, client)) = self.press_serial() else {
            return;
        };
        let seat = client.globals.borrow();
        if let (Some(toplevel), Some(seat)) = (&*self.toplevel.borrow(), &seat.seat) {
            let _ = toplevel.request(
                TOPLEVEL_RESIZE,
                &[
                    Arg::Object(Some(seat)),
                    Arg::Uint(serial),
                    Arg::Uint(edge as u32),
                ],
            );
        }
    }

    fn show_window_menu(&self, position: Point) {
        let Some((serial, client)) = self.press_serial() else {
            return;
        };
        let seat = client.globals.borrow();
        if let (Some(toplevel), Some(seat)) = (&*self.toplevel.borrow(), &seat.seat) {
            let _ = toplevel.request(
                TOPLEVEL_SHOW_WINDOW_MENU,
                &[
                    Arg::Object(Some(seat)),
                    Arg::Uint(serial),
                    Arg::Int(position.x.round() as i32),
                    Arg::Int(position.y.round() as i32),
                ],
            );
        }
    }

    fn set_maximized(&self, maximized: bool) {
        if let Some(toplevel) = &*self.toplevel.borrow() {
            let opcode = if maximized {
                TOPLEVEL_SET_MAXIMIZED
            } else {
                TOPLEVEL_UNSET_MAXIMIZED
            };
            let _ = toplevel.request(opcode, &[]);
        }
    }

    fn minimize(&self) {
        if let Some(toplevel) = &*self.toplevel.borrow() {
            let _ = toplevel.request(TOPLEVEL_SET_MINIMIZED, &[]);
        }
    }

    fn server_decorations(&self) -> bool {
        self.inner.borrow().server_side
    }

    fn set_modal_parent(&self, parent: &Rc<WindowShared>) {
        let Some(client) = self.client.upgrade() else {
            return;
        };
        let parent = client
            .windows
            .borrow()
            .iter()
            .find(|window| Rc::ptr_eq(&window.shared, parent))
            .cloned();
        let toplevel = self.toplevel.borrow();
        let Some(toplevel) = toplevel.as_ref() else {
            return;
        };
        if let Some(parent) = parent
            && let Some(parent) = &*parent.toplevel.borrow()
        {
            let _ = toplevel.request(TOPLEVEL_SET_PARENT, &[Arg::Object(Some(parent))]);
        }
        let globals = client.globals.borrow();
        let mut dialog = self.dialog.borrow_mut();
        if let (Some(manager), None) = (&globals.wm_dialog, dialog.as_ref()) {
            *dialog = manager
                .request(
                    WM_DIALOG_GET_XDG_DIALOG,
                    &[Arg::NewId, Arg::Object(Some(toplevel))],
                )
                .ok()
                .flatten();
            if let Some(dialog) = dialog.as_ref() {
                let _ = dialog.request(DIALOG_SET_MODAL, &[]);
            }
        }
    }

    fn begin_drag(
        &self,
        payload: crate::native::transfer::Payload,
        allowed: choscorui_core::DragOperations,
        preview: Option<&choscorui_core::Image>,
    ) -> Result<(), Error> {
        let client = self.client.upgrade().ok_or(Error::Closed)?;
        client.begin_drag(self, payload, allowed, preview)
    }

    /// Wayland has no protocol for fake input of a client.
    fn simulate_pointer(&self, _position: Point, _button: Option<bool>) -> Result<(), Error> {
        Err(Error::MissingProtocol("fake pointer input"))
    }
}
