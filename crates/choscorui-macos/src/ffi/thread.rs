//! The main-thread check that AppKit entry points make first.

use std::ffi::c_int;

unsafe extern "C" {
    safe fn pthread_main_np() -> c_int;
}

/// True on the process main thread, the only thread that may use AppKit.
pub(crate) fn is_main_thread() -> bool {
    pthread_main_np() != 0
}
