//! The one connection of the X11 backend: an Xlib display whose socket
//! xcb shares, through libX11-xcb.
//!
//! `XOpenDisplay` opens the connection, and `XGetXCBConnection` gives its
//! xcb connection. The requests of the backend go through xcb. Xlib owns
//! the event queue, because XIM waits for its replies in `XIfEvent`. The
//! event loop reads each event with `XNextEvent`, and gives it to
//! `XFilterEvent` before it handles it.
//!
//! The backend reads the events in the wire format of the protocol. For
//! each event code that the backend handles, a hook of `XESetWireToEvent`
//! copies the wire event before Xlib queues it. The hook calls the
//! previous procedure of the code first, so the `XEvent` of a core event
//! is complete, and the side effects of the XKB procedure of Xlib stay.
//! A hook of `XESetWireToEventCookie` copies the generic events of
//! XInput and Present. Each hook marks its `XEvent` with a number, and
//! `next_event` finds the copy of an event by its number. The copies of
//! the events that XIM took from the queue are dropped. An event without
//! a mark is an event that XIM put back into the queue.
//!
//! A hook of `XESetWireToError` copies each error and tells Xlib that
//! the error is handled, so the error handler of Xlib does not stop the
//! process. A lost connection does not stop it either: the I/O error
//! handler of the process is silent for these displays, and the exit
//! handler of each display returns. After the loss, the backend makes no
//! more Xlib calls that read or write the connection.

use super::ctypes::{CInt, CLong, CUlong, c_void};
use super::macros::{callback, libraries};
use super::wayland::table;
use super::x11::{Bytes, Xcb, XcbConnection};
use super::xlib::{
    self, GENERIC_EVENT, IoErrorHandler, KEY_PRESS, KEY_RELEASE, QUEUED_AFTER_READING,
    WireToEventProc, XAnyEvent, XDisplay, XErrorEventRec, XEvent, XGenericEventCookie, XKeyEvent,
    XWireError, XWireEvent, Xlib, XlibError,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

libraries! {
    /// libX11-xcb: the xcb connection of an Xlib display.
    pub(crate) struct XlibXcb {
        library: "libX11-xcb",
        sonames: ["libX11-xcb.so.1"],
        headers: ["X11/Xlib-xcb.h"],
        pkg_config: "x11-xcb",
        package: "libx11-xcb-dev",
        functions {
            fn XGetXCBConnection(display: *mut XDisplay) -> *mut XcbConnection;
        }
    }
}

/// The mark of an `XEvent` that a hook copied, in `pad[MARK]`. The
/// number of the copy follows in `pad[MARK + 1]`. No event of the core
/// protocol or of an extension reaches these words.
const MARK: usize = 22;

/// The value of the mark: "CHOSCRUI" in ASCII.
const MARK_VALUE: CLong = 0x4348_4f53_4352_5549;

/// The first and the last event code of the core protocol.
const CORE_EVENTS: std::ops::RangeInclusive<u8> = 2..=34;

/// The first event code of the extensions.
const FIRST_EXTENSION_EVENT: u8 = 64;

/// The copies of the events and the errors of one display, in the order
/// of arrival.
pub(crate) struct Stream {
    /// The previous procedure of each event code, which the hook calls.
    previous: [WireToEventProc; 128],
    /// The event codes with a hook.
    watched: [bool; 128],
    /// The wire events that Xlib queued, by number.
    copies: VecDeque<(u64, Bytes)>,
    /// The number of the latest copy.
    next: u64,
    errors: VecDeque<Bytes>,
    /// Set by the I/O error handlers.
    lost: bool,
}

impl Stream {
    fn new() -> Self {
        Self {
            previous: [None; 128],
            watched: [false; 128],
            copies: VecDeque::new(),
            next: 0,
            errors: VecDeque::new(),
            lost: false,
        }
    }

    /// Keeps the copy of a wire event and returns its number.
    fn push(&mut self, bytes: Vec<u8>) -> u64 {
        self.next += 1;
        self.copies.push_back((self.next, Bytes::from(bytes)));
        self.next
    }

    /// Takes the copy with the number `id`, when its event has the type
    /// `kind`. The older copies belong to the events that XIM took from
    /// the queue, so they go.
    fn take(&mut self, id: u64, kind: CInt) -> Option<Bytes> {
        let index = self
            .copies
            .binary_search_by_key(&id, |(number, _)| *number)
            .ok()?;
        let code = self.copies[index].1.u8(0) & 0x7f;
        if CInt::from(code) != kind {
            return None;
        }
        self.copies.drain(..index);
        self.copies.pop_front().map(|(_, bytes)| bytes)
    }
}

/// The streams of the open displays, by the address of the display. The
/// hooks of Xlib find the stream of their display here.
static STREAMS: Mutex<Vec<(usize, Arc<Mutex<Stream>>)>> = Mutex::new(Vec::new());

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The stream of `display`, when it is a display of this module.
fn stream_of(display: *mut XDisplay) -> Option<Arc<Mutex<Stream>>> {
    lock(&STREAMS)
        .iter()
        .find(|(address, _)| *address == display.addr())
        .map(|(_, stream)| Arc::clone(stream))
}

/// The I/O error handler of the process before this module set its own.
static PREVIOUS_IO_HANDLER: OnceLock<IoErrorHandler> = OnceLock::new();

/// Writes the mark and the number `id` into `event`.
///
/// # Safety
///
/// `event` points at a whole `XEvent`.
unsafe fn mark(event: *mut XEvent, id: u64) {
    // The pad starts the event and covers it.
    let pad = event.cast::<[CLong; 24]>();
    // SAFETY: the caller passes a whole event.
    unsafe { (*pad)[MARK] = MARK_VALUE };
    // SAFETY: as above.
    unsafe { (*pad)[MARK + 1] = CLong::from_ne_bytes(id.to_ne_bytes()) };
}

callback! {
    /// The hook of an event code: copies the wire event, and fills the
    /// `XEvent` of the queue. A core event gets the `XEvent` of the
    /// previous procedure. An extension event gets an `XAnyEvent` with
    /// its code, after the previous procedure ran on a scratch event.
    ///
    /// # Safety
    ///
    /// Xlib passes its display, the `XEvent` of the queue, and a wire
    /// event of 32 bytes, with the display locked.
    fn wire_to_event(display: *mut XDisplay, event: *mut XEvent, wire: *mut XWireEvent) -> CInt
        [boundary "Xlib wire to event", fallback 0]
    {
        let Some(stream) = stream_of(display) else {
            return 0;
        };
        // SAFETY: a wire event has 32 bytes.
        let bytes = unsafe { table(wire.cast::<u8>().cast_const(), 32) }.to_vec();
        let code = bytes[0] & 0x7f;
        let previous = lock(&stream).previous[usize::from(code)];
        let core = code < FIRST_EXTENSION_EVENT;
        let filled = match previous {
            Some(previous) if core => {
                // SAFETY: the arguments are those of this call.
                unsafe { previous(display, event, wire) != 0 }
            }
            Some(previous) => {
                let mut scratch = XEvent { pad: [0; 24] };
                // SAFETY: the scratch event is a whole event.
                unsafe { previous(display, &raw mut scratch, wire) };
                false
            }
            None => false,
        };
        if !filled {
            let any = XAnyEvent {
                r#type: CInt::from(code),
                serial: CUlong::from(u16::from_ne_bytes([bytes[2], bytes[3]])),
                send_event: CInt::from(bytes[0] & 0x80 != 0),
                display,
                window: 0,
            };
            // SAFETY: Xlib passes a whole event, which starts with the
            // fields of `XAnyEvent`.
            unsafe { event.cast::<XAnyEvent>().write(any) };
        }
        let id = lock(&stream).push(bytes);
        // SAFETY: Xlib passes a whole event.
        unsafe { mark(event, id) };
        1
    }

    /// The hook of the generic events of an extension: copies the wire
    /// event with its payload, and fills the cookie of the queue without
    /// data.
    ///
    /// # Safety
    ///
    /// Xlib passes its display, the cookie of a whole `XEvent` of the
    /// queue, and a generic event whose payload follows its 32 bytes.
    fn wire_to_cookie(
        display: *mut XDisplay,
        cookie: *mut XGenericEventCookie,
        wire: *mut XWireEvent,
    ) -> CInt
        [boundary "Xlib wire to cookie", fallback 0]
    {
        let Some(stream) = stream_of(display) else {
            return 0;
        };
        let pointer = wire.cast::<u8>().cast_const();
        // SAFETY: a wire event has 32 bytes.
        let header = unsafe { table(pointer, 32) };
        let units = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
        let length = 32 + 4 * usize::try_from(units).unwrap_or(0);
        // SAFETY: Xlib moved the `length` units of the payload after the
        // 32 bytes.
        let bytes = unsafe { table(pointer, length) }.to_vec();
        let filled = XGenericEventCookie {
            r#type: GENERIC_EVENT,
            serial: CUlong::from(u16::from_ne_bytes([bytes[2], bytes[3]])),
            send_event: CInt::from(bytes[0] & 0x80 != 0),
            display,
            extension: CInt::from(bytes[1]),
            evtype: CInt::from(u16::from_ne_bytes([bytes[8], bytes[9]])),
            cookie: 0,
            data: std::ptr::null_mut(),
        };
        // SAFETY: Xlib passes the cookie of a whole event.
        unsafe { *cookie = filled };
        let id = lock(&stream).push(bytes);
        // SAFETY: the cookie is the start of a whole event.
        unsafe { mark(cookie.cast::<XEvent>(), id) };
        1
    }

    /// The hook of an error code: copies the error, and tells Xlib that it
    /// is handled.
    ///
    /// # Safety
    ///
    /// Xlib passes its display and a wire error of 32 bytes.
    fn wire_to_error(display: *mut XDisplay, _event: *mut XErrorEventRec, wire: *mut XWireError) -> CInt
        [boundary "Xlib wire to error", fallback 0]
    {
        if let Some(stream) = stream_of(display) {
            // SAFETY: a wire error has 32 bytes.
            let bytes = unsafe { table(wire.cast::<u8>().cast_const(), 32) }.to_vec();
            lock(&stream).errors.push_back(Bytes::from(bytes));
        }
        0
    }

    /// The I/O error handler of the process: marks a display of this
    /// module as lost, and calls the previous handler for the others.
    ///
    /// # Safety
    ///
    /// Xlib passes a display.
    fn io_error(display: *mut XDisplay) -> CInt
        [boundary "Xlib I/O error", fallback 0]
    {
        if let Some(stream) = stream_of(display) {
            lock(&stream).lost = true;
            return 0;
        }
        match PREVIOUS_IO_HANDLER.get().copied().flatten() {
            // SAFETY: the previous handler takes the display of this call.
            Some(previous) => unsafe { previous(display) },
            None => 0,
        }
    }

    /// The I/O error exit handler of a display of this module: marks it as
    /// lost, and returns instead of the exit of the process.
    ///
    /// # Safety
    ///
    /// Xlib passes the display of the handler.
    fn io_error_exit(display: *mut XDisplay, _data: *mut c_void)
        [boundary "Xlib I/O error exit", fallback ()]
    {
        if let Some(stream) = stream_of(display) {
            lock(&stream).lost = true;
        }
    }
}

/// An event of the connection.
pub(crate) enum Event {
    /// An error, in wire format.
    Error(Bytes),
    /// An event from the server: its wire format, and its `XEvent`.
    Server(Bytes, XlibEvent),
    /// An event that XIM put back into the queue.
    PutBack(XlibEvent),
}

/// An `XEvent` from the queue of a display.
pub(crate) struct XlibEvent(XEvent);

impl XlibEvent {
    /// The type of the event.
    pub(crate) fn kind(&self) -> CInt {
        // SAFETY: every event starts with its type.
        unsafe { self.0.r#type }
    }

    /// The window of the event; zero for a generic event or an event of
    /// an extension.
    pub(crate) fn window(&self) -> CUlong {
        if self.kind() == GENERIC_EVENT {
            return 0;
        }
        // SAFETY: every other event starts with the fields of
        // `XAnyEvent`.
        unsafe { self.0.xany.window }
    }

    /// The key event, for a key press or a key release.
    pub(crate) fn key(&self) -> Option<XKeyEvent> {
        let kind = self.kind();
        if kind != KEY_PRESS && kind != KEY_RELEASE {
            return None;
        }
        // SAFETY: the type says that the event is a key event.
        Some(unsafe { self.0.xkey })
    }

    /// Sends a key event to `window`: the window of the input context
    /// that has the focus.
    pub(crate) fn set_key_window(&mut self, window: u32) {
        if let Some(mut key) = self.key() {
            key.window = CUlong::from(window);
            self.0.xkey = key;
        }
    }

    pub(in crate::ffi) fn raw(&mut self) -> *mut XEvent {
        &raw mut self.0
    }

    /// The number of the copy of the event, when a hook marked it.
    fn id(&self) -> Option<u64> {
        // SAFETY: the pad covers the whole event.
        let pad = unsafe { self.0.pad };
        (pad[MARK] == MARK_VALUE).then(|| u64::from_ne_bytes(pad[MARK + 1].to_ne_bytes()))
    }
}

/// Why the display did not open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OpenError {
    /// libX11 or libX11-xcb did not load, or `XInitThreads` failed.
    Library(XlibError),
    /// The server refused the connection, or `DISPLAY` names none.
    Refused,
}

/// The Xlib display of the X11 backend, and its xcb connection.
pub(crate) struct SharedDisplay {
    lib: &'static Xlib,
    xcb: &'static Xcb,
    raw: *mut XDisplay,
    connection: *mut XcbConnection,
    /// The root window of screen 0, which XIM watches.
    root0: CUlong,
    stream: Arc<Mutex<Stream>>,
}

impl SharedDisplay {
    /// Opens the display of `DISPLAY`, and sets the hooks of the core
    /// events and of the errors.
    pub(crate) fn open() -> Result<std::rc::Rc<Self>, OpenError> {
        let lib = xlib::library().map_err(OpenError::Library)?;
        let shared =
            XlibXcb::get().map_err(|error| OpenError::Library(XlibError::Library(error)))?;
        let xcb = Xcb::get().map_err(|error| OpenError::Library(XlibError::Library(error)))?;
        PREVIOUS_IO_HANDLER.get_or_init(|| {
            // SAFETY: the handler stays valid for the process.
            unsafe { (lib.XSetIOErrorHandler)(Some(io_error)) }
        });
        // SAFETY: null opens the display of `DISPLAY`.
        let raw = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
        if raw.is_null() {
            return Err(OpenError::Refused);
        }
        // SAFETY: the display is open.
        unsafe { (lib.XSetIOErrorExitHandler)(raw, Some(io_error_exit), std::ptr::null_mut()) };
        // SAFETY: the display is open; the connection lives as long.
        let connection = unsafe { (shared.XGetXCBConnection)(raw) };
        // SAFETY: the display is open.
        let root0 = unsafe { (lib.XRootWindow)(raw, 0) };
        let stream = Arc::new(Mutex::new(Stream::new()));
        {
            let mut streams = lock(&STREAMS);
            // A display that Xlib freed can leave its address to this one.
            streams.retain(|(address, _)| *address != raw.addr());
            streams.push((raw.addr(), Arc::clone(&stream)));
        }
        let display = Self {
            lib,
            xcb,
            raw,
            connection,
            root0,
            stream,
        };
        // The drop of `display` closes it.
        if connection.is_null() {
            return Err(OpenError::Refused);
        }
        for code in CORE_EVENTS {
            display.watch_event(code);
        }
        for code in 1..=255 {
            // SAFETY: the display is open; the hook stays valid for the
            // process. The previous procedure is the default one.
            unsafe { (lib.XESetWireToError)(raw, code, Some(wire_to_error)) };
        }
        Ok(std::rc::Rc::new(display))
    }

    /// Sets the hook of the event code `code`: a core event, or the event
    /// of an extension.
    pub(crate) fn watch_event(&self, code: u8) {
        let index = usize::from(code & 0x7f);
        if lock(&self.stream).watched[index] {
            return;
        }
        // SAFETY: the display is open; the hook stays valid for the
        // process.
        let previous =
            unsafe { (self.lib.XESetWireToEvent)(self.raw, CInt::from(code), Some(wire_to_event)) };
        let mut stream = lock(&self.stream);
        stream.previous[index] = previous;
        stream.watched[index] = true;
    }

    /// Sets the hook of the generic events of the extension `opcode`.
    pub(crate) fn watch_generic(&self, opcode: u8) {
        // SAFETY: the display is open; the hook stays valid for the
        // process. No procedure of Xlib handles these events.
        unsafe {
            (self.lib.XESetWireToEventCookie)(self.raw, CInt::from(opcode), Some(wire_to_cookie))
        };
    }

    pub(in crate::ffi) fn lib(&self) -> &'static Xlib {
        self.lib
    }

    pub(in crate::ffi) fn raw(&self) -> *mut XDisplay {
        self.raw
    }

    pub(in crate::ffi) fn connection(&self) -> *mut XcbConnection {
        self.connection
    }

    pub(in crate::ffi) fn root0(&self) -> CUlong {
        self.root0
    }

    /// The number of the default screen.
    pub(in crate::ffi) fn default_screen(&self) -> CInt {
        // SAFETY: the display is open.
        unsafe { (self.lib.XDefaultScreen)(self.raw) }
    }

    /// True after the connection failed.
    pub(crate) fn lost(&self) -> bool {
        if lock(&self.stream).lost {
            return true;
        }
        // SAFETY: the connection lives as long as the display.
        unsafe { (self.xcb.xcb_connection_has_error)(self.connection) != 0 }
    }

    /// Sends the requests of Xlib and of xcb, and reads the events that
    /// arrived into the queue.
    pub(crate) fn flush(&self) {
        if self.lost() {
            return;
        }
        // SAFETY: the display is open.
        unsafe { (self.lib.XFlush)(self.raw) };
    }

    /// The next error or event, without a wait: first the errors, then the
    /// events of the queue, then the events that the socket holds. None
    /// when nothing arrived, or after the connection failed.
    pub(crate) fn next_event(&self) -> Option<Event> {
        if let Some(error) = lock(&self.stream).errors.pop_front() {
            return Some(Event::Error(error));
        }
        if self.lost() {
            return None;
        }
        // SAFETY: the display is open. The call reads the socket without
        // a wait when the queue is empty.
        let queued = unsafe { (self.lib.XEventsQueued)(self.raw, QUEUED_AFTER_READING) };
        if queued <= 0 {
            let mut stream = lock(&self.stream);
            if let Some(error) = stream.errors.pop_front() {
                return Some(Event::Error(error));
            }
            // The queue is empty, so XIM took the events of the copies
            // that are left.
            stream.copies.clear();
            return None;
        }
        let mut event = XlibEvent(XEvent { pad: [0; 24] });
        // SAFETY: an event is queued, so the call does not wait.
        unsafe { (self.lib.XNextEvent)(self.raw, event.raw()) };
        let copy = event
            .id()
            .and_then(|id| lock(&self.stream).take(id, event.kind()));
        Some(match copy {
            Some(bytes) => Event::Server(bytes, event),
            None => Event::PutBack(event),
        })
    }
}

impl Drop for SharedDisplay {
    fn drop(&mut self) {
        // SAFETY: this is the only close of the display, after every user
        // of it, because each holds the display.
        unsafe { (self.lib.XCloseDisplay)(self.raw) };
        let mut streams = lock(&STREAMS);
        streams.retain(|(_, stream)| !Arc::ptr_eq(stream, &self.stream));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(code: u8) -> Vec<u8> {
        let mut bytes = vec![0; 32];
        bytes[0] = code;
        bytes
    }

    #[test]
    fn a_taken_copy_drops_the_copies_that_xim_took() {
        let mut stream = Stream::new();
        let first = stream.push(event(33));
        let second = stream.push(event(2));
        let third = stream.push(event(3));
        // XIM took the first event with `XIfEvent`.
        assert_eq!(stream.take(second, 2).map(|bytes| bytes.u8(0)), Some(2));
        assert!(stream.take(first, 33).is_none());
        assert_eq!(stream.take(third, 3).map(|bytes| bytes.u8(0)), Some(3));
        assert!(stream.copies.is_empty());
    }

    #[test]
    fn a_mark_of_another_type_is_not_a_copy() {
        let mut stream = Stream::new();
        let id = stream.push(event(33 | 0x80));
        // A put-back key event with a stale mark keeps the copy.
        assert!(stream.take(id, 2).is_none());
        assert_eq!(stream.copies.len(), 1);
        // The send bit does not count.
        assert!(stream.take(id, 33).is_some());
    }

    #[test]
    fn events_keep_their_mark() {
        let mut event = XlibEvent(XEvent { pad: [0; 24] });
        assert_eq!(event.id(), None);
        // SAFETY: the event is whole.
        unsafe { mark(event.raw(), 7) };
        assert_eq!(event.id(), Some(7));
    }
}
