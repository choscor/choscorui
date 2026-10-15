//! The declarations of libX11 for XIM, a safe input method on its own
//! Xlib connection, and the Xlib connection of the EGL display.
//!
//! The X11 backend reads its events through xcb. XIM needs Xlib: the
//! input method protocol and its text conversions live there. The XIM
//! transport waits for its replies in `XIfEvent`, so Xlib must own the
//! event queue of its connection. With `XCBOwnsEventQueue`, `XIfEvent`
//! never gets an event and spins. Thus the input method opens a second
//! connection with `XOpenDisplay`, and the input contexts name the xcb
//! windows. The client gives each key event of an active window to
//! `XFilterEvent`. The input method sends back the keys that it does not
//! use and the committed text on the Xlib connection, which the event loop
//! drains. The preedit callbacks queue their changes on the thread, for
//! the client to read.
//!
//! The connection registers an instantiate callback while it has no input
//! method, so a server that starts or restarts opens again. The destroy
//! callback of an input method marks it dead: Xlib then destroyed it and
//! its contexts, and the client must not close them again.

use super::ctypes::{CChar, CInt, CLong, CUlong, c_void};
use super::macros::{c_consts, callback, ffi_struct, ffi_union, libraries, opaque_types};
use std::cell::{Cell, RefCell};
use std::os::fd::BorrowedFd;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

opaque_types! {
    /// `Display`.
    pub(crate) XDisplay = "Display";
    /// `struct _XIM`, the target of `XIM`.
    pub(crate) XimRec = "struct _XIM";
    /// `struct _XIC`, the target of `XIC`.
    pub(crate) XicRec = "struct _XIC";
    /// `struct _XrmHashBucketRec`, the target of `XrmDatabase`.
    pub(crate) XrmHashBucketRec = "struct _XrmHashBucketRec";
}

/// `XIMProc` with the input context first, as Xlib calls it for the
/// preedit callbacks.
pub(crate) type XimProc = Option<unsafe extern "C" fn(*mut XicRec, *mut CChar, *mut CChar)>;

/// `XIMProc` with the input method first, as Xlib calls it for the
/// destroy callback of an input method.
pub(crate) type XimDestroyProc = Option<unsafe extern "C" fn(*mut XimRec, *mut CChar, *mut CChar)>;

/// `XICProc`.
pub(crate) type XicProc = Option<unsafe extern "C" fn(*mut XicRec, *mut CChar, *mut CChar) -> CInt>;

/// `XIDProc`, the instantiate callback.
pub(crate) type XidProc = Option<unsafe extern "C" fn(*mut XDisplay, *mut CChar, *mut CChar)>;

ffi_struct! {
    /// `XKeyEvent`.
    pub(crate) struct XKeyEvent = "XKeyEvent" {
        pub(crate) r#type: CInt,
        pub(crate) serial: CUlong,
        pub(crate) send_event: CInt,
        pub(crate) display: *mut XDisplay,
        pub(crate) window: CUlong,
        pub(crate) root: CUlong,
        pub(crate) subwindow: CUlong,
        pub(crate) time: CUlong,
        pub(crate) x: CInt,
        pub(crate) y: CInt,
        pub(crate) x_root: CInt,
        pub(crate) y_root: CInt,
        pub(crate) state: u32,
        pub(crate) keycode: u32,
        pub(crate) same_screen: CInt,
    }

    /// `XIMCallback`, for the preedit callbacks of an input context.
    pub(crate) struct XimCallback = "XIMCallback" {
        pub(crate) client_data: *mut CChar,
        pub(crate) callback: XimProc,
    }

    /// `XIMCallback`, for the destroy callback of an input method.
    pub(crate) struct XimDestroyCallback = "XIMCallback" {
        pub(crate) client_data: *mut CChar,
        pub(crate) callback: XimDestroyProc,
    }

    /// `XICCallback`.
    pub(crate) struct XicCallback = "XICCallback" {
        pub(crate) client_data: *mut CChar,
        pub(crate) callback: XicProc,
    }

    /// `XIMText`. `string` is the union of `multi_byte` and `wide_char`.
    pub(crate) struct XimText = "XIMText" {
        pub(crate) length: u16,
        pub(crate) feedback: *mut CUlong,
        pub(crate) encoding_is_wchar: CInt,
        pub(crate) string: *mut CChar,
    }

    /// `XIMPreeditDrawCallbackStruct`.
    pub(crate) struct XimPreeditDraw = "XIMPreeditDrawCallbackStruct" {
        pub(crate) caret: CInt,
        pub(crate) chg_first: CInt,
        pub(crate) chg_length: CInt,
        pub(crate) text: *mut XimText,
    }

    /// `XIMStyles`.
    pub(crate) struct XimStyles = "XIMStyles" {
        pub(crate) count_styles: u16,
        pub(crate) supported_styles: *mut CUlong,
    }

    /// `XPoint`.
    pub(crate) struct XPoint = "XPoint" {
        pub(crate) x: i16,
        pub(crate) y: i16,
    }
}

ffi_union! {
    /// `XEvent`.
    pub(crate) union XEvent = "XEvent" {
        pub(crate) r#type: CInt,
        pub(crate) xkey: XKeyEvent,
        pub(crate) pad: [CLong; 24],
    }
}

libraries! {
    /// libX11: the input method protocol of XIM.
    pub(crate) struct Xlib {
        library: "libX11",
        sonames: ["libX11.so.6"],
        headers: ["X11/Xlib.h", "X11/Xutil.h"],
        pkg_config: "x11",
        package: "libx11-dev",
        functions {
            fn XInitThreads() -> CInt;
            fn XOpenDisplay(name: *const CChar) -> *mut XDisplay;
            fn XCloseDisplay(display: *mut XDisplay) -> CInt;
            fn XConnectionNumber(display: *mut XDisplay) -> CInt;
            fn XDefaultScreen(display: *mut XDisplay) -> CInt;
            fn XPending(display: *mut XDisplay) -> CInt;
            fn XNextEvent(display: *mut XDisplay, event: *mut XEvent) -> CInt;
            fn XFlush(display: *mut XDisplay) -> CInt;
            fn XFilterEvent(event: *mut XEvent, window: CUlong) -> CInt;
            fn XFree(data: *mut c_void) -> CInt;
            fn XSupportsLocale() -> CInt;
            fn XSetLocaleModifiers(modifiers: *const CChar) -> *mut CChar;
            fn XOpenIM(
                display: *mut XDisplay,
                database: *mut XrmHashBucketRec,
                name: *mut CChar,
                class: *mut CChar,
            ) -> *mut XimRec;
            fn XCloseIM(im: *mut XimRec) -> CInt;
            fn XRegisterIMInstantiateCallback(
                display: *mut XDisplay,
                database: *mut XrmHashBucketRec,
                name: *mut CChar,
                class: *mut CChar,
                callback: XidProc,
                client_data: *mut CChar,
            ) -> CInt;
            fn XUnregisterIMInstantiateCallback(
                display: *mut XDisplay,
                database: *mut XrmHashBucketRec,
                name: *mut CChar,
                class: *mut CChar,
                callback: XidProc,
                client_data: *mut CChar,
            ) -> CInt;
            fn XDestroyIC(ic: *mut XicRec);
            fn XSetICFocus(ic: *mut XicRec);
            fn XUnsetICFocus(ic: *mut XicRec);
            fn Xutf8LookupString(
                ic: *mut XicRec,
                event: *mut XKeyEvent,
                buffer: *mut CChar,
                bytes: CInt,
                keysym: *mut CUlong,
                status: *mut CInt,
            ) -> CInt;
        }
        variadic {
            fn XGetIMValues(im: *mut XimRec) -> *mut CChar;
            fn XSetIMValues(im: *mut XimRec) -> *mut CChar;
            fn XCreateIC(im: *mut XimRec) -> *mut XicRec;
            fn XSetICValues(ic: *mut XicRec) -> *mut CChar;
            fn XVaCreateNestedList(unused: CInt) -> *mut c_void;
        }
    }
}

c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    const KEY_PRESS: CInt = 2 => "KeyPress";
    const KEY_RELEASE: CInt = 3 => "KeyRelease";
    const PROPERTY_NOTIFY: CInt = 28 => "PropertyNotify";
    const XIM_PREEDIT_CALLBACKS: CUlong = 0x0002 => "XIMPreeditCallbacks";
    const XIM_STATUS_NOTHING: CUlong = 0x0400 => "XIMStatusNothing";
    const XIM_STATUS_NONE: CUlong = 0x0800 => "XIMStatusNone";
    const X_LOOKUP_CHARS: CInt = 2 => "XLookupChars";
    const X_LOOKUP_BOTH: CInt = 4 => "XLookupBoth";
    const X_BUFFER_OVERFLOW: CInt = -1 => "XBufferOverflow";
}

// The names of the XIM values. Xlib declares them as string macros, which
// the binding check cannot compare.
const XN_QUERY_INPUT_STYLE: &std::ffi::CStr = c"queryInputStyle";
const XN_DESTROY_CALLBACK: &std::ffi::CStr = c"destroyCallback";
const XN_INPUT_STYLE: &std::ffi::CStr = c"inputStyle";
const XN_CLIENT_WINDOW: &std::ffi::CStr = c"clientWindow";
const XN_FOCUS_WINDOW: &std::ffi::CStr = c"focusWindow";
const XN_PREEDIT_ATTRIBUTES: &std::ffi::CStr = c"preeditAttributes";
const XN_PREEDIT_START_CALLBACK: &std::ffi::CStr = c"preeditStartCallback";
const XN_PREEDIT_DONE_CALLBACK: &std::ffi::CStr = c"preeditDoneCallback";
const XN_PREEDIT_DRAW_CALLBACK: &std::ffi::CStr = c"preeditDrawCallback";
const XN_PREEDIT_CARET_CALLBACK: &std::ffi::CStr = c"preeditCaretCallback";
const XN_SPOT_LOCATION: &std::ffi::CStr = c"spotLocation";

/// The pointer of a name of an XIM value.
fn name(name: &std::ffi::CStr) -> *const CChar {
    name.as_ptr().cast()
}

/// The end of a variadic list of XIM values.
fn end() -> *const CChar {
    std::ptr::null()
}

/// A change of the preedit text of the input context of a window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PreeditChange {
    Start(u32),
    /// Replaces `length` characters at `first` with `text`, and puts the
    /// caret at character `caret`.
    Draw {
        window: u32,
        caret: usize,
        first: usize,
        length: usize,
        text: String,
    },
    Done(u32),
}

thread_local! {
    /// The changes that the callbacks queued, for `take_preedit_changes`.
    static CHANGES: RefCell<Vec<PreeditChange>> = const { RefCell::new(Vec::new()) };
}

fn queue(change: PreeditChange) {
    CHANGES.with(|changes| changes.borrow_mut().push(change));
}

/// The window of the client data of a callback.
fn window_of(client_data: *mut CChar) -> u32 {
    u32::try_from(client_data.addr()).unwrap_or(0)
}

/// The text of an `XIMText`, in the UTF-8 locale of the input method.
///
/// # Safety
///
/// `text` is null or points at a valid `XIMText`.
unsafe fn text_of(text: *const XimText) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a valid `XIMText`.
    let text = unsafe { *text };
    if text.string.is_null() {
        return String::new();
    }
    if text.encoding_is_wchar == 0 {
        // SAFETY: a multi-byte string is NUL-terminated.
        let bytes = unsafe { std::ffi::CStr::from_ptr(text.string.cast()) };
        return bytes.to_string_lossy().into_owned();
    }
    let wide = text.string.cast::<i32>();
    // SAFETY: a wide string holds `length` characters.
    let units = unsafe { std::slice::from_raw_parts(wide, usize::from(text.length)) };
    units
        .iter()
        .filter_map(|unit| u32::try_from(*unit).ok().and_then(char::from_u32))
        .collect()
}

callback! {
    /// The preedit start callback: the input method starts marked text.
    ///
    /// # Safety
    ///
    /// Xlib passes the client data of the callback, which holds the window.
    fn preedit_start(_ic: *mut XicRec, client_data: *mut CChar, _data: *mut CChar) -> CInt
        [boundary "XIM preedit start", fallback -1]
    {
        queue(PreeditChange::Start(window_of(client_data)));
        // No limit on the length of the marked text.
        -1
    }

    /// The preedit draw callback: a change of the marked text.
    ///
    /// # Safety
    ///
    /// Xlib passes the client data of the callback and a valid
    /// `XIMPreeditDrawCallbackStruct`.
    fn preedit_draw(_ic: *mut XicRec, client_data: *mut CChar, data: *mut CChar)
        [boundary "XIM preedit draw", fallback ()]
    {
        if data.is_null() {
            return;
        }
        // SAFETY: Xlib passes a valid draw structure.
        let draw = unsafe { *data.cast::<XimPreeditDraw>() };
        // SAFETY: the text is null or valid.
        let text = unsafe { text_of(draw.text) };
        let count = |value: CInt| usize::try_from(value).unwrap_or(0);
        queue(PreeditChange::Draw {
            window: window_of(client_data),
            caret: count(draw.caret),
            first: count(draw.chg_first),
            length: count(draw.chg_length),
            text,
        });
    }

    /// The preedit done callback: the marked text ends.
    ///
    /// # Safety
    ///
    /// Xlib passes the client data of the callback.
    fn preedit_done(_ic: *mut XicRec, client_data: *mut CChar, _data: *mut CChar)
        [boundary "XIM preedit done", fallback ()]
    {
        queue(PreeditChange::Done(window_of(client_data)));
    }

    /// The preedit caret callback. The client draws no caret of its own
    /// in the marked text, so it ignores the moves.
    ///
    /// # Safety
    ///
    /// Xlib passes valid arguments; the callback reads none of them.
    fn preedit_caret(_ic: *mut XicRec, _client_data: *mut CChar, _data: *mut CChar)
        [boundary "XIM preedit caret", fallback ()]
    {
    }

    /// The instantiate callback: an input method of `XMODIFIERS` can open
    /// on the display of the registration. Xlib keeps one list of these
    /// callbacks for the process, so it can call one from the thread of
    /// another display.
    ///
    /// # Safety
    ///
    /// The client data is the `AtomicBool` of an `XimDisplay`, which lives
    /// until its registration ends.
    fn instantiated(_display: *mut XDisplay, client_data: *mut CChar, _data: *mut CChar)
        [boundary "XIM instantiate", fallback ()]
    {
        if client_data.is_null() {
            return;
        }
        // SAFETY: the client data points at a live `AtomicBool`, which any
        // thread can set.
        let flag = unsafe { &*client_data.cast_const().cast::<AtomicBool>() };
        flag.store(true, Ordering::Release);
    }

    /// The destroy callback: the input method stopped. Xlib destroyed it
    /// and its input contexts.
    ///
    /// # Safety
    ///
    /// The client data is the `alive` cell of an `InputMethod`, which
    /// lives until after the method closes. Xlib calls the callback while
    /// it handles the connection of the method, which only the thread of
    /// the method uses.
    fn destroyed(_im: *mut XimRec, client_data: *mut CChar, _data: *mut CChar)
        [boundary "XIM destroy", fallback ()]
    {
        if client_data.is_null() {
            return;
        }
        // SAFETY: the client data points at a live cell of this thread.
        let alive = unsafe { &*client_data.cast_const().cast::<Cell<bool>>() };
        alive.set(false);
    }
}

/// An Xlib connection of its own to the server of `DISPLAY`, for the
/// EGL display when EGL has no XCB platform.
pub(crate) struct XlibDisplay {
    lib: &'static Xlib,
    raw: *mut XDisplay,
}

impl XlibDisplay {
    /// Opens the connection. Returns None without libX11 or when the
    /// server refuses.
    pub(crate) fn open() -> Option<Self> {
        let lib = Xlib::get().ok()?;
        // SAFETY: null opens the display of `DISPLAY`.
        let raw = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
        (!raw.is_null()).then_some(Self { lib, raw })
    }

    pub(in crate::ffi) fn raw(&self) -> *mut XDisplay {
        self.raw
    }

    /// The number of the default screen.
    pub(in crate::ffi) fn default_screen(&self) -> CInt {
        // SAFETY: the display is open.
        unsafe { (self.lib.XDefaultScreen)(self.raw) }
    }
}

impl Drop for XlibDisplay {
    fn drop(&mut self) {
        // SAFETY: this is the only close of the display.
        unsafe { (self.lib.XCloseDisplay)(self.raw) };
    }
}

/// Takes the preedit changes that the callbacks queued.
pub(crate) fn take_preedit_changes() -> Vec<PreeditChange> {
    CHANGES.with(|changes| std::mem::take(&mut *changes.borrow_mut()))
}

/// A key event for `XFilterEvent`, from an xcb key event.
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyInput {
    pub(crate) pressed: bool,
    pub(crate) window: u32,
    pub(crate) root: u32,
    pub(crate) time: u32,
    pub(crate) position: (i16, i16),
    pub(crate) root_position: (i16, i16),
    pub(crate) state: u16,
    pub(crate) keycode: u8,
}

/// An event that the input method sent on the Xlib connection, in the
/// order of arrival.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImEvent {
    /// A key that the input method did not use.
    Key {
        window: u32,
        keycode: u8,
        pressed: bool,
    },
    /// Committed text.
    Commit { window: u32, text: String },
    /// A change of the marked text, from a preedit callback.
    Preedit(PreeditChange),
}

/// The library after `XInitThreads`, or `None` when it does not load.
/// Each thread of a process can open its own input method, so Xlib must
/// lock its global state. `XInitThreads` must come before any other Xlib
/// call.
fn library() -> Option<&'static Xlib> {
    static READY: OnceLock<bool> = OnceLock::new();
    let lib = Xlib::get().ok()?;
    // SAFETY: the first call of the process, before any other Xlib call
    // of this crate.
    let ready = *READY.get_or_init(|| unsafe { (lib.XInitThreads)() } != 0);
    ready.then_some(lib)
}

/// Calls `f` with the UTF-8 locale and the modifiers of `XMODIFIERS`.
/// Returns None when Xlib does not support the locale. The locale lock
/// also serializes the list of instantiate callbacks, which Xlib keeps
/// for the process without a lock.
fn with_xim_locale<T>(lib: &Xlib, f: impl FnOnce() -> T) -> Option<T> {
    super::libc::with_utf8_ctype(|| {
        // SAFETY: the call reads the current locale.
        if unsafe { (lib.XSupportsLocale)() } == 0 {
            return None;
        }
        // SAFETY: the empty string reads `XMODIFIERS`.
        unsafe { (lib.XSetLocaleModifiers)(name(c"")) };
        Some(f())
    })
    .flatten()
}

/// The Xlib connection of the input method. While it has no input method,
/// it watches for one to start.
pub(crate) struct XimDisplay {
    lib: &'static Xlib,
    raw: *mut XDisplay,
    /// Set by the instantiate callback.
    instantiable: Box<AtomicBool>,
    watching: Cell<bool>,
}

impl XimDisplay {
    /// Opens a connection to the server of `DISPLAY`. Returns None
    /// without libX11 or when the server refuses.
    pub(crate) fn open() -> Option<Rc<Self>> {
        let lib = library()?;
        // SAFETY: null opens the display of `DISPLAY`.
        let raw = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
        (!raw.is_null()).then(|| {
            Rc::new(Self {
                lib,
                raw,
                instantiable: Box::new(AtomicBool::new(false)),
                watching: Cell::new(false),
            })
        })
    }

    /// The client data of the instantiate callback.
    fn flag(&self) -> *mut CChar {
        (&raw const *self.instantiable).cast_mut().cast()
    }

    /// Registers the instantiate callback. When an input method of
    /// `XMODIFIERS` is already there, Xlib calls it during the
    /// registration. Returns false when the registration failed, as
    /// without a UTF-8 locale.
    pub(crate) fn watch(&self) -> bool {
        if self.watching.get() {
            return true;
        }
        let registered = with_xim_locale(self.lib, || {
            // SAFETY: the display is open; null takes the defaults. The
            // flag lives until `unwatch` or the drop of `self`.
            unsafe {
                (self.lib.XRegisterIMInstantiateCallback)(
                    self.raw,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    Some(instantiated),
                    self.flag(),
                )
            }
        });
        self.watching
            .set(registered.is_some_and(|registered| registered != 0));
        self.flush();
        self.watching.get()
    }

    /// Ends the registration of the instantiate callback.
    pub(crate) fn unwatch(&self) {
        if !self.watching.replace(false) {
            return;
        }
        with_xim_locale(self.lib, || {
            // SAFETY: the arguments match those of the registration.
            unsafe {
                (self.lib.XUnregisterIMInstantiateCallback)(
                    self.raw,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    Some(instantiated),
                    self.flag(),
                )
            }
        });
        self.flush();
    }

    /// True once after the instantiate callback ran.
    pub(crate) fn take_instantiable(&self) -> bool {
        self.instantiable.swap(false, Ordering::Acquire)
    }

    /// The descriptor of the Xlib connection, for the event loop.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the display is open.
        let fd = unsafe { (self.lib.XConnectionNumber)(self.raw) };
        // SAFETY: the display owns the descriptor while `self` lives.
        unsafe { BorrowedFd::borrow_raw(fd) }
    }

    fn flush(&self) {
        // SAFETY: the display is open.
        unsafe { (self.lib.XFlush)(self.raw) };
    }

    /// Gives a key event to the input method. Returns true when the input
    /// method took it.
    pub(crate) fn filter_key(&self, key: KeyInput) -> bool {
        let mut event = XEvent {
            xkey: XKeyEvent {
                r#type: if key.pressed { KEY_PRESS } else { KEY_RELEASE },
                serial: 0,
                send_event: 0,
                display: self.raw,
                window: CUlong::from(key.window),
                root: CUlong::from(key.root),
                subwindow: 0,
                time: CUlong::from(key.time),
                x: CInt::from(key.position.0),
                y: CInt::from(key.position.1),
                x_root: CInt::from(key.root_position.0),
                y_root: CInt::from(key.root_position.1),
                state: u32::from(key.state),
                keycode: u32::from(key.keycode),
                same_screen: 1,
            },
        };
        // SAFETY: the event is a valid key event of this display.
        let taken = unsafe { (self.lib.XFilterEvent)(&raw mut event, 0) } != 0;
        self.flush();
        taken
    }

    /// Gives an event of the connection to the filters of Xlib. Returns
    /// true when a filter took it.
    fn filter(&self, event: &mut XEvent) -> bool {
        // SAFETY: every event starts with its type.
        let property = unsafe { event.r#type } == PROPERTY_NOTIFY;
        let lib = self.lib;
        let mut filter = || {
            // SAFETY: the event came from Xlib.
            unsafe { (lib.XFilterEvent)(event, 0) != 0 }
        };
        if property {
            // The filter of the instantiate callbacks reads their list.
            return with_xim_locale(lib, filter).unwrap_or(false);
        }
        filter()
    }

    /// Reads the events of the Xlib connection, in order. Each goes
    /// through the filters of Xlib first, where the preedit callbacks and
    /// the destroy callback run. `contexts` gives the open context of a
    /// window, for the committed text.
    pub(crate) fn events(&self, contexts: &dyn Fn(u32) -> Option<*mut XicRec>) -> Vec<ImEvent> {
        let mut events: Vec<ImEvent> = take_preedit_changes()
            .into_iter()
            .map(ImEvent::Preedit)
            .collect();
        // SAFETY: the display is open; the call does not block.
        while unsafe { (self.lib.XPending)(self.raw) } > 0 {
            let mut event = XEvent { pad: [0; 24] };
            // SAFETY: an event is pending, so the call does not block.
            unsafe { (self.lib.XNextEvent)(self.raw, &raw mut event) };
            let filtered = self.filter(&mut event);
            events.extend(take_preedit_changes().into_iter().map(ImEvent::Preedit));
            if filtered {
                continue;
            }
            // SAFETY: every event starts with its type.
            let kind = unsafe { event.r#type };
            if kind != KEY_PRESS && kind != KEY_RELEASE {
                continue;
            }
            // SAFETY: the type says that the event is a key event.
            let mut key = unsafe { event.xkey };
            let window = u32::try_from(key.window).unwrap_or(0);
            if key.keycode != 0 {
                events.push(ImEvent::Key {
                    window,
                    keycode: u8::try_from(key.keycode).unwrap_or(0),
                    pressed: kind == KEY_PRESS,
                });
            } else if kind == KEY_PRESS
                && let Some(ic) = contexts(window)
                && let Some(text) = self.lookup(ic, &mut key)
            {
                events.push(ImEvent::Commit { window, text });
            }
        }
        events
    }

    /// The committed text of a key event without a key code.
    fn lookup(&self, ic: *mut XicRec, key: &mut XKeyEvent) -> Option<String> {
        let mut buffer = vec![CChar(0); 256];
        loop {
            let mut status: CInt = 0;
            let length = CInt::try_from(buffer.len()).unwrap_or(CInt::MAX);
            // SAFETY: the buffer holds `length` bytes; the key event came
            // from the input method of `ic`, which is alive.
            let count = unsafe {
                (self.lib.Xutf8LookupString)(
                    ic,
                    key,
                    buffer.as_mut_ptr(),
                    length,
                    std::ptr::null_mut(),
                    &raw mut status,
                )
            };
            let count = usize::try_from(count).unwrap_or(0);
            if status == X_BUFFER_OVERFLOW {
                buffer.resize(count + 1, CChar(0));
                continue;
            }
            if status != X_LOOKUP_CHARS && status != X_LOOKUP_BOTH {
                return None;
            }
            let bytes: Vec<u8> = buffer[..count]
                .iter()
                .map(|c| u8::from_ne_bytes(c.0.to_ne_bytes()))
                .collect();
            return String::from_utf8(bytes).ok();
        }
    }
}

impl Drop for XimDisplay {
    fn drop(&mut self) {
        self.unwatch();
        // SAFETY: this is the only close of the display, after every
        // input method of it closed, because each holds the display.
        unsafe { (self.lib.XCloseDisplay)(self.raw) };
    }
}

/// The result of an attempt to open an input method.
pub(crate) enum Opened {
    /// An input method with preedit callbacks.
    Method(Rc<InputMethod>),
    /// An input method opened, but it takes no preedit callbacks or no
    /// destroy callback, so it closed again.
    Unusable,
    /// No input method opened.
    Missing,
}

/// The destroy callback of an input method and its state.
struct DestroyWatch {
    /// False after the destroy callback.
    alive: Cell<bool>,
    callback: XimDestroyCallback,
}

/// An open input method with preedit callbacks.
pub(crate) struct InputMethod {
    display: Rc<XimDisplay>,
    im: *mut XimRec,
    style: CUlong,
    watch: Box<DestroyWatch>,
}

impl InputMethod {
    /// Opens the input method of `XMODIFIERS` on `display`.
    pub(crate) fn open(display: &Rc<XimDisplay>) -> Opened {
        let lib = display.lib;
        let im = with_xim_locale(lib, || {
            // SAFETY: the display is open; null takes the defaults.
            unsafe {
                (lib.XOpenIM)(
                    display.raw,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            }
        })
        .unwrap_or(std::ptr::null_mut());
        if im.is_null() {
            return Opened::Missing;
        }
        let mut method = Self {
            display: Rc::clone(display),
            im,
            style: 0,
            watch: Box::new(DestroyWatch {
                alive: Cell::new(true),
                callback: XimDestroyCallback {
                    client_data: std::ptr::null_mut(),
                    callback: Some(destroyed),
                },
            }),
        };
        method.watch.callback.client_data = (&raw const method.watch.alive).cast_mut().cast();
        let Some(style) = method.callback_style() else {
            return Opened::Unusable;
        };
        method.style = style;
        // SAFETY: the list names one value and ends with null. The
        // callback structure and its cell live in the box until after the
        // method closes.
        let failed = unsafe {
            (lib.XSetIMValues)(
                method.im,
                name(XN_DESTROY_CALLBACK),
                &raw const method.watch.callback,
                end(),
            )
        };
        if !failed.is_null() {
            return Opened::Unusable;
        }
        display.flush();
        Opened::Method(Rc::new(method))
    }

    /// The input style with preedit callbacks and no status, if the input
    /// method has one.
    fn callback_style(&self) -> Option<CUlong> {
        let mut styles: *mut XimStyles = std::ptr::null_mut();
        // SAFETY: the list names one value and ends with null; the result
        // goes to `styles`.
        let failed = unsafe {
            (self.display.lib.XGetIMValues)(
                self.im,
                name(XN_QUERY_INPUT_STYLE),
                &raw mut styles,
                end(),
            )
        };
        if !failed.is_null() || styles.is_null() {
            return None;
        }
        // SAFETY: Xlib returned a valid list.
        let list = unsafe { *styles };
        // SAFETY: the list holds `count_styles` styles.
        let supported = unsafe {
            std::slice::from_raw_parts(list.supported_styles, usize::from(list.count_styles))
        };
        let wanted = [
            XIM_PREEDIT_CALLBACKS | XIM_STATUS_NOTHING,
            XIM_PREEDIT_CALLBACKS | XIM_STATUS_NONE,
        ];
        let found = supported
            .iter()
            .copied()
            .find(|style| wanted.contains(style));
        // SAFETY: Xlib allocated the list for the caller.
        unsafe { (self.display.lib.XFree)(styles.cast()) };
        found
    }

    /// False after the input method stopped. Xlib then destroyed the
    /// method and its contexts.
    pub(crate) fn is_alive(&self) -> bool {
        self.watch.alive.get()
    }

    /// Creates an input context for the xcb window `window`, with the
    /// preedit callbacks.
    pub(crate) fn create_context(self: &Rc<Self>, window: u32) -> Option<InputContext> {
        if !self.is_alive() {
            return None;
        }
        let lib = self.display.lib;
        let data = std::ptr::without_provenance_mut::<CChar>(window as usize);
        let callbacks = Box::new(Callbacks {
            start: XicCallback {
                client_data: data,
                callback: Some(preedit_start),
            },
            draw: XimCallback {
                client_data: data,
                callback: Some(preedit_draw),
            },
            done: XimCallback {
                client_data: data,
                callback: Some(preedit_done),
            },
            caret: XimCallback {
                client_data: data,
                callback: Some(preedit_caret),
            },
        });
        // SAFETY: the list pairs names with pointers to callbacks that
        // live in the box, and ends with null.
        let attributes = unsafe {
            (lib.XVaCreateNestedList)(
                0,
                name(XN_PREEDIT_START_CALLBACK),
                &raw const callbacks.start,
                name(XN_PREEDIT_DONE_CALLBACK),
                &raw const callbacks.done,
                name(XN_PREEDIT_DRAW_CALLBACK),
                &raw const callbacks.draw,
                name(XN_PREEDIT_CARET_CALLBACK),
                &raw const callbacks.caret,
                end(),
            )
        };
        if attributes.is_null() {
            return None;
        }
        let xid = CUlong::from(window);
        // SAFETY: the list pairs names with values of their types, and
        // ends with null.
        let ic = unsafe {
            (lib.XCreateIC)(
                self.im,
                name(XN_INPUT_STYLE),
                self.style,
                name(XN_CLIENT_WINDOW),
                xid,
                name(XN_FOCUS_WINDOW),
                xid,
                name(XN_PREEDIT_ATTRIBUTES),
                attributes,
                end(),
            )
        };
        // SAFETY: the nested list came from `XVaCreateNestedList`.
        unsafe { (lib.XFree)(attributes) };
        self.display.flush();
        (!ic.is_null()).then(|| InputContext {
            method: Rc::clone(self),
            ic,
            _callbacks: callbacks,
        })
    }
}

impl Drop for InputMethod {
    fn drop(&mut self) {
        // After the destroy callback, Xlib already closed the method.
        if self.is_alive() {
            // SAFETY: the input method is open, and every context closed
            // before, because each holds the method.
            unsafe { (self.display.lib.XCloseIM)(self.im) };
            self.display.flush();
        }
    }
}

/// The callbacks of an input context, which live as long as it.
struct Callbacks {
    start: XicCallback,
    draw: XimCallback,
    done: XimCallback,
    caret: XimCallback,
}

/// An input context for one window.
pub(crate) struct InputContext {
    method: Rc<InputMethod>,
    ic: *mut XicRec,
    _callbacks: Box<Callbacks>,
}

impl InputContext {
    /// The context, while its input method is alive.
    pub(crate) fn raw(&self) -> Option<*mut XicRec> {
        self.method.is_alive().then_some(self.ic)
    }

    /// Gives the context the focus, or takes it away.
    pub(crate) fn set_focus(&self, focused: bool) {
        let Some(ic) = self.raw() else {
            return;
        };
        let lib = self.method.display.lib;
        if focused {
            // SAFETY: the context is open.
            unsafe { (lib.XSetICFocus)(ic) };
        } else {
            // SAFETY: the context is open.
            unsafe { (lib.XUnsetICFocus)(ic) };
        }
        self.method.display.flush();
    }

    /// Puts the candidate window of the input method near the caret, at
    /// `x`, `y` pixels of the window.
    pub(crate) fn set_spot(&self, (x, y): (i16, i16)) {
        let Some(ic) = self.raw() else {
            return;
        };
        let lib = self.method.display.lib;
        let point = XPoint { x, y };
        // SAFETY: the list names one value and ends with null; Xlib copies
        // the point when the list is set.
        let attributes = unsafe {
            (lib.XVaCreateNestedList)(0, name(XN_SPOT_LOCATION), &raw const point, end())
        };
        if attributes.is_null() {
            return;
        }
        // SAFETY: the list names one value and ends with null.
        unsafe { (lib.XSetICValues)(ic, name(XN_PREEDIT_ATTRIBUTES), attributes, end()) };
        // SAFETY: the nested list came from `XVaCreateNestedList`.
        unsafe { (lib.XFree)(attributes) };
        self.method.display.flush();
    }
}

impl Drop for InputContext {
    fn drop(&mut self) {
        // After the destroy callback, Xlib already destroyed the context.
        if let Some(ic) = self.raw() {
            // SAFETY: this is the only destruction of the context.
            unsafe { (self.method.display.lib.XDestroyIC)(ic) };
            self.method.display.flush();
        }
    }
}

/// The layouts of the structs and unions, for the binding check.
#[cfg(test)]
pub(crate) fn layouts() -> Vec<super::checks::StructItem> {
    vec![
        XKeyEvent::layout(),
        XimCallback::layout(),
        XimDestroyCallback::layout(),
        XicCallback::layout(),
        XimText::layout(),
        XimPreeditDraw::layout(),
        XimStyles::layout(),
        XPoint::layout(),
        XEvent::layout(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::ImeEvent;
    use crate::native::x11::ime::Composition;

    /// Calls the draw callback of `window` with a multi-byte text.
    fn draw(window: *mut CChar, caret: CInt, (first, length): (CInt, CInt), text: &std::ffi::CStr) {
        let mut ximtext = XimText {
            length: u16::try_from(text.to_str().unwrap().chars().count()).unwrap(),
            feedback: std::ptr::null_mut(),
            encoding_is_wchar: 0,
            string: text.as_ptr().cast_mut().cast(),
        };
        let mut draw = XimPreeditDraw {
            caret,
            chg_first: first,
            chg_length: length,
            text: &raw mut ximtext,
        };
        // SAFETY: the arguments are valid, as Xlib passes them.
        unsafe { preedit_draw(std::ptr::null_mut(), window, (&raw mut draw).cast()) };
    }

    #[test]
    fn callbacks_queue_the_preedit_changes_of_their_window() {
        let window = std::ptr::without_provenance_mut::<CChar>(0x2a);
        // SAFETY: the arguments are valid, as Xlib passes them.
        unsafe { preedit_start(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        draw(window, 3, (0, 0), c"xin chào");
        // SAFETY: as above.
        unsafe { preedit_done(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        assert_eq!(
            take_preedit_changes(),
            vec![
                PreeditChange::Start(0x2a),
                PreeditChange::Draw {
                    window: 0x2a,
                    caret: 3,
                    first: 0,
                    length: 0,
                    text: "xin chào".to_owned(),
                },
                PreeditChange::Done(0x2a),
            ]
        );
        assert!(take_preedit_changes().is_empty());
    }

    /// The acceptance test of XIM composition: the preedit callbacks and a
    /// commit give the `ImeEvent` sequence of the other backends, with
    /// UTF-16 ranges.
    #[test]
    fn preedit_callbacks_and_a_commit_give_the_ime_events() {
        let window = std::ptr::without_provenance_mut::<CChar>(7);
        // SAFETY: the arguments are valid, as Xlib passes them.
        unsafe { preedit_start(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        draw(window, 2, (0, 0), c"ni");
        // "ni" becomes "你" and an emoji, which takes two UTF-16 units.
        draw(window, 2, (0, 2), c"你😀");
        let mut events: Vec<ImEvent> = take_preedit_changes()
            .into_iter()
            .map(ImEvent::Preedit)
            .collect();
        events.push(ImEvent::Commit {
            window: 7,
            text: "你😀".to_owned(),
        });
        // The input method then clears the marked text and ends it.
        draw(window, 0, (0, 2), c"");
        // SAFETY: as above.
        unsafe { preedit_done(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        events.extend(take_preedit_changes().into_iter().map(ImEvent::Preedit));
        // A second composition ends without a commit.
        // SAFETY: as above.
        unsafe { preedit_start(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        draw(window, 1, (0, 0), c"a");
        // SAFETY: as above.
        unsafe { preedit_done(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        events.extend(take_preedit_changes().into_iter().map(ImEvent::Preedit));

        let mut composition = Composition::default();
        let converted: Vec<(u32, ImeEvent)> = events
            .into_iter()
            .filter_map(|event| composition.convert(event))
            .collect();
        let ime = |event| (7, event);
        assert_eq!(
            converted,
            vec![
                ime(ImeEvent::SetMarkedText {
                    text: "ni".into(),
                    selected_utf16: 2..2,
                    replacement_utf16: None,
                }),
                ime(ImeEvent::SetMarkedText {
                    text: "你😀".into(),
                    selected_utf16: 3..3,
                    replacement_utf16: None,
                }),
                ime(ImeEvent::CommitText {
                    text: "你😀".into(),
                    replacement_utf16: None,
                }),
                ime(ImeEvent::SetMarkedText {
                    text: "a".into(),
                    selected_utf16: 1..1,
                    replacement_utf16: None,
                }),
                ime(ImeEvent::Unmark),
            ]
        );
    }
}
