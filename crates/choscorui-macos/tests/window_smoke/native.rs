//! A minimal binding to the Objective-C runtime for the smoke test. The
//! test reads the view and accessibility objects as VoiceOver does, and
//! sends native events as the window server does. The public API of the
//! crate has no such seam, so this module holds the test's only `unsafe`.
//! Each `unsafe` item allows the `unsafe_code` lint by itself, with its
//! reason.

use std::ffi::{CStr, CString, c_char, c_void};

/// An Objective-C object, or nil.
pub(crate) type Id = *mut c_void;
/// A registered selector.
type Sel = *mut c_void;

/// `NSPoint`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct NativePoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// `NSRange`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct NativeRange {
    pub(crate) location: usize,
    pub(crate) length: usize,
}

// The test declares the runtime functions that it calls.
#[allow(unsafe_code)]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
}

// The scroll test creates a Core Graphics event, which has precise and line
// deltas that the NSEvent factories cannot make.
#[allow(unsafe_code)]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent2(
        source: Id,
        units: u32,
        count: u32,
        y: i32,
        x: i32,
        z: i32,
    ) -> Id;
}

// The scroll test releases the event that it creates.
#[allow(unsafe_code)]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: Id);
}

/// The class object named `name`, or nil.
#[allow(unsafe_code)] // The test looks up AppKit classes in the runtime.
pub(crate) fn class(name: &CStr) -> Id {
    // SAFETY: `name` is a NUL-terminated string that lives for the call.
    unsafe { objc_getClass(name.as_ptr()) }
}

/// The selector named `name`.
#[allow(unsafe_code)] // The test registers the selectors that it sends.
fn selector(name: &CStr) -> Sel {
    // SAFETY: `name` is a NUL-terminated string that lives for the call.
    unsafe { sel_registerName(name.as_ptr()) }
}

/// An argument tuple of a message.
pub(crate) trait Args {
    /// Sends `selector` to `receiver` with these arguments.
    fn send<R>(self, receiver: Id, selector: Sel) -> R;
}

/// Implements [`Args`] for a tuple.
macro_rules! args {
    ($($arg:ident $index:tt),*) => {
        impl<$($arg),*> Args for ($($arg,)*) {
            // The test sends messages through `objc_msgSend`, cast to the
            // argument types of each message.
            #[allow(unsafe_code)]
            fn send<R>(self, receiver: Id, selector: Sel) -> R {
                let send: extern "C" fn(Id, Sel $(, $arg)*) -> R =
                    // SAFETY: on arm64 and x86-64, `objc_msgSend` takes the
                    // receiver, the selector, and the arguments with their
                    // C ABI. Each call in the test uses the SDK argument and
                    // result types of its selector, and no result is a
                    // struct larger than 16 bytes.
                    unsafe { std::mem::transmute(objc_msgSend as *const ()) };
                send(receiver, selector $(, self.$index)*)
            }
        }
    };
}

args!();
args!(A0 0);
args!(A0 0, A1 1);
args!(A0 0, A1 1, A2 2);
args!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6, A7 7, A8 8);
args!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6, A7 7, A8 8, A9 9);

/// Sends the message `name` to `receiver`. The argument and result types
/// must be the SDK types of the selector.
pub(crate) fn send<A: Args, R>(receiver: Id, name: &CStr, args: A) -> R {
    args.send(receiver, selector(name))
}

/// A new autoreleased `NSString` with `text`.
pub(crate) fn ns_string(text: &str) -> Id {
    let text = CString::new(text).unwrap();
    send(
        class(c"NSString"),
        c"stringWithUTF8String:",
        (text.as_ptr(),),
    )
}

/// The text of the `NSString` `string`.
#[allow(unsafe_code)] // The test reads the UTF-8 buffer of an NSString.
pub(crate) fn rust_string(string: Id) -> String {
    let utf8: *const c_char = send(string, c"UTF8String", ());
    assert!(!utf8.is_null());
    // SAFETY: `UTF8String` returns a NUL-terminated buffer that lives as
    // long as the string, which the current autorelease pool keeps.
    unsafe { CStr::from_ptr(utf8) }.to_str().unwrap().to_owned()
}

/// The first window of the application.
pub(crate) fn first_window() -> Id {
    let application: Id = send(class(c"NSApplication"), c"sharedApplication", ());
    let windows: Id = send(application, c"windows", ());
    let window: Id = send(windows, c"objectAtIndex:", (0usize,));
    assert!(!window.is_null());
    window
}

/// The content view of the first window.
pub(crate) fn content_view() -> Id {
    send(first_window(), c"contentView", ())
}

/// The accessibility child of the content view at `index`.
pub(crate) fn accessibility_child(index: usize) -> Id {
    let children: Id = send(content_view(), c"accessibilityChildren", ());
    send(children, c"objectAtIndex:", (index,))
}

/// A scroll-wheel `NSEvent` from a Core Graphics event with `units`
/// (0 for pixels, 1 for lines) and a vertical delta of `y`.
#[allow(unsafe_code)] // The test creates and releases a Core Graphics event.
pub(crate) fn scroll_event(units: u32, y: i32) -> Id {
    // SAFETY: a null source is allowed, and one wheel with these deltas is
    // a valid scroll event.
    let cg = unsafe { CGEventCreateScrollWheelEvent2(std::ptr::null_mut(), units, 1, y, 0, 0) };
    assert!(!cg.is_null());
    let event: Id = send(class(c"NSEvent"), c"eventWithCGEvent:", (cg,));
    // SAFETY: the create function returned a +1 event, and the autoreleased
    // NSEvent holds its own reference.
    unsafe { CFRelease(cg) };
    assert!(!event.is_null());
    event
}
