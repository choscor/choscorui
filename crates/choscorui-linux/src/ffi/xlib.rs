//! The declarations of libX11 for XIM, and a safe input method on its
//! own Xlib connection.
//!
//! The X11 backend reads its events through xcb. XIM needs Xlib: the
//! input method protocol and its text conversions live there. So the
//! input method opens a second connection with `XOpenDisplay`, and the
//! input contexts name the xcb windows. The client gives each key event
//! of an active window to `XFilterEvent`. The input method sends back the
//! keys that it does not use and the committed text on the Xlib
//! connection, which the event loop drains. The preedit callbacks queue
//! their changes on the thread, for the client to read.

use super::ctypes::{CChar, CInt, CLong, CUlong, c_void};
use super::macros::{c_consts, callback, ffi_struct, ffi_union, libraries, opaque_types};
use std::cell::RefCell;
use std::os::fd::BorrowedFd;
use std::rc::Rc;
use std::sync::OnceLock;

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

/// `XICProc`.
pub(crate) type XicProc = Option<unsafe extern "C" fn(*mut XicRec, *mut CChar, *mut CChar) -> CInt>;

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

    /// `XIMCallback`.
    pub(crate) struct XimCallback = "XIMCallback" {
        pub(crate) client_data: *mut CChar,
        pub(crate) callback: XimProc,
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

/// An event that the input method sent back on the Xlib connection.
pub(crate) enum ImEvent {
    /// A key that the input method did not use.
    Key {
        window: u32,
        keycode: u8,
        pressed: bool,
    },
    /// Committed text.
    Commit { window: u32, text: String },
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

/// An open input method with preedit callbacks.
pub(crate) struct InputMethod {
    lib: &'static Xlib,
    display: *mut XDisplay,
    im: *mut XimRec,
    style: CUlong,
}

impl InputMethod {
    /// Opens the input method of `XMODIFIERS` on a new connection to the
    /// server of `DISPLAY`. Returns None without libX11, without a UTF-8
    /// locale, or without an input method that takes preedit callbacks.
    pub(crate) fn open() -> Option<Rc<Self>> {
        let lib = library()?;
        // SAFETY: null opens the display of `DISPLAY`.
        let display = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return None;
        }
        let im = super::libc::with_utf8_ctype(|| {
            // SAFETY: the call reads the current locale.
            if unsafe { (lib.XSupportsLocale)() } == 0 {
                return std::ptr::null_mut();
            }
            // SAFETY: the empty string reads `XMODIFIERS`.
            unsafe { (lib.XSetLocaleModifiers)(name(c"")) };
            // SAFETY: the display is open; null takes the defaults.
            unsafe {
                (lib.XOpenIM)(
                    display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            }
        })
        .unwrap_or(std::ptr::null_mut());
        if im.is_null() {
            // SAFETY: this is the only close of the display.
            unsafe { (lib.XCloseDisplay)(display) };
            return None;
        }
        let mut method = Self {
            lib,
            display,
            im,
            style: 0,
        };
        method.style = method.callback_style()?;
        Some(Rc::new(method))
    }

    /// The input style with preedit callbacks and no status, if the input
    /// method has one.
    fn callback_style(&self) -> Option<CUlong> {
        let mut styles: *mut XimStyles = std::ptr::null_mut();
        // SAFETY: the list names one value and ends with null; the result
        // goes to `styles`.
        let failed = unsafe {
            (self.lib.XGetIMValues)(self.im, name(XN_QUERY_INPUT_STYLE), &raw mut styles, end())
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
        unsafe { (self.lib.XFree)(styles.cast()) };
        found
    }

    /// The descriptor of the Xlib connection, for the event loop.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the display is open.
        let fd = unsafe { (self.lib.XConnectionNumber)(self.display) };
        // SAFETY: the display owns the descriptor while `self` lives.
        unsafe { BorrowedFd::borrow_raw(fd) }
    }

    fn flush(&self) {
        // SAFETY: the display is open.
        unsafe { (self.lib.XFlush)(self.display) };
    }

    /// Creates an input context for the xcb window `window`, with the
    /// preedit callbacks.
    pub(crate) fn create_context(self: &Rc<Self>, window: u32) -> Option<InputContext> {
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
            (self.lib.XVaCreateNestedList)(
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
            (self.lib.XCreateIC)(
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
        unsafe { (self.lib.XFree)(attributes) };
        self.flush();
        (!ic.is_null()).then(|| InputContext {
            method: Rc::clone(self),
            ic,
            _callbacks: callbacks,
        })
    }

    /// Gives a key event to the input method. Returns true when the input
    /// method took it.
    pub(crate) fn filter_key(&self, key: KeyInput) -> bool {
        let mut event = XEvent {
            xkey: XKeyEvent {
                r#type: if key.pressed { KEY_PRESS } else { KEY_RELEASE },
                serial: 0,
                send_event: 0,
                display: self.display,
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

    /// Reads the events of the Xlib connection. Each goes through the
    /// filter of the input method first.
    pub(crate) fn events(&self, contexts: &dyn Fn(u32) -> Option<*mut XicRec>) -> Vec<ImEvent> {
        let mut events = Vec::new();
        // SAFETY: the display is open; the call does not block.
        while unsafe { (self.lib.XPending)(self.display) } > 0 {
            let mut event = XEvent { pad: [0; 24] };
            // SAFETY: an event is pending, so the call does not block.
            unsafe { (self.lib.XNextEvent)(self.display, &raw mut event) };
            // SAFETY: the event came from Xlib.
            if unsafe { (self.lib.XFilterEvent)(&raw mut event, 0) } != 0 {
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
            // from the input method of `ic`.
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

impl Drop for InputMethod {
    fn drop(&mut self) {
        // SAFETY: the input method is open, and every context closed
        // before, because each holds the method.
        unsafe { (self.lib.XCloseIM)(self.im) };
        // SAFETY: this is the only close of the display.
        unsafe { (self.lib.XCloseDisplay)(self.display) };
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
    pub(crate) fn raw(&self) -> *mut XicRec {
        self.ic
    }

    /// Gives the context the focus, or takes it away.
    pub(crate) fn set_focus(&self, focused: bool) {
        let lib = self.method.lib;
        if focused {
            // SAFETY: the context is open.
            unsafe { (lib.XSetICFocus)(self.ic) };
        } else {
            // SAFETY: the context is open.
            unsafe { (lib.XUnsetICFocus)(self.ic) };
        }
        self.method.flush();
    }

    /// Puts the candidate window of the input method near the caret, at
    /// `x`, `y` pixels of the window.
    pub(crate) fn set_spot(&self, (x, y): (i16, i16)) {
        let lib = self.method.lib;
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
        unsafe { (lib.XSetICValues)(self.ic, name(XN_PREEDIT_ATTRIBUTES), attributes, end()) };
        // SAFETY: the nested list came from `XVaCreateNestedList`.
        unsafe { (lib.XFree)(attributes) };
        self.method.flush();
    }
}

impl Drop for InputContext {
    fn drop(&mut self) {
        // SAFETY: this is the only destruction of the context.
        unsafe { (self.method.lib.XDestroyIC)(self.ic) };
        self.method.flush();
    }
}

/// The layouts of the structs and unions, for the binding check.
#[cfg(test)]
pub(crate) fn layouts() -> Vec<super::checks::StructItem> {
    vec![
        XKeyEvent::layout(),
        XimCallback::layout(),
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

    #[test]
    fn callbacks_queue_the_preedit_changes_of_their_window() {
        let window = std::ptr::without_provenance_mut::<CChar>(0x2a);
        let text = c"xin chào";
        let mut ximtext = XimText {
            length: 8,
            feedback: std::ptr::null_mut(),
            encoding_is_wchar: 0,
            string: text.as_ptr().cast_mut().cast(),
        };
        let mut draw = XimPreeditDraw {
            caret: 3,
            chg_first: 0,
            chg_length: 0,
            text: &raw mut ximtext,
        };
        // SAFETY: the arguments are valid, as Xlib passes them.
        unsafe { preedit_start(std::ptr::null_mut(), window, std::ptr::null_mut()) };
        // SAFETY: as above, with a valid draw structure.
        unsafe { preedit_draw(std::ptr::null_mut(), window, (&raw mut draw).cast()) };
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
}
