//! A safe interface to libwayland-client.
//!
//! - `Connection` owns the display connection and its event queue.
//! - `Proxy` owns one protocol object. Dropping it sends the object's
//!   destructor request when the interface has one without arguments,
//!   and destroys the client-side proxy otherwise.
//! - `Proxy::request` checks the arguments against the signature of the
//!   request in the hand-written table before it marshals them, so a
//!   wrong argument is an error and never reaches libwayland.
//! - One dispatcher receives the events of every proxy. It decodes each
//!   event into an owned `Event` and queues it on the connection.
//!   `Connection::take_events` returns them in protocol order.
//!
//! Each proxy has a unique `ObjectId` in its user data. Ids are never
//! reused, so a queued event never names a newer object by mistake.

pub(crate) mod protocols;
pub(crate) mod sys;

use super::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::callback;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::rc::Rc;
use sys::{Interface, WaylandClient, WlArgument, WlDisplay, WlInterface, WlMessage, WlProxy};

pub(crate) use protocols::*;

/// A protocol object, unique for the life of the connection. The display
/// is 0.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ObjectId(pub(crate) u64);

/// What can go wrong with the connection or a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WaylandError {
    /// libwayland-client did not load.
    Library(super::dl::LoadError),
    /// No compositor accepted the connection.
    ConnectFailed,
    /// The connection failed: the compositor went away, or it sent a
    /// protocol error.
    Connection(String),
    /// A request had the wrong arguments for its signature.
    BadRequest {
        interface: &'static str,
        opcode: u16,
        reason: &'static str,
    },
    /// libwayland did not create the new object of a request.
    NoObject(&'static str),
}

impl fmt::Display for WaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(error) => write!(f, "{error}"),
            Self::ConnectFailed => f.write_str("no Wayland compositor accepted the connection"),
            Self::Connection(reason) => write!(f, "the Wayland connection failed: {reason}"),
            Self::BadRequest {
                interface,
                opcode,
                reason,
            } => write!(f, "request {opcode} of {interface}: {reason}"),
            Self::NoObject(interface) => write!(f, "libwayland did not create a {interface}"),
        }
    }
}

impl std::error::Error for WaylandError {}

/// One argument of a request.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Arg<'a> {
    Int(i32),
    Uint(u32),
    /// A string; `None` only where the signature allows null.
    Str(Option<&'a str>),
    /// An object; `None` only where the signature allows null.
    Object(Option<&'a Proxy>),
    /// The new object of a request whose signature names its interface.
    NewId,
    /// The new object of `wl_registry.bind`: its interface and version.
    Bind(&'static Interface, u32),
    Fd(BorrowedFd<'a>),
}

/// One argument of an event.
#[derive(Debug)]
pub(crate) enum Value {
    Int(i32),
    Uint(u32),
    Fixed(f64),
    Str(Option<String>),
    Object(Option<ObjectId>),
    /// A new object that the compositor created. Dropping it destroys it.
    NewId(Proxy),
    Array(Vec<u8>),
    Fd(OwnedFd),
}

impl Value {
    pub(crate) fn int(&self) -> i32 {
        match self {
            Self::Int(value) => *value,
            Self::Uint(value) => value.cast_signed(),
            _ => 0,
        }
    }

    pub(crate) fn uint(&self) -> u32 {
        match self {
            Self::Uint(value) => *value,
            Self::Int(value) => value.cast_unsigned(),
            _ => 0,
        }
    }

    pub(crate) fn fixed(&self) -> f64 {
        match self {
            Self::Fixed(value) => *value,
            _ => 0.0,
        }
    }

    pub(crate) fn object(&self) -> Option<ObjectId> {
        match self {
            Self::Object(id) => *id,
            _ => None,
        }
    }

    /// A string that may be null.
    pub(crate) fn str_opt(&self) -> Option<&str> {
        match self {
            Self::Str(text) => text.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn str(&self) -> &str {
        match self {
            Self::Str(Some(text)) => text,
            _ => "",
        }
    }

    pub(crate) fn array(&self) -> &[u8] {
        match self {
            Self::Array(bytes) => bytes,
            _ => &[],
        }
    }

    /// The new object or the descriptor of the argument, taken out of it.
    pub(crate) fn take_proxy(&mut self) -> Option<Proxy> {
        match std::mem::replace(self, Self::Int(0)) {
            Self::NewId(proxy) => Some(proxy),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(crate) fn take_fd(&mut self) -> Option<OwnedFd> {
        match std::mem::replace(self, Self::Int(0)) {
            Self::Fd(fd) => Some(fd),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// One event: the object that it is for, its opcode, and its arguments.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) object: ObjectId,
    pub(crate) opcode: u16,
    pub(crate) args: Vec<Value>,
}

impl Event {
    /// Argument `index`, or an integer 0 when the event has fewer.
    pub(crate) fn arg(&self, index: usize) -> &Value {
        const ZERO: Value = Value::Int(0);
        self.args.get(index).unwrap_or(&ZERO)
    }

    pub(crate) fn arg_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.args.get_mut(index)
    }
}

/// The connection state that the proxies share.
struct Display {
    lib: &'static WaylandClient,
    raw: *mut WlDisplay,
    events: RefCell<VecDeque<Event>>,
    next_id: Cell<u64>,
}

impl Drop for Display {
    fn drop(&mut self) {
        // SAFETY: every proxy holds an `Rc` of the display, so none is
        // left; the display pointer came from `wl_display_connect`.
        unsafe { (self.lib.wl_display_disconnect)(self.raw) }
    }
}

/// A connection to a Wayland compositor.
pub(crate) struct Connection {
    display: Rc<Display>,
}

/// A prepared read of the connection. `read` reads the events that the
/// socket has; dropping the guard without a read cancels it.
pub(crate) struct ReadGuard<'a> {
    connection: &'a Connection,
    done: bool,
}

impl ReadGuard<'_> {
    /// Reads the events from the socket and queues them for dispatch.
    pub(crate) fn read(mut self) -> Result<(), WaylandError> {
        self.done = true;
        let display = &self.connection.display;
        // SAFETY: the read was prepared on this display and not yet ended.
        let result = unsafe { (display.lib.wl_display_read_events)(display.raw) };
        if result < 0 {
            return Err(self.connection.failure());
        }
        Ok(())
    }
}

impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        if !self.done {
            let display = &self.connection.display;
            // SAFETY: the read was prepared on this display and not ended.
            unsafe { (display.lib.wl_display_cancel_read)(display.raw) }
        }
    }
}

impl Connection {
    /// Connects to the compositor of `WAYLAND_DISPLAY`, or of `name`.
    pub(crate) fn connect(name: Option<&str>) -> Result<Self, WaylandError> {
        let lib = WaylandClient::get().map_err(WaylandError::Library)?;
        let name = name
            .map(CString::new)
            .transpose()
            .map_err(|_| WaylandError::ConnectFailed)?;
        let pointer = name.as_deref().map_or(std::ptr::null(), CStr::as_ptr);
        // SAFETY: `pointer` is null or a NUL-terminated name for the call.
        let raw = unsafe { (lib.wl_display_connect)(pointer.cast::<CChar>()) };
        if raw.is_null() {
            return Err(WaylandError::ConnectFailed);
        }
        Ok(Self {
            display: Rc::new(Display {
                lib,
                raw,
                events: RefCell::new(VecDeque::new()),
                next_id: Cell::new(1),
            }),
        })
    }

    /// The socket of the connection, for the event loop.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the display is connected.
        let fd = unsafe { (self.display.lib.wl_display_get_fd)(self.display.raw) };
        // SAFETY: the descriptor stays open while the display is
        // connected, which outlives the borrow of `self`.
        unsafe { BorrowedFd::borrow_raw(fd) }
    }

    /// The error that ended the connection.
    fn failure(&self) -> WaylandError {
        let display = &self.display;
        // SAFETY: the display is connected.
        let code = unsafe { (display.lib.wl_display_get_error)(display.raw) };
        let mut interface: *const WlInterface = std::ptr::null();
        let mut id = 0u32;
        // SAFETY: the out-pointers are valid for the call.
        let protocol = unsafe {
            (display.lib.wl_display_get_protocol_error)(
                display.raw,
                &raw mut interface,
                &raw mut id,
            )
        };
        let reason = if interface.is_null() {
            std::io::Error::from_raw_os_error(code).to_string()
        } else {
            // SAFETY: libwayland returns the interface of the object that
            // the error names, which is one of the static tables.
            let interface = unsafe { &*interface };
            // SAFETY: the tables hold NUL-terminated name literals.
            let name = unsafe { CStr::from_ptr(interface.name.cast()) };
            format!(
                "protocol error {protocol} on {}@{id}",
                name.to_string_lossy()
            )
        };
        WaylandError::Connection(reason)
    }

    /// Sends the buffered requests. Returns false when the socket is full
    /// and some requests wait.
    pub(crate) fn flush(&self) -> Result<bool, WaylandError> {
        // SAFETY: the display is connected.
        let result = unsafe { (self.display.lib.wl_display_flush)(self.display.raw) };
        if result >= 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if super::libc::would_block(&error) {
            Ok(false)
        } else {
            Err(self.failure())
        }
    }

    /// Prepares to read the socket. Queued events that wait for dispatch
    /// are dispatched first.
    pub(crate) fn prepare_read(&self) -> Result<ReadGuard<'_>, WaylandError> {
        loop {
            // SAFETY: the display is connected.
            let result = unsafe { (self.display.lib.wl_display_prepare_read)(self.display.raw) };
            if result == 0 {
                return Ok(ReadGuard {
                    connection: self,
                    done: false,
                });
            }
            self.dispatch_pending()?;
        }
    }

    /// Dispatches the events that libwayland has read. The dispatcher
    /// queues them for `take_events`.
    pub(crate) fn dispatch_pending(&self) -> Result<(), WaylandError> {
        // SAFETY: the display is connected; the dispatcher only queues.
        let result = unsafe { (self.display.lib.wl_display_dispatch_pending)(self.display.raw) };
        if result < 0 {
            Err(self.failure())
        } else {
            Ok(())
        }
    }

    /// Sends the requests and waits until the compositor has handled
    /// them, dispatching the events that arrive meanwhile.
    pub(crate) fn roundtrip(&self) -> Result<(), WaylandError> {
        // SAFETY: the display is connected; the dispatcher only queues.
        let result = unsafe { (self.display.lib.wl_display_roundtrip)(self.display.raw) };
        if result < 0 {
            Err(self.failure())
        } else {
            Ok(())
        }
    }

    /// The queued events, oldest first.
    pub(crate) fn take_events(&self) -> Vec<Event> {
        self.display.events.borrow_mut().drain(..).collect()
    }

    /// Sends a request of `wl_display`: `sync` (0) or `get_registry` (1).
    pub(crate) fn display_request(&self, opcode: u16) -> Result<Proxy, WaylandError> {
        let display = Proxy {
            display: Rc::clone(&self.display),
            raw: self.display.raw.cast::<WlProxy>(),
            id: ObjectId(0),
            interface: &WL_DISPLAY,
            version: 1,
            owned: false,
        };
        display
            .request(opcode, &[Arg::NewId])?
            .ok_or(WaylandError::NoObject("wl_display child"))
    }
}

/// An owned protocol object.
pub(crate) struct Proxy {
    display: Rc<Display>,
    raw: *mut WlProxy,
    id: ObjectId,
    interface: &'static Interface,
    version: u32,
    /// False for the display, which the connection owns.
    owned: bool,
}

impl fmt::Debug for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.interface.name, self.id.0)
    }
}

/// The parts of a request's signature: each argument kind, and whether it
/// may be null.
fn signature_args(signature: &CStr) -> Vec<(u8, bool)> {
    let mut args = Vec::new();
    let mut nullable = false;
    for &byte in signature.to_bytes() {
        match byte {
            b'0'..=b'9' => {}
            b'?' => nullable = true,
            kind => {
                args.push((kind, nullable));
                nullable = false;
            }
        }
    }
    args
}

/// The `since` version of a signature: its leading digits, or 1.
fn signature_since(signature: &CStr) -> u32 {
    let digits: String = signature
        .to_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .map(|&byte| char::from(byte))
        .collect();
    digits.parse().unwrap_or(1)
}

/// The `length` elements at `pointer`, or an empty slice for a null
/// pointer or a zero length, as the tables and libwayland pass them.
///
/// # Safety
///
/// A non-null `pointer` points to `length` initialized elements that stay
/// valid and unchanged for `'a`.
pub(crate) unsafe fn table<'a, T>(pointer: *const T, length: usize) -> &'a [T] {
    if pointer.is_null() || length == 0 {
        return &[];
    }
    // SAFETY: the caller guarantees `length` elements at `pointer`.
    unsafe { std::slice::from_raw_parts(pointer, length) }
}

/// Owned C strings that the marshalled arguments point into.
struct Keep {
    strings: Vec<CString>,
}

impl Proxy {
    pub(crate) fn id(&self) -> ObjectId {
        self.id
    }

    pub(crate) fn version(&self) -> u32 {
        self.version
    }

    pub(crate) fn interface(&self) -> &'static Interface {
        self.interface
    }

    /// The message `opcode` of the interface's requests.
    fn message(&self, opcode: u16) -> Result<&'static WlMessage, WaylandError> {
        let count = usize::try_from(self.interface.c.method_count).unwrap_or(0);
        if usize::from(opcode) >= count {
            return Err(self.bad(opcode, "no such request"));
        }
        // SAFETY: the methods table has `method_count` entries and is a
        // static of the hand-written tables.
        let methods = unsafe { table(self.interface.c.methods, count) };
        Ok(&methods[usize::from(opcode)])
    }

    fn bad(&self, opcode: u16, reason: &'static str) -> WaylandError {
        WaylandError::BadRequest {
            interface: self.interface.name,
            opcode,
            reason,
        }
    }

    /// Sends request `opcode` with `args`. Returns the new object when
    /// the request creates one. A destructor request must go through
    /// `destroy`.
    pub(crate) fn request(
        &self,
        opcode: u16,
        args: &[Arg<'_>],
    ) -> Result<Option<Proxy>, WaylandError> {
        if self.interface.destructors.get(usize::from(opcode)) == Some(&true) {
            return Err(self.bad(opcode, "a destructor request needs destroy"));
        }
        self.marshal(opcode, args, 0)
    }

    /// Sends the destructor request `opcode` with `args`, which destroys
    /// the object.
    pub(crate) fn destroy(mut self, opcode: u16, args: &[Arg<'_>]) -> Result<(), WaylandError> {
        if self.interface.destructors.get(usize::from(opcode)) != Some(&true) {
            return Err(self.bad(opcode, "not a destructor request"));
        }
        self.marshal(opcode, args, sys::WL_MARSHAL_FLAG_DESTROY)?;
        self.owned = false;
        Ok(())
    }

    fn marshal(
        &self,
        opcode: u16,
        args: &[Arg<'_>],
        flags: u32,
    ) -> Result<Option<Proxy>, WaylandError> {
        let message = self.message(opcode)?;
        // SAFETY: the tables hold NUL-terminated signature literals.
        let signature = unsafe { CStr::from_ptr(message.signature.cast()) };
        if signature_since(signature) > self.version {
            return Err(self.bad(opcode, "the object's version is too old for the request"));
        }
        let kinds = signature_args(signature);
        // `Bind` stands for three arguments: the name, the version, and
        // the new id.
        let mut expanded: Vec<Arg<'_>> = Vec::with_capacity(kinds.len());
        let mut created: Option<(&'static Interface, u32)> = None;
        for arg in args {
            if let Arg::Bind(interface, version) = arg {
                expanded.push(Arg::Str(Some(interface.name)));
                expanded.push(Arg::Uint(*version));
                expanded.push(Arg::Bind(interface, *version));
                created = Some((interface, *version));
            } else {
                expanded.push(*arg);
            }
        }
        if expanded.len() != kinds.len() {
            return Err(self.bad(opcode, "wrong number of arguments"));
        }
        let mut keep = Keep {
            strings: Vec::new(),
        };
        let mut wire = Vec::with_capacity(kinds.len());
        // SAFETY: the types table has one entry per argument.
        let types = unsafe { table(message.types, kinds.len()) };
        for (index, (arg, &(kind, nullable))) in expanded.iter().zip(&kinds).enumerate() {
            let typed = types[index];
            let value = match (kind, arg) {
                (b'i', Arg::Int(value)) => WlArgument { i: *value },
                (b'u', Arg::Uint(value)) => WlArgument { u: *value },
                (b's', Arg::Str(text)) => match text {
                    Some(text) => {
                        let text = CString::new(*text)
                            .map_err(|_| self.bad(opcode, "a string has an interior NUL"))?;
                        let pointer = text.as_ptr().cast::<CChar>();
                        keep.strings.push(text);
                        WlArgument { s: pointer }
                    }
                    None if nullable => WlArgument {
                        s: std::ptr::null(),
                    },
                    None => return Err(self.bad(opcode, "a string may not be null")),
                },
                (b'o', Arg::Object(object)) => match object {
                    Some(object) => {
                        if !typed.is_null() && !std::ptr::eq(typed, &raw const object.interface.c) {
                            return Err(self.bad(opcode, "an object has the wrong interface"));
                        }
                        WlArgument { o: object.raw }
                    }
                    None if nullable => WlArgument {
                        o: std::ptr::null_mut(),
                    },
                    None => return Err(self.bad(opcode, "an object may not be null")),
                },
                (b'n', Arg::NewId) if !typed.is_null() => {
                    // SAFETY: a non-null type is a pointer to the `c`
                    // field of a hand-written `Interface`, its first
                    // field.
                    let interface = unsafe { &*typed.cast::<Interface>() };
                    created = Some((interface, self.version));
                    WlArgument { n: 0 }
                }
                (b'n', Arg::Bind(..)) if typed.is_null() => WlArgument { n: 0 },
                (b'h', Arg::Fd(fd)) => WlArgument { h: fd.as_raw_fd() },
                _ => return Err(self.bad(opcode, "an argument does not match the signature")),
            };
            wire.push(value);
        }
        let (interface, version) = match created {
            Some((interface, version)) => (&raw const interface.c, version),
            None => (std::ptr::null(), self.version),
        };
        // SAFETY: the arguments match the signature of the request, the
        // strings in `keep` outlive the call, and libwayland copies them
        // into its buffer.
        let raw = unsafe {
            (self.display.lib.wl_proxy_marshal_array_flags)(
                self.raw,
                u32::from(opcode),
                interface,
                version,
                flags,
                wire.as_mut_ptr(),
            )
        };
        drop(keep);
        let Some((interface, version)) = created else {
            return Ok(None);
        };
        if raw.is_null() {
            return Err(WaylandError::NoObject(interface.name));
        }
        Ok(Some(Proxy::adopt(&self.display, raw, interface, version)))
    }

    /// Takes ownership of a new proxy: gives it an id and the dispatcher.
    fn adopt(
        display: &Rc<Display>,
        raw: *mut WlProxy,
        interface: &'static Interface,
        version: u32,
    ) -> Proxy {
        let id = display.next_id.get();
        display.next_id.set(id + 1);
        // SAFETY: `raw` is a live proxy that nothing else configured. The
        // dispatcher data is the display, which the proxy keeps alive
        // through its `Rc`, and the user data is the id.
        unsafe {
            (display.lib.wl_proxy_add_dispatcher)(
                raw,
                Some(dispatch),
                Rc::as_ptr(display).cast::<c_void>(),
                std::ptr::without_provenance_mut(id as usize),
            );
        }
        Proxy {
            display: Rc::clone(display),
            raw,
            id: ObjectId(id),
            interface,
            version,
            owned: true,
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
        // A destructor request without arguments destroys the object on
        // both sides.
        let destructor = self
            .interface
            .destructors
            .iter()
            .position(|&destructor| destructor);
        if let Some(opcode) = destructor.and_then(|opcode| u16::try_from(opcode).ok())
            && let Ok(message) = self.message(opcode)
        {
            // SAFETY: the tables hold NUL-terminated signature literals.
            let signature = unsafe { CStr::from_ptr(message.signature.cast()) };
            if signature_args(signature).is_empty() && signature_since(signature) <= self.version {
                // SAFETY: a destructor request without arguments; the
                // flag destroys the proxy after marshalling.
                unsafe {
                    (self.display.lib.wl_proxy_marshal_array_flags)(
                        self.raw,
                        u32::from(opcode),
                        std::ptr::null(),
                        self.version,
                        sys::WL_MARSHAL_FLAG_DESTROY,
                        std::ptr::null_mut(),
                    );
                }
                return;
            }
        }
        // SAFETY: the proxy is live and owned; nothing uses it after drop.
        unsafe { (self.display.lib.wl_proxy_destroy)(self.raw) }
    }
}

/// The id in the user data of a proxy, or 0 for one without (the
/// display).
///
/// # Safety
///
/// `proxy` is a live proxy of the display.
unsafe fn object_id(lib: &WaylandClient, proxy: *mut WlProxy) -> ObjectId {
    // SAFETY: the caller guarantees a live proxy.
    let data = unsafe { (lib.wl_proxy_get_user_data)(proxy) };
    ObjectId(data.addr() as u64)
}

/// Decodes the arguments of an event.
///
/// # Safety
///
/// `message` is the event's message from a hand-written table, and `args`
/// holds one argument for each entry of its signature, as libwayland
/// passes them to a dispatcher.
unsafe fn decode(display: &Rc<Display>, message: &WlMessage, args: *mut WlArgument) -> Vec<Value> {
    // SAFETY: the tables hold NUL-terminated signature literals.
    let signature = unsafe { CStr::from_ptr(message.signature.cast()) };
    let mut values = Vec::new();
    let kinds = signature_args(signature);
    // SAFETY: the caller guarantees an argument for each entry.
    let args = unsafe { table(args, kinds.len()) };
    // SAFETY: the types table has one entry per argument.
    let types = unsafe { table(message.types, kinds.len()) };
    for (index, (kind, _)) in kinds.into_iter().enumerate() {
        let arg = args[index];
        let value = match kind {
            b'i' => {
                // SAFETY: the signature names the field that libwayland
                // wrote.
                Value::Int(unsafe { arg.i })
            }
            b'u' => {
                // SAFETY: as above.
                Value::Uint(unsafe { arg.u })
            }
            b'f' => {
                // SAFETY: as above.
                Value::Fixed(f64::from(unsafe { arg.f }) / 256.0)
            }
            b's' => {
                // SAFETY: as above.
                let pointer = unsafe { arg.s };
                Value::Str(if pointer.is_null() {
                    None
                } else {
                    // SAFETY: a non-null string argument is a
                    // NUL-terminated string for the dispatch.
                    let text = unsafe { CStr::from_ptr(pointer.cast()) };
                    Some(text.to_string_lossy().into_owned())
                })
            }
            b'o' => {
                // SAFETY: as above.
                let pointer = unsafe { arg.o };
                Value::Object(if pointer.is_null() {
                    None
                } else {
                    // SAFETY: libwayland passes a live proxy.
                    Some(unsafe { object_id(display.lib, pointer) })
                })
            }
            b'n' => {
                // SAFETY: as above; libwayland passes the new proxy.
                let pointer = unsafe { arg.o };
                let typed = types[index];
                if pointer.is_null() || typed.is_null() {
                    Value::Object(None)
                } else {
                    // SAFETY: a non-null type is the first field of a
                    // hand-written `Interface`.
                    let interface = unsafe { &*typed.cast::<Interface>() };
                    // SAFETY: libwayland created the proxy for this event,
                    // and it has the version of its parent.
                    let version = unsafe { (display.lib.wl_proxy_get_version)(pointer) };
                    Value::NewId(Proxy::adopt(display, pointer, interface, version))
                }
            }
            b'a' => {
                // SAFETY: as above.
                let pointer = unsafe { arg.a };
                Value::Array(if pointer.is_null() {
                    Vec::new()
                } else {
                    // SAFETY: libwayland passes a valid array for the
                    // dispatch.
                    let array = unsafe { &*pointer };
                    if array.size == 0 || array.data.is_null() {
                        Vec::new()
                    } else {
                        // SAFETY: the array holds `size` bytes at `data`.
                        unsafe { table(array.data.cast::<u8>(), array.size) }.to_vec()
                    }
                })
            }
            b'h' => {
                // SAFETY: as above.
                let fd = unsafe { arg.h };
                // SAFETY: the client owns a received descriptor.
                Value::Fd(unsafe { OwnedFd::from_raw_fd(fd) })
            }
            _ => Value::Int(0),
        };
        values.push(value);
    }
    values
}

callback! {
    /// The dispatcher of every proxy: it queues the event on the
    /// connection.
    ///
    /// # Safety
    ///
    /// libwayland calls it with the dispatcher data of `adopt` (the
    /// display), a live proxy of that display, and the event's message and
    /// arguments.
    fn dispatch(
        data: *const c_void,
        target: *mut c_void,
        opcode: u32,
        message: *const WlMessage,
        args: *mut WlArgument,
    ) -> CInt [boundary "wayland dispatcher", fallback 0] {
        // SAFETY: the data is the display, which the target proxy keeps
        // alive; one more strong count makes a temporary `Rc`.
        unsafe { Rc::increment_strong_count(data.cast::<Display>()) };
        // SAFETY: the count above belongs to this `Rc`, which drops at
        // the end of the callback.
        let display = unsafe { Rc::from_raw(data.cast::<Display>()) };
        // SAFETY: libwayland passes a live proxy of the display.
        let object = unsafe { object_id(display.lib, target.cast::<WlProxy>()) };
        // SAFETY: the message is from a hand-written table.
        let message = unsafe { &*message };
        // SAFETY: libwayland passes the arguments of the message.
        let args = unsafe { decode(&display, message, args) };
        display.events.borrow_mut().push_back(Event {
            object,
            opcode: u16::try_from(opcode).unwrap_or(u16::MAX),
            args,
        });
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_split_into_arguments() {
        assert_eq!(
            signature_args(c"?oii"),
            vec![(b'o', true), (b'i', false), (b'i', false)]
        );
        assert_eq!(signature_args(c"4iiii").len(), 4);
        assert_eq!(signature_since(c"4iiii"), 4);
        assert_eq!(signature_since(c"usun"), 1);
    }

    #[test]
    fn tables_mark_their_destructors() {
        assert_eq!(WL_SURFACE.destructors.first(), Some(&true));
        assert!(!WL_SURFACE.destructors[1]);
        assert_eq!(WL_SURFACE.c.version, 6);
        assert_eq!(WL_SURFACE.name, "wl_surface");
    }
}
