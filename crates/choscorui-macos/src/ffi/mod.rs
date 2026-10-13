//! The FFI boundary of `choscorui-macos`.
//!
//! All hand-written bindings to the Objective-C runtime and the system
//! frameworks are here. Typed descriptors declare each binding one time,
//! and the checks in `checks` compare every descriptor with the runtime.
//! Each `unsafe` block holds one raw operation with its `SAFETY` comment.

pub(crate) mod appkit;
pub(crate) mod block;
pub(crate) mod callback;
pub(crate) mod cf;
#[cfg(test)]
pub(crate) mod checks;
pub(crate) mod class;
mod classes;
pub(crate) mod encode;
pub(crate) mod foundation;
pub(crate) mod geometry;
pub(crate) mod graphics;
#[cfg(test)]
pub(crate) mod header;
pub(crate) mod image_io;
pub(crate) mod keys;
pub(crate) mod metal;
pub(crate) mod msg;
pub(crate) mod quartz;
pub(crate) mod retained;
pub(crate) mod runloop;
mod runtime;
pub(crate) mod sdk;
pub(crate) mod text;
mod thread;
pub(crate) mod user_notifications;

pub(crate) use class::Obj;
pub(crate) use retained::Pool;
pub(crate) use thread::is_main_thread;
