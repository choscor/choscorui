//! The declarations of libcanberra, and the event sound `bell`: the alert
//! sound when the display server has no bell of its own.

use super::ctypes::{CChar, CInt, c_void};
use super::macros::{libraries, opaque_types};
use std::cell::Cell;

opaque_types! {
    /// `ca_context`.
    pub(crate) CaContext = "ca_context";
    /// `ca_proplist`.
    pub(crate) CaProplist = "ca_proplist";
}

/// `ca_finish_callback_t`.
pub(crate) type FinishCallback =
    Option<unsafe extern "C" fn(*mut CaContext, u32, CInt, *mut c_void)>;

libraries! {
    /// libcanberra: event sounds of the sound theme.
    pub(crate) struct Canberra {
        library: "libcanberra",
        sonames: ["libcanberra.so.0"],
        headers: ["canberra.h"],
        pkg_config: "libcanberra",
        package: "libcanberra-dev",
        functions {
            fn ca_context_create(context: *mut *mut CaContext) -> CInt;
            fn ca_context_destroy(context: *mut CaContext) -> CInt;
            fn ca_context_play_full(
                context: *mut CaContext,
                id: u32,
                properties: *mut CaProplist,
                callback: FinishCallback,
                data: *mut c_void,
            ) -> CInt;
            fn ca_proplist_create(list: *mut *mut CaProplist) -> CInt;
            fn ca_proplist_destroy(list: *mut CaProplist) -> CInt;
            fn ca_proplist_sets(list: *mut CaProplist, key: *const CChar, value: *const CChar) -> CInt;
        }
    }
}

/// The context of this thread. A sound plays after `ca_context_play_full`
/// returns, so the context lives until the thread ends.
struct Context(*mut CaContext);

impl Drop for Context {
    fn drop(&mut self) {
        if let Ok(lib) = Canberra::get() {
            // SAFETY: the context came from `ca_context_create` and this
            // is its only release.
            unsafe { (lib.ca_context_destroy)(self.0) };
        }
    }
}

thread_local! {
    static CONTEXT: Cell<Option<Context>> = const { Cell::new(None) };
}

/// Plays the event sound `bell` of the sound theme. Returns false when
/// libcanberra does not load or does not play the sound.
pub(crate) fn play_bell() -> bool {
    let Ok(lib) = Canberra::get() else {
        return false;
    };
    let context = CONTEXT.with(|slot| {
        if let Some(context) = slot.take() {
            let raw = context.0;
            slot.set(Some(context));
            return Some(raw);
        }
        let mut raw = std::ptr::null_mut();
        // SAFETY: `raw` is a valid out pointer for the new context.
        let status = unsafe { (lib.ca_context_create)(&raw mut raw) };
        if status != 0 || raw.is_null() {
            return None;
        }
        slot.set(Some(Context(raw)));
        Some(raw)
    });
    let Some(context) = context else {
        return false;
    };
    let mut list = std::ptr::null_mut();
    // SAFETY: `list` is a valid out pointer for the new property list.
    if unsafe { (lib.ca_proplist_create)(&raw mut list) } != 0 || list.is_null() {
        return false;
    }
    let (key, value) = (c"event.id", c"bell");
    // SAFETY: `list` is live, and both strings end with NUL; the call
    // copies them.
    let set = unsafe { (lib.ca_proplist_sets)(list, key.as_ptr().cast(), value.as_ptr().cast()) };
    // SAFETY: `context` and `list` are live. No callback is given, so
    // the call keeps no pointer to Rust data.
    let played = set == 0
        && unsafe { (lib.ca_context_play_full)(context, 0, list, None, std::ptr::null_mut()) } == 0;
    // SAFETY: `list` came from `ca_proplist_create`; the context copied
    // the properties it needs.
    unsafe { (lib.ca_proplist_destroy)(list) };
    played
}
