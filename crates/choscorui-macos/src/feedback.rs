//! [`SystemFeedback`]: the AppKit signals that controls send, such as the
//! alert sound.

use crate::ffi::appkit::NSBeep;
use choscorui_core::Feedback;

/// The [`Feedback`] of macOS. Give it to a view host so that its controls
/// play the system alert sound.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFeedback;

impl Feedback for SystemFeedback {
    fn beep(&self) {
        NSBeep();
    }
}
