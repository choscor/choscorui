//! The Wayland and X11 backend of ChoscorUI.
//!
//! The protocol state machines (`select_backend`, `WaylandSurface`,
//! `FrameScheduler`, `X11Window`, `LinuxInput`, and `Clipboard`) are
//! platform-neutral and compile on every host. The native backend
//! compiles only for Linux. All of its `unsafe` code is in `ffi`.

#![deny(unsafe_code)]

mod backend;
mod clipboard;
mod frame;
mod input;
mod wayland;
mod x11;

pub use backend::*;
pub use clipboard::*;
pub use frame::*;
pub use input::*;
pub use wayland::*;
pub use x11::*;

// All `unsafe` of the crate is in `ffi`: each block is one raw FFI
// operation with its `SAFETY` comment, and the checks in `ffi::checks`
// compile every declaration against the system headers.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod ffi;
#[cfg(target_os = "linux")]
pub use ffi::LoadError;

#[cfg(target_os = "linux")]
pub mod dbus;
#[cfg(target_os = "linux")]
pub mod services;
