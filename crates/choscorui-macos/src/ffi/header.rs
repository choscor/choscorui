//! The SDK header type of each message and callback argument and result
//! type, for the message and callback parts of the SDK header check.

use super::block::Block;
use super::class::{AnyClass, AnyProtocol, Obj, Receiver, ReceiverKind, Sel};
use super::encode::Bool;
use super::geometry::{CGAffineTransform, NSPoint, NSRange, NSRect, NSSize};
use super::metal::{MTLClearColor, MTLOrigin, MTLRegion, MTLScissorRect, MTLSize, MTLViewport};
use super::msg::{Autoreleased, Init, Shared, Utf8};
use super::quartz::CAFrameRateRange;
use super::retained::Retained;
use std::ffi::CStr;

/// How the generated check passes an argument or compares a result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Check {
    /// An integer, a floating-point number, `BOOL`, or `void`. The check
    /// compares the ABI shape, because C++ does not convert an integer to
    /// an enum.
    Scalar,
    /// A pointer whose target type the descriptor does not name.
    AnyPointer,
    /// An object, a Core Foundation object, a block, a selector, or a
    /// struct. The compiler converts it as in Objective-C.
    Typed,
}

/// One header type and how the check uses it.
#[derive(Clone, Debug)]
pub(crate) struct HeaderType {
    pub(crate) name: String,
    pub(crate) check: Check,
}

/// The header types of a message or a callback: the result and each
/// argument.
pub(crate) struct Signature {
    pub(crate) ret: HeaderType,
    pub(crate) args: Vec<HeaderType>,
}

/// The SDK header type of an FFI type.
pub(crate) trait Header {
    const CHECK: Check = Check::Typed;
    /// Appends the C type, for example `NSWindow *` or `NSUInteger`.
    fn header(out: &mut String);
}

/// The header type of `T`.
pub(crate) fn header_type<T: Header>() -> HeaderType {
    let mut name = String::new();
    T::header(&mut name);
    HeaderType {
        name,
        check: T::CHECK,
    }
}

/// The signature of a message that takes `A` and returns `R`.
pub(crate) fn signature<A: HeaderArgs, R: Header>() -> Signature {
    let mut args = Vec::new();
    A::headers(&mut args);
    Signature {
        ret: header_type::<R>(),
        args,
    }
}

/// The header type of the class or protocol `name` as a receiver.
pub(crate) fn receiver_header(name: &CStr, kind: ReceiverKind) -> String {
    let name = name.to_string_lossy();
    match kind {
        ReceiverKind::Class => format!("{name} *"),
        ReceiverKind::Protocol => format!("id<{name}>"),
    }
}

/// Appends the header type of an object of class or protocol `C`. A plain
/// `NSObject` handle stands for any object, `id`.
fn object_header<C: Receiver>(out: &mut String) {
    if C::KIND == ReceiverKind::Class && C::NAME == c"NSObject" {
        out.push_str("id");
    } else {
        out.push_str(&receiver_header(C::NAME, C::KIND));
    }
}

/// Implements [`Header`] for types with a fixed header spelling.
macro_rules! header_as {
    ($check:ident: $($ty:ty => $header:literal),* $(,)?) => {$(
        impl Header for $ty {
            const CHECK: Check = Check::$check;
            fn header(out: &mut String) {
                out.push_str($header);
            }
        }
    )*};
}

header_as!(Scalar:
    () => "void",
    bool => "BOOL",
    Bool => "BOOL",
    isize => "NSInteger",
    usize => "NSUInteger",
    i16 => "int16_t",
    u16 => "uint16_t",
    f32 => "float",
    f64 => "double",
);
header_as!(Typed:
    NSPoint => "NSPoint",
    NSSize => "NSSize",
    NSRect => "NSRect",
    NSRange => "NSRange",
    CGAffineTransform => "CGAffineTransform",
    MTLClearColor => "MTLClearColor",
    MTLOrigin => "MTLOrigin",
    MTLSize => "MTLSize",
    MTLRegion => "MTLRegion",
    MTLScissorRect => "MTLScissorRect",
    MTLViewport => "MTLViewport",
    CAFrameRateRange => "CAFrameRateRange",
    &AnyClass => "Class",
    &AnyProtocol => "Protocol *",
    Sel => "SEL",
    Option<Sel> => "SEL",
    &CStr => "const char *",
    Utf8 => "const char *",
);

impl<T> Header for *const T {
    const CHECK: Check = Check::AnyPointer;
    fn header(out: &mut String) {
        out.push_str("const void *");
    }
}

impl<T> Header for *mut T {
    const CHECK: Check = Check::AnyPointer;
    fn header(out: &mut String) {
        out.push_str("void *");
    }
}

impl<T: Header> Header for Option<&mut T> {
    fn header(out: &mut String) {
        T::header(out);
        out.push_str(" *");
    }
}

/// Implements [`Header`] for the object handles and result markers of
/// class or protocol `C`.
macro_rules! object_as {
    ($($ty:ty),* $(,)?) => {$(
        impl<C: Receiver> Header for $ty {
            fn header(out: &mut String) {
                object_header::<C>(out);
            }
        }
    )*};
}

object_as!(
    &Obj<C>,
    Option<&Obj<C>>,
    Option<Retained<C>>,
    Autoreleased<C>,
    Shared<C>,
    Init<C>,
);

impl<B: HeaderArgs> Header for &Block<B> {
    fn header(out: &mut String) {
        let mut args = Vec::new();
        B::headers(&mut args);
        let args: Vec<_> = args.into_iter().map(|arg| arg.name).collect();
        out.push_str(&format!("void (^)({})", args.join(", ")));
    }
}

impl<B: HeaderArgs> Header for Option<&Block<B>> {
    fn header(out: &mut String) {
        <&Block<B>>::header(out);
    }
}

/// The header types of an argument tuple.
pub(crate) trait HeaderArgs {
    fn headers(out: &mut Vec<HeaderType>);
}

/// Implements [`HeaderArgs`] for a tuple of [`Header`] types.
macro_rules! header_args {
    ($($arg:ident),*) => {
        impl<$($arg: Header),*> HeaderArgs for ($($arg,)*) {
            fn headers(_out: &mut Vec<HeaderType>) {
                $(_out.push(header_type::<$arg>());)*
            }
        }
    };
}

header_args!();
header_args!(A0);
header_args!(A0, A1);
header_args!(A0, A1, A2);
header_args!(A0, A1, A2, A3);
header_args!(A0, A1, A2, A3, A4);
header_args!(A0, A1, A2, A3, A4, A5);
header_args!(A0, A1, A2, A3, A4, A5, A6);
header_args!(A0, A1, A2, A3, A4, A5, A6, A7);
header_args!(A0, A1, A2, A3, A4, A5, A6, A7, A8);
header_args!(A0, A1, A2, A3, A4, A5, A6, A7, A8, A9);
