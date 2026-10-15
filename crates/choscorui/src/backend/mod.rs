//! Backend selection. This module is the only place where the target
//! operating system picks an implementation. Each backend offers the same
//! crate-private types, `Platform`, `NativeWindow`, and `NativeMenu`, the
//! `capabilities` and `window_support` functions, and the public
//! `platform` escape hatch. Linux gets `linux`, and a target without a backend gets `none`. Its
//! capabilities cannot show windows, so `App::start` returns
//! `AppError::Unsupported`.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::platform;
#[cfg(target_os = "macos")]
pub(crate) use macos::{NativeMenu, NativeWindow, Platform, capabilities, window_support};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::platform;
#[cfg(target_os = "linux")]
pub(crate) use linux::{NativeMenu, NativeWindow, Platform, capabilities, window_support};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod none;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use none::platform;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) use none::{NativeMenu, NativeWindow, Platform, capabilities, window_support};
