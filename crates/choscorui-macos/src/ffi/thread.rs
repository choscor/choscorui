//! The main-thread check that AppKit entry points make first, and the
//! quality of service of the main thread.

use super::sdk::{c_functions, sdk_const};
use std::ffi::c_int;

sdk_const! {
    /// `QOS_CLASS_USER_INTERACTIVE`, the class of the thread that draws
    /// the user interface.
    QOS_CLASS_USER_INTERACTIVE: u32 = 0x21, sdk = QOS_CLASS_USER_INTERACTIVE;
}

c_functions! {
    functions {
        safe fn pthread_main_np() -> c_int;
        safe fn pthread_set_qos_class_self_np(class: u32, relative_priority: c_int) -> c_int;
    }
}

/// True on the process main thread, the only thread that may use AppKit.
pub(crate) fn is_main_thread() -> bool {
    pthread_main_np() != 0
}

/// Gives the calling thread the user-interactive quality of service. An
/// app bundle gets it on its main thread from the system, but a process
/// started from a shell does not, and the scheduler then can run its
/// frames on efficiency cores. A failure leaves the class unchanged.
pub(crate) fn make_user_interactive() {
    pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
}
