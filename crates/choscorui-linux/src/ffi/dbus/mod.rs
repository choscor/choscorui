//! A safe interface to libdbus-1 for one thread that owns the connection.
//!
//! - `Connection` owns a private connection. Its drop closes the
//!   connection and gives back the last reference. Clones share it.
//! - libdbus asks for its descriptors and timers through the watch and
//!   timeout functions. The connection keeps them in a list, and the
//!   event loop of the caller waits on them: `watches`, `next_timeout`,
//!   `handle`, and `dispatch`.
//! - The C callbacks only queue: the filter queues signals for each
//!   matching `Subscription`, an object path handler queues method calls,
//!   and a pending call queues its reply. `dispatch` runs the method
//!   handlers after libdbus returns, so a handler can call the
//!   connection again.

mod message;
pub(crate) mod sys;

pub use message::{Message, MessageKind};

use crate::dbus::{MatchRule, Value, names};
use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::dl::LoadError;
use crate::ffi::macros::callback;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};
use sys::{
    DBusBusType, DBusConnection, DBusDispatchStatus, DBusError, DBusHandlerResult, DBusLib,
    DBusMessage, DBusObjectPathVTable, DBusPendingCall, DBusTimeout, DBusWatch,
};

/// What can go wrong with a connection or a call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// libdbus-1 did not load.
    Library(LoadError),
    /// No bus accepted the connection: the libdbus error name and text.
    Connect { name: String, message: String },
    /// The connection is closed.
    Disconnected,
    /// The peer answered with an error, or libdbus made one, such as
    /// `org.freedesktop.DBus.Error.NoReply` after the call timeout.
    Remote { name: String, message: String },
    /// No answer came in the time that the caller gave.
    TimedOut,
    /// A name, path, signature, or value breaks the D-Bus rules.
    InvalidArgument(String),
    /// libdbus ran out of memory.
    NoMemory,
    /// The wait for the connection failed.
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(error) => write!(f, "{error}"),
            Self::Connect { name, message } => {
                write!(f, "the D-Bus connection failed: {name}: {message}")
            }
            Self::Disconnected => f.write_str("the D-Bus connection is closed"),
            Self::Remote { name, message } => write!(f, "{name}: {message}"),
            Self::TimedOut => f.write_str("no D-Bus answer came in time"),
            Self::InvalidArgument(reason) => write!(f, "invalid D-Bus argument: {reason}"),
            Self::NoMemory => f.write_str("libdbus ran out of memory"),
            Self::Io(reason) => write!(f, "the D-Bus wait failed: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// The D-Bus error name of a remote error.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Remote { name, .. } | Self::Connect { name, .. } => Some(name),
            _ => None,
        }
    }
}

/// The libdbus table. The first call also turns on the thread support
/// of libdbus, because the tests and the services can own connections on
/// more than one thread.
pub(crate) fn lib() -> Result<&'static DBusLib, Error> {
    static THREADS: std::sync::Once = std::sync::Once::new();
    let lib = DBusLib::get().map_err(Error::Library)?;
    THREADS.call_once(|| {
        // SAFETY: plain call; libdbus allows it before any other use and
        // makes repeated calls a no-op.
        unsafe { (lib.dbus_threads_init_default)() };
    });
    Ok(lib)
}

/// A `DBusError` that frees itself.
struct ErrorSlot {
    lib: &'static DBusLib,
    raw: DBusError,
}

impl ErrorSlot {
    fn new(lib: &'static DBusLib) -> Box<Self> {
        let mut slot = Box::new(Self {
            lib,
            raw: DBusError::zeroed(),
        });
        // SAFETY: the storage is valid and stays at one address in the
        // box.
        unsafe { (lib.dbus_error_init)(&raw mut slot.raw) };
        slot
    }

    fn pointer(&mut self) -> *mut DBusError {
        &raw mut self.raw
    }

    /// The name and text of the error, if one is set.
    fn take(&mut self) -> Option<(String, String)> {
        // SAFETY: the error was initialized.
        if unsafe { (self.lib.dbus_error_is_set)(&raw const self.raw) } == 0 {
            return None;
        }
        let text = |pointer: *const CChar| {
            if pointer.is_null() {
                String::new()
            } else {
                // SAFETY: a set error holds NUL-terminated strings until
                // `dbus_error_free`.
                unsafe { CStr::from_ptr(pointer.cast()) }
                    .to_string_lossy()
                    .into_owned()
            }
        };
        Some((text(self.raw.name), text(self.raw.message)))
    }

    fn remote(&mut self) -> Error {
        self.take()
            .map_or(Error::NoMemory, |(name, message)| Error::Remote {
                name,
                message,
            })
    }
}

impl Drop for ErrorSlot {
    fn drop(&mut self) {
        // SAFETY: the error was initialized; freeing an unset error is a
        // no-op.
        unsafe { (self.lib.dbus_error_free)(&raw mut self.raw) }
    }
}

fn c_text(text: &str) -> Result<CString, Error> {
    CString::new(text).map_err(|_| Error::InvalidArgument(format!("{text:?} contains NUL")))
}

/// A timeout in milliseconds for libdbus.
fn millis(timeout: Duration) -> CInt {
    CInt::try_from(timeout.as_millis()).unwrap_or(sys::DBUS_TIMEOUT_INFINITE)
}

/// A descriptor that libdbus wants watched. The connection keeps a
/// duplicate of the descriptor, so it stays open while the caller holds
/// this value, even when libdbus closes its own.
#[derive(Clone, Debug)]
pub struct WatchFd {
    id: u64,
    fd: Rc<OwnedFd>,
    read: bool,
    write: bool,
}

impl WatchFd {
    /// True when libdbus waits for input.
    pub fn read(&self) -> bool {
        self.read
    }

    /// True when libdbus waits to write.
    pub fn write(&self) -> bool {
        self.write
    }
}

impl AsFd for WatchFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// What the wait saw on a `WatchFd`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Readiness {
    pub read: bool,
    pub write: bool,
    /// The peer hung up, or the descriptor has an error.
    pub hangup: bool,
}

struct WatchEntry {
    id: u64,
    raw: *mut DBusWatch,
    fd: Rc<OwnedFd>,
    flags: u32,
    enabled: bool,
}

struct TimeoutEntry {
    raw: *mut DBusTimeout,
    interval: Duration,
    enabled: bool,
    deadline: Instant,
}

/// The answer of a method handler.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// A method return with these values.
    Return(Vec<Value>),
    /// An error reply.
    Error { name: String, message: String },
    /// The handler sent its reply itself, or sends none.
    Sent,
}

impl Reply {
    /// An error reply.
    pub fn error(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            name: name.into(),
            message: message.into(),
        }
    }

    /// `org.freedesktop.DBus.Error.UnknownMethod` for `call`.
    pub fn unknown_method(call: &Message) -> Self {
        Self::error(
            "org.freedesktop.DBus.Error.UnknownMethod",
            format!(
                "no method {}.{}",
                call.interface().unwrap_or_default(),
                call.member().unwrap_or_default()
            ),
        )
    }

    /// `org.freedesktop.DBus.Error.InvalidArgs` with `message`.
    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::error("org.freedesktop.DBus.Error.InvalidArgs", message)
    }
}

/// A method handler of an exported object. It gets the connection, so
/// it must not keep a clone of it: the connection keeps the handler.
pub type MethodHandler = Rc<dyn Fn(&Connection, &Message) -> Reply>;

/// Identifies a call of `Connection::call_async`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CallId(u64);

type SignalQueue = Rc<RefCell<VecDeque<Message>>>;

struct SubscriptionEntry {
    id: u64,
    rule: MatchRule,
    queue: SignalQueue,
}

/// The state that the C callbacks reach. It never holds the connection,
/// so the callbacks keep no cycle alive.
struct State {
    lib: &'static DBusLib,
    next_id: Cell<u64>,
    watches: RefCell<Vec<WatchEntry>>,
    timeouts: RefCell<Vec<TimeoutEntry>>,
    subscriptions: RefCell<Vec<SubscriptionEntry>>,
    /// Method calls for exported objects: the export id and the call.
    calls: RefCell<VecDeque<(u64, Message)>>,
    exports: RefCell<HashMap<u64, MethodHandler>>,
    replies: RefCell<VecDeque<(CallId, Message)>>,
}

impl State {
    fn id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    fn route_signal(&self, message: &Message) {
        for entry in self.subscriptions.borrow().iter() {
            if entry.rule.matches(message) {
                entry.queue.borrow_mut().push_back(message.clone());
            }
        }
    }
}

/// The data of an exported object path in libdbus.
struct ExportData {
    state: Rc<State>,
    id: u64,
}

/// The data of a pending call in libdbus.
struct PendingData {
    state: Rc<State>,
    id: CallId,
}

/// Turns an `Rc<State>` into callback data; `free_state` gives it back.
fn state_data(state: &Rc<State>) -> *mut c_void {
    Rc::into_raw(Rc::clone(state)).cast_mut().cast()
}

/// Borrows the state of callback data.
///
/// # Safety
///
/// `data` came from `state_data`, and libdbus has not freed it.
unsafe fn state_ref<'a>(data: *mut c_void) -> &'a State {
    // SAFETY: the caller guarantees that libdbus still holds the count.
    unsafe { &*data.cast_const().cast::<State>() }
}

callback! {
    /// Gives back the state count of callback data.
    ///
    /// # Safety
    ///
    /// libdbus calls it one time with data from `state_data`.
    fn free_state(data: *mut c_void) [boundary "D-Bus free function", fallback ()] {
        // SAFETY: the count came from `Rc::into_raw` and is given back
        // one time.
        drop(unsafe { Rc::from_raw(data.cast_const().cast::<State>()) });
    }

    /// Keeps a new watch.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live watch and the state data.
    fn add_watch(watch: *mut DBusWatch, data: *mut c_void) -> u32
        [boundary "D-Bus add watch", fallback 0]
    {
        // SAFETY: libdbus passes the data of `set_watch_functions`.
        let state = unsafe { state_ref(data) };
        // SAFETY: the watch is live.
        let raw_fd = unsafe { (state.lib.dbus_watch_get_unix_fd)(watch) };
        if raw_fd < 0 {
            return 0;
        }
        // SAFETY: libdbus keeps the descriptor open for the call.
        let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        let Ok(fd) = borrowed.try_clone_to_owned() else {
            return 0;
        };
        // SAFETY: the watch is live.
        let flags = unsafe { (state.lib.dbus_watch_get_flags)(watch) };
        // SAFETY: the watch is live.
        let enabled = unsafe { (state.lib.dbus_watch_get_enabled)(watch) } != 0;
        let id = state.id();
        state.watches.borrow_mut().push(WatchEntry {
            id,
            raw: watch,
            fd: Rc::new(fd),
            flags,
            enabled,
        });
        1
    }

    /// Forgets a watch.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a watch and the state data.
    fn remove_watch(watch: *mut DBusWatch, data: *mut c_void)
        [boundary "D-Bus remove watch", fallback ()]
    {
        // SAFETY: libdbus passes the data of `set_watch_functions`.
        let state = unsafe { state_ref(data) };
        state.watches.borrow_mut().retain(|entry| entry.raw != watch);
    }

    /// Reads the new state of a watch.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live watch and the state data.
    fn toggle_watch(watch: *mut DBusWatch, data: *mut c_void)
        [boundary "D-Bus toggle watch", fallback ()]
    {
        // SAFETY: libdbus passes the data of `set_watch_functions`.
        let state = unsafe { state_ref(data) };
        // SAFETY: the watch is live.
        let enabled = unsafe { (state.lib.dbus_watch_get_enabled)(watch) } != 0;
        // SAFETY: the watch is live.
        let flags = unsafe { (state.lib.dbus_watch_get_flags)(watch) };
        for entry in state.watches.borrow_mut().iter_mut() {
            if entry.raw == watch {
                entry.enabled = enabled;
                entry.flags = flags;
            }
        }
    }

    /// Keeps a new timeout.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live timeout and the state data.
    fn add_timeout(timeout: *mut DBusTimeout, data: *mut c_void) -> u32
        [boundary "D-Bus add timeout", fallback 0]
    {
        // SAFETY: libdbus passes the data of `set_timeout_functions`.
        let state = unsafe { state_ref(data) };
        // SAFETY: the timeout is live.
        let interval = unsafe { (state.lib.dbus_timeout_get_interval)(timeout) };
        // SAFETY: the timeout is live.
        let enabled = unsafe { (state.lib.dbus_timeout_get_enabled)(timeout) } != 0;
        let interval = Duration::from_millis(u64::try_from(interval).unwrap_or(0));
        state.timeouts.borrow_mut().push(TimeoutEntry {
            raw: timeout,
            interval,
            enabled,
            deadline: Instant::now() + interval,
        });
        1
    }

    /// Forgets a timeout.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a timeout and the state data.
    fn remove_timeout(timeout: *mut DBusTimeout, data: *mut c_void)
        [boundary "D-Bus remove timeout", fallback ()]
    {
        // SAFETY: libdbus passes the data of `set_timeout_functions`.
        let state = unsafe { state_ref(data) };
        state.timeouts.borrow_mut().retain(|entry| entry.raw != timeout);
    }

    /// Reads the new state of a timeout and restarts its interval.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live timeout and the state data.
    fn toggle_timeout(timeout: *mut DBusTimeout, data: *mut c_void)
        [boundary "D-Bus toggle timeout", fallback ()]
    {
        // SAFETY: libdbus passes the data of `set_timeout_functions`.
        let state = unsafe { state_ref(data) };
        // SAFETY: the timeout is live.
        let enabled = unsafe { (state.lib.dbus_timeout_get_enabled)(timeout) } != 0;
        // SAFETY: the timeout is live.
        let interval = unsafe { (state.lib.dbus_timeout_get_interval)(timeout) };
        let interval = Duration::from_millis(u64::try_from(interval).unwrap_or(0));
        for entry in state.timeouts.borrow_mut().iter_mut() {
            if entry.raw == timeout {
                entry.enabled = enabled;
                entry.interval = interval;
                entry.deadline = Instant::now() + interval;
            }
        }
    }

    /// Queues each signal for the subscriptions that match it.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live message and the state data.
    fn filter(
        _connection: *mut DBusConnection,
        message: *mut DBusMessage,
        data: *mut c_void,
    ) -> DBusHandlerResult [
        boundary "D-Bus filter",
        fallback DBusHandlerResult::DBUS_HANDLER_RESULT_NOT_YET_HANDLED
    ] {
        // SAFETY: libdbus passes the data of `add_filter`.
        let state = unsafe { state_ref(data) };
        // SAFETY: the message is live for the call.
        if let Some(message) = unsafe { Message::borrowed(state.lib, message) }
            && message.kind() == Some(MessageKind::Signal)
        {
            state.route_signal(&message);
        }
        DBusHandlerResult::DBUS_HANDLER_RESULT_NOT_YET_HANDLED
    }

    /// Queues a method call for an exported object.
    ///
    /// # Safety
    ///
    /// libdbus calls it with a live message and the data of the export.
    fn object_message(
        _connection: *mut DBusConnection,
        message: *mut DBusMessage,
        data: *mut c_void,
    ) -> DBusHandlerResult [
        boundary "D-Bus object path handler",
        fallback DBusHandlerResult::DBUS_HANDLER_RESULT_NOT_YET_HANDLED
    ] {
        // SAFETY: libdbus passes the data of the registration, which it
        // frees only in `object_unregister`.
        let export = unsafe { &*data.cast_const().cast::<ExportData>() };
        // SAFETY: the message is live for the call.
        let Some(message) = (unsafe { Message::borrowed(export.state.lib, message) }) else {
            return DBusHandlerResult::DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
        };
        if message.kind() != Some(MessageKind::MethodCall) {
            return DBusHandlerResult::DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
        }
        export.state.calls.borrow_mut().push_back((export.id, message));
        DBusHandlerResult::DBUS_HANDLER_RESULT_HANDLED
    }

    /// Frees the data of an export when libdbus drops the registration.
    ///
    /// # Safety
    ///
    /// libdbus calls it one time with the data of the export.
    fn object_unregister(_connection: *mut DBusConnection, data: *mut c_void)
        [boundary "D-Bus object path unregister", fallback ()]
    {
        // SAFETY: the data came from `Box::into_raw` and is freed one time.
        drop(unsafe { Box::from_raw(data.cast::<ExportData>()) });
    }

    /// Queues the reply of a pending call.
    ///
    /// # Safety
    ///
    /// libdbus calls it with the completed call and its data.
    fn pending_notify(pending: *mut DBusPendingCall, data: *mut c_void)
        [boundary "D-Bus pending call notify", fallback ()]
    {
        // SAFETY: libdbus passes the data of `set_notify`, which it frees
        // only in `free_pending`.
        let call = unsafe { &*data.cast_const().cast::<PendingData>() };
        // SAFETY: the call is complete; the reply reference moves to the
        // caller.
        let reply = unsafe { (call.state.lib.dbus_pending_call_steal_reply)(pending) };
        if let Some(reply) = Message::adopt(call.state.lib, reply) {
            call.state.replies.borrow_mut().push_back((call.id, reply));
        }
    }

    /// Frees the data of a pending call.
    ///
    /// # Safety
    ///
    /// libdbus calls it one time with the data of `set_notify`.
    fn free_pending(data: *mut c_void) [boundary "D-Bus free function", fallback ()] {
        // SAFETY: the data came from `Box::into_raw` and is freed one time.
        drop(unsafe { Box::from_raw(data.cast::<PendingData>()) });
    }
}

/// The handlers of every exported object path.
static OBJECT_VTABLE: DBusObjectPathVTable = DBusObjectPathVTable {
    unregister_function: Some(object_unregister),
    message_function: Some(object_message),
    dbus_internal_pad1: None,
    dbus_internal_pad2: None,
    dbus_internal_pad3: None,
    dbus_internal_pad4: None,
};

struct Inner {
    lib: &'static DBusLib,
    raw: NonNull<DBusConnection>,
    state: Rc<State>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // SAFETY: the connection is private and open or closed; closing
        // it again is allowed.
        unsafe { (self.lib.dbus_connection_close)(self.raw.as_ptr()) };
        // SAFETY: the connection holds the reference of its creation,
        // which it gives back. libdbus then frees the callback data.
        unsafe { (self.lib.dbus_connection_unref)(self.raw.as_ptr()) };
    }
}

/// A private D-Bus connection that one thread owns.
#[derive(Clone)]
pub struct Connection {
    inner: Rc<Inner>,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("unique_name", &self.unique_name())
            .finish_non_exhaustive()
    }
}

/// How `Connection::request_name` asks for a name.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NameFlags {
    pub allow_replacement: bool,
    pub replace_existing: bool,
    pub do_not_queue: bool,
}

/// The answer of the bus to `Connection::request_name`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameReply {
    PrimaryOwner,
    InQueue,
    Exists,
    AlreadyOwner,
}

impl Connection {
    /// A new private connection to the session bus.
    pub fn session() -> Result<Self, Error> {
        let lib = lib()?;
        let mut error = ErrorSlot::new(lib);
        // SAFETY: the error is initialized storage for the call.
        let raw =
            unsafe { (lib.dbus_bus_get_private)(DBusBusType::DBUS_BUS_SESSION, error.pointer()) };
        let Some(raw) = NonNull::new(raw) else {
            return Err(connect_error(&mut error));
        };
        Self::wrap(lib, raw)
    }

    /// A new private connection to the bus at `address`, registered with
    /// `Hello`. The accessibility bus is such a bus.
    pub fn open_private(address: &str) -> Result<Self, Error> {
        let lib = lib()?;
        let address = c_text(address)?;
        let mut error = ErrorSlot::new(lib);
        // SAFETY: the address is NUL-terminated and the error is
        // initialized storage for the call.
        let raw =
            unsafe { (lib.dbus_connection_open_private)(address.as_ptr().cast(), error.pointer()) };
        let Some(raw) = NonNull::new(raw) else {
            return Err(connect_error(&mut error));
        };
        let connection = Self::wrap(lib, raw)?;
        // SAFETY: the connection is open and the error is initialized.
        let registered = unsafe { (lib.dbus_bus_register)(raw.as_ptr(), error.pointer()) };
        if registered == 0 {
            return Err(connect_error(&mut error));
        }
        Ok(connection)
    }

    /// Takes over a new private connection and installs the callbacks.
    fn wrap(lib: &'static DBusLib, raw: NonNull<DBusConnection>) -> Result<Self, Error> {
        let state = Rc::new(State {
            lib,
            next_id: Cell::new(1),
            watches: RefCell::default(),
            timeouts: RefCell::default(),
            subscriptions: RefCell::default(),
            calls: RefCell::default(),
            exports: RefCell::default(),
            replies: RefCell::default(),
        });
        let connection = Self {
            inner: Rc::new(Inner {
                lib,
                raw,
                state: Rc::clone(&state),
            }),
        };
        // SAFETY: the connection is open; a closed bus must not end the
        // process.
        unsafe { (lib.dbus_connection_set_exit_on_disconnect)(raw.as_ptr(), 0) };
        let data = state_data(&state);
        // SAFETY: the callbacks match the libdbus types, and libdbus
        // frees `data` with `free_state`.
        let watching = unsafe {
            (lib.dbus_connection_set_watch_functions)(
                raw.as_ptr(),
                Some(add_watch),
                Some(remove_watch),
                Some(toggle_watch),
                data,
                Some(free_state),
            )
        };
        if watching == 0 {
            // SAFETY: libdbus did not keep the data, so it is freed here.
            unsafe { free_state(data) };
            return Err(Error::NoMemory);
        }
        let data = state_data(&state);
        // SAFETY: as above.
        let timing = unsafe {
            (lib.dbus_connection_set_timeout_functions)(
                raw.as_ptr(),
                Some(add_timeout),
                Some(remove_timeout),
                Some(toggle_timeout),
                data,
                Some(free_state),
            )
        };
        if timing == 0 {
            // SAFETY: libdbus did not keep the data, so it is freed here.
            unsafe { free_state(data) };
            return Err(Error::NoMemory);
        }
        let data = state_data(&state);
        // SAFETY: as above.
        let filtering = unsafe {
            (lib.dbus_connection_add_filter)(raw.as_ptr(), Some(filter), data, Some(free_state))
        };
        if filtering == 0 {
            // SAFETY: libdbus did not keep the data, so it is freed here.
            unsafe { free_state(data) };
            return Err(Error::NoMemory);
        }
        Ok(connection)
    }

    fn raw(&self) -> *mut DBusConnection {
        self.inner.raw.as_ptr()
    }

    fn lib(&self) -> &'static DBusLib {
        self.inner.lib
    }

    fn state(&self) -> &State {
        &self.inner.state
    }

    /// The unique name that the bus gave the connection, such as `:1.7`.
    pub fn unique_name(&self) -> Option<String> {
        // SAFETY: the connection is live.
        let pointer = unsafe { (self.lib().dbus_bus_get_unique_name)(self.raw()) };
        if pointer.is_null() {
            return None;
        }
        // SAFETY: the name belongs to the connection, which outlives the
        // copy.
        let name = unsafe { CStr::from_ptr(pointer.cast()) };
        Some(name.to_string_lossy().into_owned())
    }

    pub fn is_connected(&self) -> bool {
        // SAFETY: the connection is live.
        unsafe { (self.lib().dbus_connection_get_is_connected)(self.raw()) != 0 }
    }

    /// Asks the bus for the well-known name `name`. It blocks until the
    /// bus answers.
    pub fn request_name(&self, name: &str, flags: NameFlags) -> Result<NameReply, Error> {
        if !names::is_bus_name(name) || name.starts_with(':') {
            return Err(Error::InvalidArgument(format!(
                "{name:?} is not a well-known name"
            )));
        }
        let text = c_text(name)?;
        let bits = (if flags.allow_replacement {
            sys::DBUS_NAME_FLAG_ALLOW_REPLACEMENT
        } else {
            0
        }) | (if flags.replace_existing {
            sys::DBUS_NAME_FLAG_REPLACE_EXISTING
        } else {
            0
        }) | (if flags.do_not_queue {
            sys::DBUS_NAME_FLAG_DO_NOT_QUEUE
        } else {
            0
        });
        let mut error = ErrorSlot::new(self.lib());
        // SAFETY: the name is NUL-terminated and the error initialized.
        let reply = unsafe {
            (self.lib().dbus_bus_request_name)(
                self.raw(),
                text.as_ptr().cast(),
                bits,
                error.pointer(),
            )
        };
        match reply {
            sys::DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER => Ok(NameReply::PrimaryOwner),
            sys::DBUS_REQUEST_NAME_REPLY_IN_QUEUE => Ok(NameReply::InQueue),
            sys::DBUS_REQUEST_NAME_REPLY_EXISTS => Ok(NameReply::Exists),
            sys::DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER => Ok(NameReply::AlreadyOwner),
            _ => Err(error.remote()),
        }
    }

    /// Gives a well-known name back to the bus. It blocks until the bus
    /// answers.
    pub fn release_name(&self, name: &str) -> Result<(), Error> {
        if !names::is_bus_name(name) || name.starts_with(':') {
            return Err(Error::InvalidArgument(format!(
                "{name:?} is not a well-known name"
            )));
        }
        let text = c_text(name)?;
        let mut error = ErrorSlot::new(self.lib());
        // SAFETY: the name is NUL-terminated and the error initialized.
        let reply = unsafe {
            (self.lib().dbus_bus_release_name)(self.raw(), text.as_ptr().cast(), error.pointer())
        };
        if reply < 0 {
            Err(error.remote())
        } else {
            Ok(())
        }
    }

    /// True when a connection owns `name`. It blocks until the bus
    /// answers.
    pub fn name_has_owner(&self, name: &str) -> Result<bool, Error> {
        if !names::is_bus_name(name) {
            return Err(Error::InvalidArgument(format!(
                "{name:?} is not a bus name"
            )));
        }
        let text = c_text(name)?;
        let mut error = ErrorSlot::new(self.lib());
        // SAFETY: the name is NUL-terminated and the error initialized.
        let owned = unsafe {
            (self.lib().dbus_bus_name_has_owner)(self.raw(), text.as_ptr().cast(), error.pointer())
        };
        match error.take() {
            Some((name, message)) => Err(Error::Remote { name, message }),
            None => Ok(owned != 0),
        }
    }

    /// Queues `message` for sending and returns its serial. libdbus
    /// writes what it can at once; the write watch sends the rest.
    pub fn send(&self, message: Message) -> Result<u32, Error> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        let mut serial = 0u32;
        // SAFETY: the connection and the message are live; libdbus takes
        // its own reference to the message.
        let sent = unsafe {
            (self.lib().dbus_connection_send)(self.raw(), message.raw(), &raw mut serial)
        };
        if sent == 0 {
            Err(Error::NoMemory)
        } else {
            Ok(serial)
        }
    }

    /// Blocks until every queued message is written.
    pub fn flush(&self) {
        // SAFETY: the connection is live.
        unsafe { (self.lib().dbus_connection_flush)(self.raw()) }
    }

    /// Sends the method call `message` and blocks until its reply or
    /// `timeout`. An error reply is `Error::Remote`. Other messages that
    /// arrive meanwhile wait for the next `dispatch`.
    pub fn call(&self, message: Message, timeout: Duration) -> Result<Message, Error> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        let mut error = ErrorSlot::new(self.lib());
        // SAFETY: the connection and the message are live, and the error
        // is initialized; the reply reference moves to the caller.
        let reply = unsafe {
            (self.lib().dbus_connection_send_with_reply_and_block)(
                self.raw(),
                message.raw(),
                millis(timeout),
                error.pointer(),
            )
        };
        Message::adopt(self.lib(), reply).ok_or_else(|| error.remote())
    }

    /// Sends the method call `message` without a wait. The reply, or the
    /// `NoReply` error after `timeout`, arrives in a later `dispatch`:
    /// read it with `take_reply` or `wait_reply`.
    pub fn call_async(&self, message: Message, timeout: Duration) -> Result<CallId, Error> {
        let mut pending: *mut DBusPendingCall = std::ptr::null_mut();
        // SAFETY: the connection and the message are live; `pending` is
        // storage for the new call reference.
        let sent = unsafe {
            (self.lib().dbus_connection_send_with_reply)(
                self.raw(),
                message.raw(),
                &raw mut pending,
                millis(timeout),
            )
        };
        if sent == 0 {
            return Err(Error::NoMemory);
        }
        if pending.is_null() {
            return Err(Error::Disconnected);
        }
        let id = CallId(self.state().id());
        let data = Box::into_raw(Box::new(PendingData {
            state: Rc::clone(&self.inner.state),
            id,
        }));
        // SAFETY: the call is live, and libdbus frees `data` with
        // `free_pending`.
        let noted = unsafe {
            (self.lib().dbus_pending_call_set_notify)(
                pending,
                Some(pending_notify),
                data.cast(),
                Some(free_pending),
            )
        };
        if noted == 0 {
            // SAFETY: libdbus did not keep the data, so it is freed here.
            unsafe { free_pending(data.cast()) };
        }
        // SAFETY: the connection keeps its own reference until the call
        // completes; this gives back the reference of the caller.
        unsafe { (self.lib().dbus_pending_call_unref)(pending) };
        if noted == 0 {
            Err(Error::NoMemory)
        } else {
            Ok(id)
        }
    }

    /// The reply of `call`, when it arrived. An error reply is
    /// `Error::Remote`.
    pub fn take_reply(&self, call: CallId) -> Option<Result<Message, Error>> {
        let mut replies = self.state().replies.borrow_mut();
        let index = replies.iter().position(|(id, _)| *id == call)?;
        let (_, reply) = replies.remove(index)?;
        Some(match reply.as_error() {
            Some(error) => Err(error),
            None => Ok(reply),
        })
    }

    /// Every reply that arrived, oldest first.
    pub fn take_replies(&self) -> Vec<(CallId, Result<Message, Error>)> {
        let replies: Vec<(CallId, Message)> = self.state().replies.borrow_mut().drain(..).collect();
        replies
            .into_iter()
            .map(|(id, reply)| {
                let result = match reply.as_error() {
                    Some(error) => Err(error),
                    None => Ok(reply),
                };
                (id, result)
            })
            .collect()
    }

    /// Asks the bus for the signals that match `rule`, and queues them
    /// for the returned subscription. It blocks until the bus answers.
    pub fn subscribe(&self, rule: MatchRule) -> Result<Subscription, Error> {
        let text = c_text(&rule.to_rule()?)?;
        let mut error = ErrorSlot::new(self.lib());
        // SAFETY: the rule is NUL-terminated and the error initialized.
        unsafe {
            (self.lib().dbus_bus_add_match)(self.raw(), text.as_ptr().cast(), error.pointer())
        };
        if let Some((name, message)) = error.take() {
            return Err(Error::Remote { name, message });
        }
        let id = self.state().id();
        let queue = SignalQueue::default();
        self.state()
            .subscriptions
            .borrow_mut()
            .push(SubscriptionEntry {
                id,
                rule,
                queue: Rc::clone(&queue),
            });
        Ok(Subscription {
            id,
            text,
            queue,
            connection: Rc::downgrade(&self.inner),
        })
    }

    /// Sends the method calls of the object `path` to `handler`.
    pub fn export(&self, path: &str, handler: MethodHandler) -> Result<Export, Error> {
        self.register(path, handler, false)
    }

    /// Sends the method calls of `path` and of every path below it to
    /// `handler`.
    pub fn export_fallback(&self, path: &str, handler: MethodHandler) -> Result<Export, Error> {
        self.register(path, handler, true)
    }

    fn register(
        &self,
        path: &str,
        handler: MethodHandler,
        fallback: bool,
    ) -> Result<Export, Error> {
        if !names::is_object_path(path) {
            return Err(Error::InvalidArgument(format!(
                "{path:?} is not an object path"
            )));
        }
        let text = c_text(path)?;
        let id = self.state().id();
        let data = Box::into_raw(Box::new(ExportData {
            state: Rc::clone(&self.inner.state),
            id,
        }));
        let register = if fallback {
            self.lib().dbus_connection_register_fallback
        } else {
            self.lib().dbus_connection_register_object_path
        };
        // SAFETY: the path is NUL-terminated, the table is static, and
        // libdbus frees `data` with `object_unregister`.
        let registered = unsafe {
            register(
                self.raw(),
                text.as_ptr().cast(),
                &raw const OBJECT_VTABLE,
                data.cast(),
            )
        };
        if registered == 0 {
            // SAFETY: libdbus did not keep the data, so it is freed here.
            drop(unsafe { Box::from_raw(data) });
            return Err(Error::InvalidArgument(format!(
                "{path} is already exported"
            )));
        }
        self.state().exports.borrow_mut().insert(id, handler);
        Ok(Export {
            id,
            path: text,
            connection: Rc::downgrade(&self.inner),
        })
    }

    /// The descriptors that libdbus waits on now.
    pub fn watches(&self) -> Vec<WatchFd> {
        self.state()
            .watches
            .borrow()
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| WatchFd {
                id: entry.id,
                fd: Rc::clone(&entry.fd),
                read: entry.flags & sys::DBUS_WATCH_READABLE != 0,
                write: entry.flags & sys::DBUS_WATCH_WRITABLE != 0,
            })
            .collect()
    }

    /// The longest wait before `dispatch` has work: zero when messages
    /// wait, the time to the next libdbus timeout, or `None`.
    pub fn next_timeout(&self) -> Option<Duration> {
        if !self.state().calls.borrow().is_empty() {
            return Some(Duration::ZERO);
        }
        // SAFETY: the connection is live.
        let status = unsafe { (self.lib().dbus_connection_get_dispatch_status)(self.raw()) };
        if status != DBusDispatchStatus::DBUS_DISPATCH_COMPLETE {
            return Some(Duration::ZERO);
        }
        let now = Instant::now();
        self.state()
            .timeouts
            .borrow()
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.deadline.saturating_duration_since(now))
            .min()
    }

    /// Lets libdbus read or write `watch` after the wait saw `ready`.
    pub fn handle(&self, watch: &WatchFd, ready: Readiness) {
        let mut flags = 0;
        if ready.read {
            flags |= sys::DBUS_WATCH_READABLE;
        }
        if ready.write {
            flags |= sys::DBUS_WATCH_WRITABLE;
        }
        if ready.hangup {
            flags |= sys::DBUS_WATCH_ERROR | sys::DBUS_WATCH_HANGUP;
        }
        if flags == 0 {
            return;
        }
        let raw = self
            .state()
            .watches
            .borrow()
            .iter()
            .find(|entry| entry.id == watch.id && entry.enabled)
            .map(|entry| entry.raw);
        if let Some(raw) = raw {
            // SAFETY: the watch is still in the list, so it is live; no
            // borrow of the list is held while libdbus changes it.
            unsafe { (self.lib().dbus_watch_handle)(raw, flags) };
        }
    }

    /// Runs the libdbus timeouts that are due.
    fn fire_timeouts(&self) {
        let now = Instant::now();
        let due: Vec<*mut DBusTimeout> = self
            .state()
            .timeouts
            .borrow()
            .iter()
            .filter(|entry| entry.enabled && entry.deadline <= now)
            .map(|entry| entry.raw)
            .collect();
        for raw in due {
            // An earlier timeout can remove this one, so look it up again.
            let live = self
                .state()
                .timeouts
                .borrow_mut()
                .iter_mut()
                .find(|entry| entry.raw == raw && entry.enabled)
                .map(|entry| entry.deadline = now + entry.interval)
                .is_some();
            if live {
                // SAFETY: the timeout is in the list, so it is live; no
                // borrow of the list is held.
                unsafe { (self.lib().dbus_timeout_handle)(raw) };
            }
        }
    }

    /// Runs the due timeouts, turns the read data into messages, and
    /// runs the method handlers. `Error::Disconnected` after the bus or
    /// the peer closed the connection.
    pub fn dispatch(&self) -> Result<(), Error> {
        self.fire_timeouts();
        loop {
            // SAFETY: the connection is live; the callbacks only queue.
            let status = unsafe { (self.lib().dbus_connection_dispatch)(self.raw()) };
            if status != DBusDispatchStatus::DBUS_DISPATCH_DATA_REMAINS {
                break;
            }
        }
        loop {
            let call = self.state().calls.borrow_mut().pop_front();
            let Some((id, message)) = call else {
                break;
            };
            let handler = self.state().exports.borrow().get(&id).cloned();
            let reply = match handler {
                Some(handler) => handler(self, &message),
                None => Reply::error(
                    "org.freedesktop.DBus.Error.UnknownObject",
                    "the object is no longer exported",
                ),
            };
            self.send_reply(&message, reply)?;
        }
        if self.is_connected() {
            Ok(())
        } else {
            Err(Error::Disconnected)
        }
    }

    /// Sends `reply` to `call`, unless the caller wants no reply.
    pub fn send_reply(&self, call: &Message, reply: Reply) -> Result<(), Error> {
        if call.no_reply() {
            return Ok(());
        }
        let message = match reply {
            Reply::Sent => return Ok(()),
            Reply::Return(values) => {
                match call.method_return()?.with(&values) {
                    Ok(message) => message,
                    // A handler returned values that break the rules.
                    Err(error) => {
                        call.error_reply("org.freedesktop.DBus.Error.Failed", &error.to_string())?
                    }
                }
            }
            Reply::Error { name, message } => match call.error_reply(&name, &message) {
                Ok(reply) => reply,
                Err(_) => call.error_reply("org.freedesktop.DBus.Error.Failed", &message)?,
            },
        };
        match self.send(message) {
            Ok(_) | Err(Error::Disconnected) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// One turn of a loop that waits only on this connection: it waits
    /// up to `timeout` (forever for `None`) for the connection, then
    /// handles what came and dispatches. Messages that wait in the queue,
    /// for example from a blocking `call`, end the wait at once, so a
    /// caller sees their effects after the turn instead of waiting on a
    /// quiet socket.
    pub fn turn(&self, timeout: Option<Duration>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        let watches = self.watches();
        let wait = match (self.next_timeout(), timeout) {
            (Some(own), Some(limit)) => Some(own.min(limit)),
            (own, limit) => own.or(limit),
        };
        let list: Vec<crate::ffi::libc::Watch<'_>> = watches
            .iter()
            .map(|watch| crate::ffi::libc::Watch {
                fd: watch.as_fd(),
                read: watch.read,
                write: watch.write,
            })
            .collect();
        let ready =
            crate::ffi::libc::wait(&list, wait).map_err(|error| Error::Io(error.to_string()))?;
        drop(list);
        for (watch, ready) in watches.iter().zip(ready) {
            self.handle(
                watch,
                Readiness {
                    read: ready.read,
                    write: ready.write,
                    hangup: ready.hangup,
                },
            );
        }
        self.dispatch()
    }

    /// Turns the loop until `done` returns a value or `timeout` passes.
    pub fn run_until<T>(
        &self,
        timeout: Option<Duration>,
        mut done: impl FnMut(&Self) -> Option<T>,
    ) -> Result<T, Error> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        loop {
            if let Some(value) = done(self) {
                return Ok(value);
            }
            let remaining = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(Error::TimedOut);
                    }
                    Some(left)
                }
                None => None,
            };
            self.turn(remaining)?;
        }
    }

    /// Waits in a nested loop for the reply of `call`.
    pub fn wait_reply(&self, call: CallId, timeout: Option<Duration>) -> Result<Message, Error> {
        self.run_until(timeout, |connection| connection.take_reply(call))?
    }

    /// Waits in a nested loop for the next signal of `subscription`, such
    /// as the `Response` of a portal request.
    pub fn wait_signal(
        &self,
        subscription: &Subscription,
        timeout: Option<Duration>,
    ) -> Result<Message, Error> {
        self.run_until(timeout, |_| subscription.next())
    }
}

fn connect_error(error: &mut ErrorSlot) -> Error {
    match error.take() {
        Some((name, message)) => Error::Connect { name, message },
        None => Error::NoMemory,
    }
}

/// The signals of one match rule. Its drop removes the rule from the bus.
pub struct Subscription {
    id: u64,
    text: CString,
    queue: SignalQueue,
    connection: Weak<Inner>,
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("rule", &self.text)
            .finish_non_exhaustive()
    }
}

impl Subscription {
    /// The oldest queued signal.
    pub fn next(&self) -> Option<Message> {
        self.queue.borrow_mut().pop_front()
    }

    /// Every queued signal, oldest first.
    pub fn take(&self) -> Vec<Message> {
        self.queue.borrow_mut().drain(..).collect()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let Some(inner) = self.connection.upgrade() else {
            return;
        };
        inner
            .state
            .subscriptions
            .borrow_mut()
            .retain(|entry| entry.id != self.id);
        // SAFETY: the connection is live and the rule NUL-terminated; a
        // null error makes the call send without a wait.
        unsafe {
            (inner.lib.dbus_bus_remove_match)(
                inner.raw.as_ptr(),
                self.text.as_ptr().cast(),
                std::ptr::null_mut(),
            );
        }
    }
}

/// An exported object path. Its drop removes the object.
pub struct Export {
    id: u64,
    path: CString,
    connection: Weak<Inner>,
}

impl fmt::Debug for Export {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Export")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Drop for Export {
    fn drop(&mut self) {
        let Some(inner) = self.connection.upgrade() else {
            return;
        };
        inner.state.exports.borrow_mut().remove(&self.id);
        // SAFETY: the connection is live and the path NUL-terminated;
        // libdbus calls `object_unregister`, which frees the data.
        unsafe {
            (inner.lib.dbus_connection_unregister_object_path)(
                inner.raw.as_ptr(),
                self.path.as_ptr().cast(),
            );
        }
    }
}
