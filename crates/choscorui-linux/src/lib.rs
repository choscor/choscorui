//! The Wayland and X11 backend of ChoscorUI.
//!
//! The protocol state machines (`select_backend`, `WaylandSurface`,
//! `FrameScheduler`, `X11Window`, `LinuxInput`, and `Clipboard`) are
//! platform-neutral and compile on every host. The native backend
//! compiles only for Linux. All of its `unsafe` code is in `ffi`.
//!
//! On Linux, `decode_image` and `encode_png` convert between encoded
//! images and `choscorui_core::Image` with the system codec libraries.
//! `process_usage` and `CountingAllocator` give the resource figures of
//! `choscorui-bench` and the lifetime smoke test.

#![deny(unsafe_code)]

mod backend;
mod clipboard;
mod frame;
#[cfg(target_os = "linux")]
mod imaging;
mod input;
mod wayland;
mod x11;

pub use backend::*;
pub use clipboard::*;
pub use frame::*;
#[cfg(target_os = "linux")]
pub use imaging::{decode_image, encode_png, image_codecs_support};
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
pub use ffi::{CountingAllocator, allocation_count};
#[cfg(target_os = "linux")]
mod process;
#[cfg(target_os = "linux")]
pub use process::{ProcessUsage, ProcessUsageError, process_usage};
#[cfg(target_os = "linux")]
mod native;
#[cfg(target_os = "linux")]
pub use native::*;

// The text system and the painters use the safe wrappers of `ffi`.
#[cfg(target_os = "linux")]
mod gles;
#[cfg(target_os = "linux")]
pub use gles::{GlesError, render_offscreen_gles};
#[cfg(target_os = "linux")]
mod paint;
#[cfg(target_os = "linux")]
mod text;
#[cfg(target_os = "linux")]
pub use paint::{CairoError, render_offscreen};
#[cfg(target_os = "linux")]
pub use text::{LinuxTextShaper, available_font_families, set_desktop_font, text_shaping_support};

#[cfg(target_os = "linux")]
pub mod atspi;
#[cfg(target_os = "linux")]
pub mod dbus;
#[cfg(target_os = "linux")]
pub mod services;
