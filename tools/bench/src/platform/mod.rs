//! The native backend of each OS. Each module gives the same items: the
//! window types, the text shaper, the counting allocator, the loop
//! calls, the process usage, and the machine manifest. The scenarios use
//! only these items, so they do not depend on the OS.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
