//! The C side of the boundary: the macros that declare C functions and
//! statics, SDK constants, and `repr(C)` structs.
//!
//! Each macro emits the real declaration and, in tests, a descriptor from
//! the same tokens. `checks::sdk` generates an Objective-C++ file from the
//! descriptors and compiles it against the SDK headers, so a declaration
//! cannot drift from its header.

#[cfg(test)]
use super::cf::{CfRetained, CfType};
#[cfg(test)]
use super::class::Sel;
#[cfg(test)]
use super::encode::Bool;
#[cfg(test)]
use std::ptr::NonNull;

/// The ABI shape of a C type, which the SDK header check compares with the
/// header type. One Rust type, such as `&CFType`, stands for many distinct
/// header types, so the check compares shapes and not names.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape {
    Void,
    /// A data, object, block, or function pointer.
    Pointer,
    /// A signed integer of this many bytes.
    Signed(usize),
    /// An unsigned integer of this many bytes.
    Unsigned(usize),
    /// A floating-point number of this many bytes.
    Float(usize),
    /// A struct with its C name and size.
    Struct(&'static str, usize),
    /// A C type that Rust keeps opaque. The header type must be this type.
    Opaque(&'static str),
    /// A C array of this many bytes.
    Array(usize),
}

/// The ABI shape of an FFI type.
#[cfg(test)]
pub(crate) trait CShape {
    const SHAPE: Shape;
    /// True for a +1 Core Foundation result.
    const OWNED: bool = false;
}

/// Implements [`CShape`] for numbers.
#[cfg(test)]
macro_rules! number_shape {
    ($kind:ident: $($ty:ty),*) => {$(
        impl CShape for $ty {
            const SHAPE: Shape = Shape::$kind(size_of::<$ty>());
        }
    )*};
}

#[cfg(test)]
number_shape!(Signed: i8, i16, i32, i64, isize);
#[cfg(test)]
number_shape!(Unsigned: u8, u16, u32, u64, usize, bool);
#[cfg(test)]
number_shape!(Float: f32, f64);

#[cfg(test)]
impl CShape for () {
    const SHAPE: Shape = Shape::Void;
}

/// Objective-C `BOOL`: `bool` on arm64 and `signed char` on x86-64.
#[cfg(test)]
impl CShape for Bool {
    const SHAPE: Shape = if cfg!(target_arch = "aarch64") {
        Shape::Unsigned(1)
    } else {
        Shape::Signed(1)
    };
}

/// Implements [`CShape`] as [`Shape::Pointer`] for pointer types.
#[cfg(test)]
macro_rules! pointer_shape {
    ($(<$($param:ident),*> $ty:ty),* $(,)?) => {$(
        impl<$($param: ?Sized),*> CShape for $ty {
            const SHAPE: Shape = Shape::Pointer;
        }
    )*};
}

#[cfg(test)]
pointer_shape!(
    <T> *const T,
    <T> *mut T,
    <T> &T,
    <T> Option<&T>,
    <T> &mut T,
    <T> Option<&mut T>,
    <T> NonNull<T>,
    <T> Option<NonNull<T>>,
    <> Sel,
    <> Option<Sel>,
);

#[cfg(test)]
impl<T: CfType> CShape for Option<CfRetained<T>> {
    const SHAPE: Shape = Shape::Pointer;
    const OWNED: bool = true;
}

/// A +1 Objective-C object from a C function of the create rule, such as
/// `MTLCreateSystemDefaultDevice`.
#[cfg(test)]
impl<C> CShape for Option<super::retained::Retained<C>> {
    const SHAPE: Shape = Shape::Pointer;
    const OWNED: bool = true;
}

#[cfg(test)]
impl<T, const N: usize> CShape for [T; N] {
    const SHAPE: Shape = Shape::Array(size_of::<[T; N]>());
}

/// Implements [`CShape`] for C function pointers and their nullable form.
#[cfg(test)]
macro_rules! function_shape {
    ($($arg:ident),*) => {
        impl<R $(, $arg)*> CShape for unsafe extern "C" fn($($arg),*) -> R {
            const SHAPE: Shape = Shape::Pointer;
        }
        impl<R $(, $arg)*> CShape for Option<unsafe extern "C" fn($($arg),*) -> R> {
            const SHAPE: Shape = Shape::Pointer;
        }
    };
}

#[cfg(test)]
function_shape!(A0);
#[cfg(test)]
function_shape!(A0, A1);
#[cfg(test)]
function_shape!(A0, A1, A2);

/// A C function or static, as `c_functions!` records it.
#[cfg(test)]
#[derive(Debug)]
pub(crate) enum CItem {
    Static {
        name: &'static str,
        shape: Shape,
    },
    Function {
        name: &'static str,
        args: &'static [Shape],
        ret: Shape,
        /// True when the result is a `CfRetained`.
        owned: bool,
    },
}

/// An SDK constant with its Rust value and its SDK expression.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SdkConst {
    pub(crate) name: &'static str,
    pub(crate) sdk: &'static str,
    pub(crate) value: i128,
}

/// The layout of a `repr(C)` struct, as `ffi_struct!` records it.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct CStruct {
    pub(crate) name: &'static str,
    pub(crate) size: usize,
    pub(crate) align: usize,
    /// The C name and the offset of each field.
    pub(crate) fields: &'static [(&'static str, usize)],
}

/// Declares the C functions and statics of one framework, in one
/// `extern "C-unwind"` block:
///
/// ```ignore
/// c_functions! {
///     #[link(name = "CoreFoundation", kind = "framework")]
///     statics {
///         pub(crate) safe static kCFBooleanTrue: &'static CFBoolean;
///     }
///     functions {
///         safe fn CFGetTypeID(object: &CFType) -> usize;
///         unsafe fn CFRelease(object: *const c_void);
///     }
/// }
/// ```
///
/// The `#[link]` attribute is optional for the system library. Each item
/// says `safe` or `unsafe`. Only `#[cfg]` attributes are
/// allowed on an item. In tests the macro also emits `c_items()`, the
/// descriptors that the SDK header check compiles.
macro_rules! c_functions {
    (
        $(#[link($($link:tt)*)])?
        $(statics {$(
            $(#[cfg($static_cfg:meta)])*
            $static_vis:vis $static_safety:ident static $static:ident : $static_ty:ty;
        )*})?
        functions {$(
            $(#[cfg($cfg:meta)])*
            $vis:vis $safety:ident fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?;
        )*}
    ) => {
        $(#[link($($link)*)])?
        unsafe extern "C-unwind" {
            $($(
                $(#[cfg($static_cfg)])*
                $static_vis $static_safety static $static: $static_ty;
            )*)?
            $(
                $(#[cfg($cfg)])*
                $vis $safety fn $name($($arg: $ty),*) $(-> $ret)?;
            )*
        }

        /// The descriptors of the C functions and statics of this module.
        #[cfg(test)]
        pub(crate) fn c_items() -> Vec<$crate::ffi::sdk::CItem> {
            use $crate::ffi::sdk::{CItem, CShape};
            let mut items = Vec::new();
            $($(
                $(#[cfg($static_cfg)])*
                items.push(CItem::Static {
                    name: stringify!($static),
                    shape: <$static_ty as CShape>::SHAPE,
                });
            )*)?
            $(
                $(#[cfg($cfg)])*
                items.push(CItem::Function {
                    name: stringify!($name),
                    args: const { &[$(<$ty as CShape>::SHAPE),*] },
                    ret: <$crate::ffi::sdk::c_functions!(@ret $($ret)?) as CShape>::SHAPE,
                    owned: <$crate::ffi::sdk::c_functions!(@ret $($ret)?) as CShape>::OWNED,
                });
            )*
            items
        }
    };
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };
}
pub(crate) use c_functions;

/// Declares SDK constants with the SDK expression that each one equals:
///
/// ```ignore
/// sdk_const! {
///     pub(crate) WINDOW_STYLE_FULL_SCREEN: usize = 1 << 14, sdk = NSWindowStyleMaskFullScreen;
///     pub(crate) GRADIENT_EXTEND: u32 = 1 | 2,
///         sdk = kCGGradientDrawsBeforeStartLocation | kCGGradientDrawsAfterEndLocation;
/// }
/// ```
///
/// In tests the macro also emits `SDK_CONSTS`, which the SDK header check
/// compares with the headers.
macro_rules! sdk_const {
    ($(
        $(#[doc = $doc:literal])*
        $vis:vis $name:ident : $ty:ty = $value:expr, sdk = $($sdk:ident)|+;
    )*) => {
        $(
            $(#[doc = $doc])*
            $vis const $name: $ty = $value;
        )*

        /// The SDK constants of this module, for the SDK header check.
        #[cfg(test)]
        pub(crate) const SDK_CONSTS: &[$crate::ffi::sdk::SdkConst] = &[$(
            $crate::ffi::sdk::SdkConst {
                name: stringify!($name),
                sdk: stringify!($($sdk)|+),
                value: $name as i128,
            }
        ),*];
    };
}
pub(crate) use sdk_const;

/// Declares `repr(C)` structs with the C name of each struct and, where it
/// differs, of each field:
///
/// ```ignore
/// ffi_struct! {
///     /// `NSPoint`, the same type as `CGPoint`.
///     #[derive(Clone, Copy)]
///     pub(crate) struct NSPoint = CGPoint {
///         pub(crate) x: f64,
///         pub(crate) y: f64,
///     }
///     struct ObjcSuper = objc_super {
///         receiver: NonNull<Obj<NSObject>>,
///         superclass as super_class: &'static AnyClass,
///     }
/// }
/// ```
///
/// In tests the macro also emits `STRUCTS`, the layouts that the SDK
/// header check compares with the headers.
macro_rules! ffi_struct {
    ($(
        $(#[$meta:meta])*
        $vis:vis struct $name:ident = $c_name:ident {$(
            $(#[$field_meta:meta])*
            $field_vis:vis $field:ident $(as $c_field:ident)? : $field_ty:ty
        ),* $(,)?}
    )*) => {
        $(
            $(#[$meta])*
            #[repr(C)]
            $vis struct $name {$(
                $(#[$field_meta])*
                $field_vis $field: $field_ty,
            )*}

            #[cfg(test)]
            impl $crate::ffi::sdk::CShape for $name {
                const SHAPE: $crate::ffi::sdk::Shape =
                    $crate::ffi::sdk::Shape::Struct(stringify!($c_name), size_of::<$name>());
            }
        )*

        /// The struct layouts of this module, for the SDK header check.
        #[cfg(test)]
        pub(crate) const STRUCTS: &[$crate::ffi::sdk::CStruct] = &[$(
            $crate::ffi::sdk::CStruct {
                name: stringify!($c_name),
                size: size_of::<$name>(),
                align: align_of::<$name>(),
                fields: &[$((
                    $crate::ffi::sdk::ffi_struct!(@field $field $($c_field)?),
                    ::std::mem::offset_of!($name, $field),
                )),*],
            }
        ),*];
    };
    (@field $field:ident) => { stringify!($field) };
    (@field $field:ident $c_field:ident) => { stringify!($c_field) };
}
pub(crate) use ffi_struct;
