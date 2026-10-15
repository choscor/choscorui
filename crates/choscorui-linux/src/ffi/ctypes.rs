//! The C scalar types of the declarations.
//!
//! The Linux targets are LP64: `int` is 32 bits, and `long` and pointers
//! are 64 bits. `CChar` is the C `char` type. It is a distinct Rust type,
//! so the binding check can tell `char *` from `signed char *` and
//! `unsigned char *`.

pub(crate) use std::ffi::c_void;

pub(crate) type CInt = i32;
pub(crate) type CUint = u32;
pub(crate) type CLong = i64;
pub(crate) type CUlong = u64;

/// The C `char` type.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CChar(pub(crate) std::ffi::c_char);
