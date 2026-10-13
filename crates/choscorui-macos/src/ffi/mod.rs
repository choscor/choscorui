//! The FFI boundary of `choscorui-macos`.
//!
//! All hand-written bindings to the Objective-C runtime and the system
//! frameworks are here. Typed descriptors declare each binding one time,
//! and the checks in `checks` compare every descriptor with the runtime.
//! Each `unsafe` block holds one raw operation with its `SAFETY` comment.
#![deny(
    unsafe_op_in_unsafe_fn,
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    clippy::missing_safety_doc
)]

pub(crate) mod appkit;
pub(crate) mod block;
pub(crate) mod cf;
#[cfg(test)]
pub(crate) mod checks;
pub(crate) mod class;
pub(crate) mod encode;
pub(crate) mod foundation;
pub(crate) mod geometry;
pub(crate) mod graphics;
pub(crate) mod image_io;
pub(crate) mod legacy;
pub(crate) mod msg;
pub(crate) mod retained;
pub(crate) mod runloop;
mod runtime;
pub(crate) mod text;
mod thread;
pub(crate) mod user_notifications;

pub(crate) use class::Obj;
pub(crate) use retained::Pool;
pub(crate) use thread::is_main_thread;
