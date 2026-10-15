//! The clipboard and drag-and-drop on `wl_data_device`.
//!
//! The compositor announces each offer with `data_offer`, then its MIME
//! types, then uses it as the selection or as the offer of a drag. The
//! crate's `Clipboard` state machine tracks the offer and the ownership.
//! While the application owns the selection or runs a drag, it reads its
//! own payload directly, so a read never waits on its own event loop.

use super::super::transfer::{self, Payload};
use super::super::types::{Error, WindowEvent};
use super::super::window::WindowShared;
use super::{Client, Role, WaylandWindow};
use crate::ffi::libc::{self, Watch};
use crate::ffi::wayland::{Arg, Event, ObjectId, Proxy};
use choscorui_core::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, Image, Point, negotiate_drag,
};
use std::collections::HashMap;
use std::os::fd::{AsFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

// wl_data_device_manager requests.
const MANAGER_CREATE_DATA_SOURCE: u16 = 0;
const MANAGER_GET_DATA_DEVICE: u16 = 1;
// wl_data_device requests and events.
const DEVICE_START_DRAG: u16 = 0;
const DEVICE_SET_SELECTION: u16 = 1;
const DEVICE_DATA_OFFER: u16 = 0;
const DEVICE_ENTER: u16 = 1;
const DEVICE_LEAVE: u16 = 2;
const DEVICE_MOTION: u16 = 3;
const DEVICE_DROP: u16 = 4;
const DEVICE_SELECTION: u16 = 5;
// wl_data_offer requests and events.
const OFFER_ACCEPT: u16 = 0;
const OFFER_RECEIVE: u16 = 1;
const OFFER_DESTROY: u16 = 2;
const OFFER_FINISH: u16 = 3;
const OFFER_SET_ACTIONS: u16 = 4;
const OFFER_OFFER: u16 = 0;
const OFFER_SOURCE_ACTIONS: u16 = 1;
const OFFER_ACTION: u16 = 2;
// wl_data_source requests and events.
const SOURCE_OFFER: u16 = 0;
const SOURCE_SET_ACTIONS: u16 = 2;
const SOURCE_SEND: u16 = 1;
const SOURCE_CANCELLED: u16 = 2;
const SOURCE_DND_DROP_PERFORMED: u16 = 3;
const SOURCE_DND_FINISHED: u16 = 4;
const SOURCE_ACTION: u16 = 5;
// wl_data_device_manager.dnd_action values.
const ACTION_COPY: u32 = 1;
const ACTION_MOVE: u32 = 2;

/// How long a read waits for the other client to write its data.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// The Wayland drag actions of `operations`. Wayland has no link action.
fn actions(operations: DragOperations) -> u32 {
    (if operations.copy { ACTION_COPY } else { 0 })
        | (if operations.move_ { ACTION_MOVE } else { 0 })
}

fn operations(actions: u32) -> DragOperations {
    DragOperations {
        copy: actions & ACTION_COPY != 0,
        move_: actions & ACTION_MOVE != 0,
        link: false,
    }
}

fn operation(action: u32) -> DragOperation {
    match action {
        ACTION_MOVE => DragOperation::Move,
        ACTION_COPY => DragOperation::Copy,
        _ => DragOperation::None,
    }
}

/// An offer of another client, or of this one.
struct Offer {
    proxy: Proxy,
    mimes: Vec<String>,
    source_actions: u32,
    /// The action that the compositor chose for a drag.
    action: u32,
}

/// The drag over one of the windows.
struct Incoming {
    offer: Option<ObjectId>,
    window: Weak<WaylandWindow>,
    serial: u32,
    position: Point,
    /// The accepted formats, the allowed operations, and the preferred
    /// operation, or None when the window does not take the drag.
    accepted: Option<(Vec<DragFormat>, DragOperations, DragOperation)>,
}

/// A drag that a window of the application started.
struct Outgoing {
    source: Proxy,
    payload: Rc<Payload>,
    window: Weak<WindowShared>,
    action: u32,
    _icon: Option<(Proxy, super::shm::Buffers)>,
}

/// The data device state of the seat.
#[derive(Default)]
pub(crate) struct DataState {
    device: Option<Proxy>,
    offers: HashMap<ObjectId, Offer>,
    selection: Option<ObjectId>,
    /// The selection source of the application and its payload.
    source: Option<(Proxy, Rc<Payload>)>,
    machine: crate::Clipboard,
    incoming: Option<Incoming>,
    outgoing: Option<Outgoing>,
}

/// What the clipboard holds, as the application sees it.
pub(crate) enum Contents {
    Empty,
    Local(Rc<Payload>),
    Remote(ObjectId, Vec<String>),
}

impl Client {
    /// Gets the data device of the seat once both globals are bound.
    pub(super) fn ensure_data_device(&self) {
        let globals = self.globals.borrow();
        let (Some(manager), Some(seat)) = (&globals.data_device_manager, &globals.seat) else {
            return;
        };
        let mut data = self.data.borrow_mut();
        if data.device.is_some() {
            return;
        }
        data.device = manager
            .request(
                MANAGER_GET_DATA_DEVICE,
                &[Arg::NewId, Arg::Object(Some(seat))],
            )
            .ok()
            .flatten();
        if let Some(device) = &data.device {
            self.routes
                .borrow_mut()
                .insert(device.id(), Role::DataDevice);
        }
    }

    /// True when the seat has a data device.
    pub(crate) fn has_data_device(&self) -> bool {
        self.data.borrow().device.is_some()
    }

    /// Makes `payload` the selection. The compositor accepts it only from
    /// a client with recent input, so the last input serial goes with it.
    pub(crate) fn set_selection(&self, payload: Payload) -> Result<(), Error> {
        let mimes = payload.mime_types();
        let serial = self.seat.borrow().input_serial;
        let source = self.create_source(&mimes, None)?;
        let mut data = self.data.borrow_mut();
        let device = data.device.as_ref().ok_or(Error::ClipboardWriteFailed)?;
        device
            .request(
                DEVICE_SET_SELECTION,
                &[Arg::Object(Some(&source)), Arg::Uint(serial)],
            )
            .map_err(|_| Error::ClipboardWriteFailed)?;
        // A private payload has no MIME types, but the application still
        // owns the selection.
        let _ = data
            .machine
            .claim(mimes.iter().map(String::as_str).chain(["private"]));
        data.source = Some((source, Rc::new(payload)));
        drop(data);
        self.flush();
        Ok(())
    }

    /// The selection as the application sees it.
    pub(crate) fn selection(&self) -> Contents {
        let data = self.data.borrow();
        if data.machine.owns_selection()
            && let Some((_, payload)) = &data.source
        {
            return Contents::Local(payload.clone());
        }
        match data
            .selection
            .and_then(|id| data.offers.get(&id).map(|offer| (id, offer)))
        {
            Some((id, offer)) => Contents::Remote(id, offer.mimes.clone()),
            None => Contents::Empty,
        }
    }

    /// Reads `mime` of offer `id` from the other client. Returns None when
    /// the offer is gone, the type is missing, or the data does not
    /// arrive within the timeout.
    pub(crate) fn receive(&self, id: ObjectId, mime: &str) -> Option<Vec<u8>> {
        let data = self.data.borrow();
        let offer = data.offers.get(&id)?;
        if data.machine.read(mime).is_err() && data.selection == Some(id) {
            return None;
        }
        let (read_end, write_end) = libc::pipe().ok()?;
        offer
            .proxy
            .request(
                OFFER_RECEIVE,
                &[Arg::Str(Some(mime)), Arg::Fd(write_end.as_fd())],
            )
            .ok()?;
        // The other client holds its own copy of the write end now.
        drop(write_end);
        drop(data);
        self.flush();
        read_all(&read_end)
    }

    fn create_source(&self, mimes: &[String], allowed: Option<u32>) -> Result<Proxy, Error> {
        let globals = self.globals.borrow();
        let manager = globals
            .data_device_manager
            .as_ref()
            .ok_or(Error::ClipboardWriteFailed)?;
        let source = manager
            .request(MANAGER_CREATE_DATA_SOURCE, &[Arg::NewId])
            .ok()
            .flatten()
            .ok_or(Error::ClipboardWriteFailed)?;
        for mime in mimes {
            let _ = source.request(SOURCE_OFFER, &[Arg::Str(Some(mime))]);
        }
        if let Some(allowed) = allowed {
            let _ = source.request(SOURCE_SET_ACTIONS, &[Arg::Uint(allowed)]);
        }
        self.routes
            .borrow_mut()
            .insert(source.id(), Role::DataSource);
        Ok(source)
    }

    /// Starts dragging `payload` from `window`. The left button must be
    /// down on the window: the drag needs the serial of that press.
    pub(crate) fn begin_drag(
        &self,
        window: &WaylandWindow,
        payload: Payload,
        allowed: DragOperations,
        preview: Option<&Image>,
    ) -> Result<(), Error> {
        let serial = {
            let seat = self.seat.borrow();
            let over = seat
                .pointer_focus
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|focus| std::ptr::eq(focus.as_ref(), window));
            match seat.press_serial {
                Some(serial) if over && seat.pressed > 0 => serial,
                _ => return Err(Error::NoDragEvent),
            }
        };
        let source = self.create_source(&payload.mime_types(), Some(actions(allowed)))?;
        let icon = preview.and_then(|image| self.image_surface(image));
        {
            let data = self.data.borrow();
            let device = data.device.as_ref().ok_or(Error::UnsupportedFormat)?;
            device
                .request(
                    DEVICE_START_DRAG,
                    &[
                        Arg::Object(Some(&source)),
                        Arg::Object(Some(&window.surface)),
                        Arg::Object(icon.as_ref().map(|(surface, _)| surface)),
                        Arg::Uint(serial),
                    ],
                )
                .map_err(|_| Error::UnsupportedFormat)?;
        }
        if let Some((surface, _)) = &icon
            && let Some(image) = preview
        {
            // The icon's center follows the pointer.
            let offset = [
                -i32::try_from(image.width() / 2).unwrap_or(0),
                -i32::try_from(image.height() / 2).unwrap_or(0),
            ];
            if surface.version() >= 5 {
                let _ = surface.request(10, &[Arg::Int(offset[0]), Arg::Int(offset[1])]);
            }
            let _ = surface.request(6, &[]);
        }
        self.data.borrow_mut().outgoing = Some(Outgoing {
            source,
            payload: Rc::new(payload),
            window: Rc::downgrade(&window.shared),
            action: 0,
            _icon: icon,
        });
        self.flush();
        Ok(())
    }

    /// Handles an event of the data device.
    pub(super) fn data_device_event(&self, event: &mut Event) {
        match event.opcode {
            DEVICE_DATA_OFFER => {
                let Some(proxy) = event.arg_mut(0).and_then(|value| value.take_proxy()) else {
                    return;
                };
                self.routes.borrow_mut().insert(proxy.id(), Role::DataOffer);
                self.data.borrow_mut().offers.insert(
                    proxy.id(),
                    Offer {
                        proxy,
                        mimes: Vec::new(),
                        source_actions: 0,
                        action: 0,
                    },
                );
            }
            DEVICE_SELECTION => {
                let mut data = self.data.borrow_mut();
                let previous = data.selection.take();
                if let Some(previous) = previous {
                    self.forget_offer(&mut data, previous);
                }
                data.selection = event.arg(0).object();
                let mimes = data
                    .selection
                    .and_then(|id| data.offers.get(&id))
                    .map(|offer| offer.mimes.clone());
                if let Some(mimes) = mimes {
                    data.machine.receive_offer(mimes);
                }
            }
            DEVICE_ENTER => self.drag_enter(event),
            DEVICE_MOTION => {
                self.drag_motion(Point::new(event.arg(1).fixed(), event.arg(2).fixed()))
            }
            DEVICE_LEAVE => self.drag_leave(),
            DEVICE_DROP => self.drag_drop(),
            _ => {}
        }
    }

    /// Handles an event of an offer.
    pub(super) fn data_offer_event(&self, event: &Event) {
        let mut data = self.data.borrow_mut();
        let Some(offer) = data.offers.get_mut(&event.object) else {
            return;
        };
        match event.opcode {
            OFFER_OFFER => offer.mimes.push(event.arg(0).str().to_owned()),
            OFFER_SOURCE_ACTIONS => offer.source_actions = event.arg(0).uint(),
            OFFER_ACTION => offer.action = event.arg(0).uint(),
            _ => {}
        }
    }

    /// Handles an event of a source of the application.
    pub(super) fn data_source_event(&self, event: &mut Event) {
        let id = event.object;
        match event.opcode {
            SOURCE_SEND => {
                let mime = event.arg(0).str().to_owned();
                let Some(fd) = event.arg_mut(1).and_then(|value| value.take_fd()) else {
                    return;
                };
                let payload = {
                    let data = self.data.borrow();
                    match (&data.source, &data.outgoing) {
                        (Some((source, payload)), _) if source.id() == id => Some(payload.clone()),
                        (_, Some(drag)) if drag.source.id() == id => Some(drag.payload.clone()),
                        _ => None,
                    }
                };
                if let Some(bytes) =
                    payload.and_then(|payload| payload.data(&mime).map(<[u8]>::to_vec))
                {
                    write_all_later(fd, bytes);
                }
            }
            SOURCE_CANCELLED => {
                let mut data = self.data.borrow_mut();
                if data
                    .source
                    .as_ref()
                    .is_some_and(|(source, _)| source.id() == id)
                {
                    data.source = None;
                    data.machine.ownership_lost();
                } else if data
                    .outgoing
                    .as_ref()
                    .is_some_and(|drag| drag.source.id() == id)
                {
                    let drag = data.outgoing.take();
                    drop(data);
                    self.end_drag(drag, DragOperation::None);
                }
                self.routes.borrow_mut().remove(&id);
            }
            SOURCE_ACTION => {
                if let Some(drag) = self
                    .data
                    .borrow_mut()
                    .outgoing
                    .as_mut()
                    .filter(|drag| drag.source.id() == id)
                {
                    drag.action = event.arg(0).uint();
                }
            }
            SOURCE_DND_FINISHED => {
                let drag = {
                    let mut data = self.data.borrow_mut();
                    if !data
                        .outgoing
                        .as_ref()
                        .is_some_and(|drag| drag.source.id() == id)
                    {
                        return;
                    }
                    data.outgoing.take()
                };
                let action = drag.as_ref().map_or(0, |drag| drag.action);
                self.routes.borrow_mut().remove(&id);
                self.end_drag(drag, operation(action));
            }
            SOURCE_DND_DROP_PERFORMED => {}
            _ => {}
        }
    }

    fn end_drag(&self, drag: Option<Outgoing>, operation: DragOperation) {
        if let Some(window) = drag.and_then(|drag| drag.window.upgrade()) {
            window.push(WindowEvent::DragEnded(operation));
        }
    }

    fn forget_offer(&self, data: &mut DataState, id: ObjectId) {
        if let Some(offer) = data.offers.remove(&id) {
            self.routes.borrow_mut().remove(&id);
            let _ = offer.proxy.destroy(OFFER_DESTROY, &[]);
        }
    }

    /// The formats and items of the current drag's offer: from the payload
    /// of the application's own drag, or from the other client.
    fn drag_formats(&self, offer: Option<ObjectId>) -> Vec<DragFormat> {
        if let Some(payload) = self.own_drag_payload() {
            return payload.formats();
        }
        let Some(id) = offer else {
            return Vec::new();
        };
        let mimes = self
            .data
            .borrow()
            .offers
            .get(&id)
            .map(|offer| offer.mimes.clone())
            .unwrap_or_default();
        transfer::formats(&mimes, &mut |mime| self.receive(id, mime))
    }

    fn drag_items(&self, offer: Option<ObjectId>, formats: &[DragFormat]) -> Vec<DragItem> {
        if let Some(payload) = self.own_drag_payload() {
            return payload.read(formats);
        }
        let Some(id) = offer else {
            return Vec::new();
        };
        let mimes = self
            .data
            .borrow()
            .offers
            .get(&id)
            .map(|offer| offer.mimes.clone())
            .unwrap_or_default();
        transfer::read(formats, &mimes, &mut |mime| self.receive(id, mime))
    }

    fn own_drag_payload(&self) -> Option<Rc<Payload>> {
        self.data
            .borrow()
            .outgoing
            .as_ref()
            .map(|drag| drag.payload.clone())
    }

    fn drag_enter(&self, event: &Event) {
        let serial = event.arg(0).uint();
        let window = event
            .arg(1)
            .object()
            .and_then(|surface| self.window_of(surface));
        let surface = Point::new(event.arg(2).fixed(), event.arg(3).fixed());
        let offer = event.arg(4).object();
        let Some(window) = window else {
            return;
        };
        let position = window.content_point(surface);
        let accepts = {
            let target = window.shared.drop_target.borrow();
            (!target.formats.is_empty() && !target.operations.is_empty())
                .then(|| (target.formats.clone(), target.operations))
        };
        let accepted = accepts.and_then(|(wanted, window_operations)| {
            let formats: Vec<DragFormat> = self
                .drag_formats(offer)
                .into_iter()
                .filter(|format| wanted.contains(format))
                .collect();
            if formats.is_empty() {
                return None;
            }
            let same_application = self.data.borrow().outgoing.is_some();
            let source = offer
                .and_then(|id| {
                    self.data
                        .borrow()
                        .offers
                        .get(&id)
                        .map(|offer| operations(offer.source_actions))
                })
                .unwrap_or(DragOperations::NONE);
            let allowed = DragOperations {
                copy: source.copy && window_operations.copy,
                move_: source.move_ && window_operations.move_,
                link: false,
            };
            // PC convention: Ctrl copies, Shift moves, Ctrl+Shift links.
            let modifiers = self.modifiers();
            let preferred = negotiate_drag(
                allowed,
                modifiers.control,
                modifiers.shift,
                same_application,
            );
            (preferred != DragOperation::None).then_some((formats, allowed, preferred))
        });
        self.data.borrow_mut().incoming = Some(Incoming {
            offer,
            window: Rc::downgrade(&window),
            serial,
            position,
            accepted: accepted.clone(),
        });
        self.answer_offer();
        if let Some((formats, allowed, _)) = accepted {
            window.shared.push(WindowEvent::Drag(DragEvent::Enter {
                position,
                formats,
                allowed,
            }));
        }
    }

    /// Tells the source whether the window takes the drag, and which
    /// action it prefers.
    fn answer_offer(&self) {
        let data = self.data.borrow();
        let Some(incoming) = &data.incoming else {
            return;
        };
        let Some(offer) = incoming.offer.and_then(|id| data.offers.get(&id)) else {
            return;
        };
        match &incoming.accepted {
            Some((_, allowed, preferred)) => {
                let mime = offer.mimes.first().map(String::as_str);
                let _ = offer
                    .proxy
                    .request(OFFER_ACCEPT, &[Arg::Uint(incoming.serial), Arg::Str(mime)]);
                let preferred = match preferred {
                    DragOperation::Move => ACTION_MOVE,
                    _ => ACTION_COPY,
                };
                let _ = offer.proxy.request(
                    OFFER_SET_ACTIONS,
                    &[Arg::Uint(actions(*allowed)), Arg::Uint(preferred)],
                );
            }
            None => {
                let _ = offer
                    .proxy
                    .request(OFFER_ACCEPT, &[Arg::Uint(incoming.serial), Arg::Str(None)]);
                let _ = offer
                    .proxy
                    .request(OFFER_SET_ACTIONS, &[Arg::Uint(0), Arg::Uint(0)]);
            }
        }
    }

    fn drag_motion(&self, surface: Point) {
        let (window, allowed, position) = {
            let mut data = self.data.borrow_mut();
            let Some(incoming) = &mut data.incoming else {
                return;
            };
            let window = incoming.window.upgrade();
            let position = window
                .as_ref()
                .map_or(surface, |window| window.content_point(surface));
            incoming.position = position;
            (
                window,
                incoming.accepted.as_ref().map(|(_, allowed, _)| *allowed),
                position,
            )
        };
        if let (Some(window), Some(allowed)) = (window, allowed) {
            window
                .shared
                .push(WindowEvent::Drag(DragEvent::Over { position, allowed }));
        }
    }

    fn drag_leave(&self) {
        let incoming = self.data.borrow_mut().incoming.take();
        let Some(incoming) = incoming else {
            return;
        };
        if incoming.accepted.is_some()
            && let Some(window) = incoming.window.upgrade()
        {
            window.shared.push(WindowEvent::Drag(DragEvent::Leave));
        }
        if let Some(id) = incoming.offer {
            let mut data = self.data.borrow_mut();
            if data.selection != Some(id) {
                self.forget_offer(&mut data, id);
            }
        }
    }

    fn drag_drop(&self) {
        let incoming = self.data.borrow_mut().incoming.take();
        let Some(incoming) = incoming else {
            return;
        };
        let window = incoming.window.upgrade();
        let Some((formats, _, preferred)) = &incoming.accepted else {
            if let Some(id) = incoming.offer {
                let mut data = self.data.borrow_mut();
                self.forget_offer(&mut data, id);
            }
            return;
        };
        let items = self.drag_items(incoming.offer, formats);
        let action = incoming
            .offer
            .and_then(|id| self.data.borrow().offers.get(&id).map(|offer| offer.action))
            .unwrap_or(0);
        let chosen = match operation(action) {
            DragOperation::None => *preferred,
            chosen => chosen,
        };
        if let Some(id) = incoming.offer {
            let mut data = self.data.borrow_mut();
            if let Some(offer) = data.offers.get(&id)
                && action != 0
                && !items.is_empty()
            {
                let _ = offer.proxy.request(OFFER_FINISH, &[]);
            }
            self.forget_offer(&mut data, id);
        }
        if let Some(window) = window {
            let event = if items.is_empty() {
                DragEvent::Leave
            } else {
                DragEvent::Drop {
                    position: incoming.position,
                    items,
                    operation: chosen,
                }
            };
            window.shared.push(WindowEvent::Drag(event));
        }
        self.flush();
    }

    /// A surface that shows `image`, for a drag icon.
    fn image_surface(&self, image: &Image) -> Option<(Proxy, super::shm::Buffers)> {
        let globals = self.globals.borrow();
        let (compositor, shm) = (globals.compositor.as_ref()?, globals.shm.as_ref()?);
        let surface = compositor.request(0, &[Arg::NewId]).ok().flatten()?;
        let mut buffers = super::shm::Buffers::default();
        super::fill_surface(&surface, &mut buffers, shm, image);
        Some((surface, buffers))
    }

    /// Releases the drag icon buffers that the compositor gave back.
    pub(super) fn release_drag_buffer(&self, id: ObjectId) -> bool {
        let mut data = self.data.borrow_mut();
        data.outgoing
            .as_mut()
            .and_then(|drag| drag._icon.as_mut())
            .is_some_and(|(_, buffers)| buffers.release(id))
    }
}

/// Reads `fd` to its end, or until the timeout passes.
fn read_all(fd: &OwnedFd) -> Option<Vec<u8>> {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut bytes = Vec::new();
    let mut chunk = vec![0; 64 * 1024];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let watch = Watch {
            fd: fd.as_fd(),
            read: true,
            write: false,
        };
        let ready = libc::wait(&[watch], Some(left)).ok()?;
        if !ready[0].read && !ready[0].hangup {
            continue;
        }
        match libc::read_fd(fd.as_fd(), &mut chunk) {
            Ok(0) => return Some(bytes),
            Ok(count) => bytes.extend_from_slice(&chunk[..count]),
            Err(error) if libc::would_block(&error) => {}
            Err(_) => return None,
        }
    }
}

/// Writes `bytes` to `fd` on another thread, so a slow reader never
/// blocks the event loop. Closing `fd` ends the stream.
fn write_all_later(fd: OwnedFd, bytes: Vec<u8>) {
    let _ = std::thread::Builder::new()
        .name("choscorui-transfer".to_owned())
        .spawn(move || {
            let mut written = 0;
            while written < bytes.len() {
                match libc::write_fd(fd.as_fd(), &bytes[written..]) {
                    Ok(0) => return,
                    Ok(count) => written += count,
                    Err(error) if libc::would_block(&error) => {
                        let watch = Watch {
                            fd: fd.as_fd(),
                            read: false,
                            write: true,
                        };
                        if libc::wait(&[watch], Some(READ_TIMEOUT)).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
}
