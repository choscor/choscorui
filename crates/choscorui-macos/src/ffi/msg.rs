//! Typed message descriptors.
//!
//! `Msg<C, A, R>` declares one message: the receiver class or protocol
//! `C`, the argument tuple `A`, and the return type `R`. The descriptor
//! types use `'static` borrows, for example `(&Obj<NSString>,)`. A send
//! binds them to the lifetimes of the actual arguments. `Msg::send` is
//! safe: its one `unsafe` operation casts `objc_msgSend` to the exact
//! `extern "C-unwind"` function type that the descriptor names.

use super::class::{AnyClass, ClassType, IsA, Obj, Receiver, Sel};
use super::encode::Bool;
#[cfg(test)]
use super::encode::Encode;
use super::retained::Retained;
use super::runtime;
use crate::Error;
use std::ffi::{CStr, c_char};
use std::marker::PhantomData;

/// In tests every argument type also implements [`Encode`]; in other
/// builds the encodings are not compiled.
#[cfg(test)]
pub(crate) trait ArgEncode: Encode {}
#[cfg(test)]
impl<T: Encode> ArgEncode for T {}
#[cfg(not(test))]
pub(crate) trait ArgEncode {}
#[cfg(not(test))]
impl<T> ArgEncode for T {}

/// Whether a message goes to instances or to the class object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Instance,
    Class,
}

/// How the result of a message is owned. The ownership check compares it
/// with the Cocoa naming rules.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Ownership {
    /// Not an object.
    Value,
    /// A +1 object from the `alloc`, `new`, `copy`, `mutableCopy`, or
    /// `init` family.
    Owned,
    /// An object that the receiver keeps alive, borrowed from it.
    Borrowed,
    /// An autoreleased object that the send retains explicitly.
    Retains,
    /// An object that lives for the whole process.
    Shared,
}

/// One message: selector, receiver, arguments, and return type.
pub(crate) struct Msg<C, A, R> {
    selector: &'static CStr,
    kind: Kind,
    _types: PhantomData<fn(&C, A) -> R>,
}

impl<C, A, R> Msg<C, A, R> {
    /// A message to instances of `C`.
    pub(crate) const fn instance(selector: &'static CStr) -> Self {
        Self {
            selector,
            kind: Kind::Instance,
            _types: PhantomData,
        }
    }

    /// A message to the class object of `C`.
    pub(crate) const fn class(selector: &'static CStr) -> Self {
        Self {
            selector,
            kind: Kind::Class,
            _types: PhantomData,
        }
    }
}

impl<C: Receiver, A: Args, R: Ret> Msg<C, A, R> {
    /// Sends the message to `receiver`. A borrowed result lives as long as
    /// the shortest borrow among the receiver and the arguments.
    pub(crate) fn send<'a>(&self, receiver: &'a Obj<C>, args: A::In<'a>) -> R::Out<'a> {
        debug_assert_eq!(self.kind, Kind::Instance, "{:?}", self.selector);
        R::out(A::invoke::<_, R>(
            receiver,
            Sel::register(self.selector),
            args,
        ))
    }
}

impl<C: ClassType, A: Args, R: ClassRet> Msg<C, A, R> {
    /// Sends the message to the class object of `C`.
    pub(crate) fn send_class<'a>(&self, args: A::In<'a>) -> Result<R::Out<'a>, Error> {
        self.send_to_class(C::class()?, args)
    }

    /// Sends the message to `class`, a subclass of `C`.
    pub(in crate::ffi) fn send_to_class<'a>(
        &self,
        class: &'static AnyClass,
        args: A::In<'a>,
    ) -> Result<R::Out<'a>, Error> {
        debug_assert_eq!(self.kind, Kind::Class, "{:?}", self.selector);
        Ok(R::out(A::invoke::<_, R>(
            class,
            Sel::register(self.selector),
            args,
        )))
    }
}

/// The return marker of an `init` message: the send consumes the +1
/// receiver and returns a +1 object or nil.
pub(crate) struct Init<X>(PhantomData<fn() -> X>);

impl<C: ClassType, A: Args> Msg<C, A, Init<C>> {
    /// Sends the `init` message to a newly allocated `receiver`, which can
    /// be an instance of a subclass `S`.
    pub(crate) fn init<'a, S: IsA<C>>(
        &self,
        receiver: Retained<S>,
        args: A::In<'a>,
    ) -> Option<Retained<S>> {
        debug_assert_eq!(self.kind, Kind::Instance, "{:?}", self.selector);
        let receiver = receiver.into_raw();
        let result = A::invoke::<_, Option<Retained<C>>>(
            receiver.cast::<Obj<C>>(),
            Sel::register(self.selector),
            args,
        );
        result.map(Retained::cast_owned)
    }
}

#[cfg(test)]
impl<X: ClassType> Encode for Init<X> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// An argument type of a message descriptor.
pub(crate) trait Arg: 'static {
    /// The type that a send takes.
    type In<'a>;
    /// The ABI type that `objc_msgSend` receives.
    type Raw<'a>: Copy;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a>;
}

/// A return type of a message descriptor.
pub(crate) trait Ret: 'static {
    /// The ABI type that `objc_msgSend` returns.
    type Raw<'a>;
    /// The type that a send returns.
    type Out<'a>;
    /// How the result is owned.
    #[cfg(test)]
    const OWNERSHIP: Ownership;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a>;
}

/// A return type whose result does not borrow from the receiver, so a
/// class message can return it.
pub(crate) trait ClassRet: Ret {}

/// Implements [`Arg`] and [`Ret`] for a type that crosses unchanged.
macro_rules! plain_value {
    ($($ty:ty),* $(,)?) => {$(
        impl Arg for $ty {
            type In<'a> = $ty;
            type Raw<'a> = $ty;
            fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
                value
            }
        }
        impl Ret for $ty {
            type Raw<'a> = $ty;
            type Out<'a> = $ty;
            #[cfg(test)]
            const OWNERSHIP: Ownership = Ownership::Value;
            fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
                raw
            }
        }
        impl ClassRet for $ty {}
    )*};
}

plain_value!(isize, usize, i16, u16, f64);

impl Ret for () {
    type Raw<'a> = ();
    type Out<'a> = ();
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Value;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}
impl ClassRet for () {}

impl Arg for bool {
    type In<'a> = bool;
    type Raw<'a> = Bool;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        Bool::new(value)
    }
}

impl Ret for bool {
    type Raw<'a> = Bool;
    type Out<'a> = bool;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Value;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw.get()
    }
}
impl ClassRet for bool {}

#[cfg(test)]
impl Encode for bool {
    fn encode(out: &mut String) {
        Bool::encode(out);
    }
}

impl<C: Receiver> Arg for &'static Obj<C> {
    type In<'a> = &'a Obj<C>;
    type Raw<'a> = &'a Obj<C>;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

impl<C: Receiver> Arg for Option<&'static Obj<C>> {
    type In<'a> = Option<&'a Obj<C>>;
    type Raw<'a> = Option<&'a Obj<C>>;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

/// A borrowed object result that the receiver keeps alive.
impl<C: Receiver> Ret for Option<&'static Obj<C>> {
    type Raw<'a> = Option<&'a Obj<C>>;
    type Out<'a> = Option<&'a Obj<C>>;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Borrowed;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}

#[cfg(test)]
impl<C: Receiver> Encode for Option<&Obj<C>> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// A +1 object result from the `alloc`, `new`, `copy`, or `mutableCopy`
/// family.
impl<C: Receiver> Ret for Option<Retained<C>> {
    type Raw<'a> = Option<Retained<C>>;
    type Out<'a> = Option<Retained<C>>;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Owned;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}
impl<C: Receiver> ClassRet for Option<Retained<C>> {}

#[cfg(test)]
impl<C: Receiver> Encode for Option<Retained<C>> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// The return marker of an autoreleased object result. The send retains
/// it, so the result does not depend on the current autorelease pool.
pub(crate) struct Autoreleased<C>(PhantomData<fn() -> C>);

impl<C: Receiver> Ret for Autoreleased<C> {
    type Raw<'a> = Option<&'a Obj<C>>;
    type Out<'a> = Option<Retained<C>>;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Retains;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw.map(Obj::retain)
    }
}
impl<C: Receiver> ClassRet for Autoreleased<C> {}

#[cfg(test)]
impl<C: Receiver> Encode for Autoreleased<C> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// The return marker of an object that lives for the whole process, such
/// as `+[NSApplication sharedApplication]`.
pub(crate) struct Shared<C>(PhantomData<fn() -> C>);

impl<C: Receiver> Ret for Shared<C> {
    type Raw<'a> = Option<&'static Obj<C>>;
    type Out<'a> = Option<&'static Obj<C>>;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Shared;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}
impl<C: Receiver> ClassRet for Shared<C> {}

#[cfg(test)]
impl<C: Receiver> Encode for Shared<C> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

impl Arg for &'static AnyClass {
    type In<'a> = &'static AnyClass;
    type Raw<'a> = &'static AnyClass;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

/// A NUL-terminated string argument.
impl Arg for &'static CStr {
    type In<'a> = &'a CStr;
    type Raw<'a> = *const c_char;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value.as_ptr()
    }
}

#[cfg(test)]
impl Encode for &CStr {
    fn encode(out: &mut String) {
        out.push('*');
    }
}

/// The return marker of a borrowed NUL-terminated UTF-8 buffer, such as
/// the result of `-[NSString UTF8String]`. A descriptor with this return
/// stays private to `ffi`.
pub(crate) struct Utf8;

impl Ret for Utf8 {
    type Raw<'a> = *const c_char;
    type Out<'a> = *const c_char;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Value;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}

#[cfg(test)]
impl Encode for Utf8 {
    fn encode(out: &mut String) {
        out.push('*');
    }
}

impl Arg for Sel {
    type In<'a> = Sel;
    type Raw<'a> = Sel;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

impl Arg for Option<Sel> {
    type In<'a> = Option<Sel>;
    type Raw<'a> = Option<Sel>;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

#[cfg(test)]
impl Encode for Option<Sel> {
    fn encode(out: &mut String) {
        out.push(':');
    }
}

/// A pointer to `count` values that a helper in `ffi` passes with its
/// length. A descriptor with this argument stays private to `ffi`.
impl<T: 'static> Arg for *const T {
    type In<'a> = *const T;
    type Raw<'a> = *const T;
    fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
        value
    }
}

/// A pointer result that a helper in `ffi` reads before it returns.
impl<T: 'static> Ret for *const T {
    type Raw<'a> = *const T;
    type Out<'a> = *const T;
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Value;
    fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
        raw
    }
}

#[cfg(test)]
impl<T: Encode> Encode for *const T {
    fn encode(out: &mut String) {
        out.push('^');
        T::encode(out);
    }
}

/// An argument tuple of a message descriptor.
pub(crate) trait Args: 'static {
    /// The tuple that a send takes.
    type In<'a>;
    /// Appends the encodings of the arguments.
    #[cfg(test)]
    fn encode(out: &mut String);
    /// Calls the message function with the ABI types of the descriptor.
    fn invoke<'a, Recv: Copy, R: Ret>(
        receiver: Recv,
        selector: Sel,
        args: Self::In<'a>,
    ) -> R::Raw<'a>;
}

/// True when a return value of type `T` needs the hidden struct-return
/// pointer of `objc_msgSend_stret`: on x86-64, a struct larger than 16
/// bytes. This is the only place that chooses the `_stret` dispatcher.
const fn struct_return<T>() -> bool {
    cfg!(target_arch = "x86_64") && std::mem::size_of::<T>() > 16
}

/// Implements [`Args`] for a tuple of [`Arg`] types.
macro_rules! args_tuple {
    ($($arg:ident $index:tt),*) => {
        impl<$($arg: Arg + ArgEncode),*> Args for ($($arg,)*) {
            type In<'a> = ($($arg::In<'a>,)*);

            #[cfg(test)]
            fn encode(_out: &mut String) {
                $(<$arg as Encode>::encode(_out);)*
            }


            fn invoke<'a, Recv: Copy, R: Ret>(
                receiver: Recv,
                selector: Sel,
                _args: Self::In<'a>,
            ) -> R::Raw<'a> {
                if struct_return::<R::Raw<'a>>() {
                    let mut result = std::mem::MaybeUninit::<R::Raw<'a>>::uninit();
                    let send: extern "C-unwind" fn(
                        *mut R::Raw<'a>,
                        Recv,
                        Sel
                        $(, $arg::Raw<'a>)*
                    ) =
                        // SAFETY: `objc_msgSend_stret` takes the result
                        // pointer, the receiver, the selector, and the
                        // arguments with the ABI types of the descriptor,
                        // which the runtime check compares with the SDK.
                        unsafe { std::mem::transmute(runtime::msg_send(true)) };
                    send(result.as_mut_ptr(), receiver, selector $(, $arg::raw(_args.$index))*);
                    // SAFETY: the dispatcher wrote the whole struct.
                    return unsafe { result.assume_init() };
                }
                let send: extern "C-unwind" fn(Recv, Sel $(, $arg::Raw<'a>)*) -> R::Raw<'a> =
                    // SAFETY: `objc_msgSend` takes the receiver, the
                    // selector, and the arguments with the ABI types of the
                    // descriptor, which the runtime check compares with the
                    // SDK.
                    unsafe { std::mem::transmute(runtime::msg_send(false)) };
                send(receiver, selector $(, $arg::raw(_args.$index))*)
            }
        }
    };
}

args_tuple!();
args_tuple!(A0 0);
args_tuple!(A0 0, A1 1);
args_tuple!(A0 0, A1 1, A2 2);
args_tuple!(A0 0, A1 1, A2 2, A3 3);
args_tuple!(A0 0, A1 1, A2 2, A3 3, A4 4);
args_tuple!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5);
args_tuple!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6);
args_tuple!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6, A7 7);
args_tuple!(A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6, A7 7, A8 8);

/// Declares message descriptors and, for tests, the table of all of them:
///
/// ```ignore
/// messages! {
///     pub(crate) SET_TITLE: instance NSWindow, c"setTitle:", (&Obj<NSString>) -> ();
/// }
/// ```
macro_rules! messages {
    ($(
        $(#[$meta:meta])*
        $vis:vis $name:ident : $kind:ident $receiver:ty, $selector:literal,
        ($($arg:ty),* $(,)?) -> $ret:ty;
    )*) => {
        $(
            $(#[$meta])*
            $vis const $name: $crate::ffi::msg::Msg<$receiver, ($($arg,)*), $ret> =
                $crate::ffi::msg::Msg::$kind($selector);
        )*

        /// Every message descriptor of this module, for the checks.
        #[cfg(test)]
        pub(crate) const MESSAGES: &[$crate::ffi::checks::MsgInfo] = &[$($name.info(),)*];
    };
}
pub(crate) use messages;

#[cfg(test)]
impl<C: Receiver, A: Args, R: Encode + MsgOwnership> Msg<C, A, R> {
    /// The descriptor data that the checks read.
    pub(crate) const fn info(&self) -> super::checks::MsgInfo {
        super::checks::MsgInfo {
            receiver: C::NAME,
            receiver_kind: C::KIND,
            kind: self.kind,
            selector: self.selector,
            ownership: R::OWNERSHIP,
            encoding: Self::encoding,
        }
    }

    fn encoding() -> String {
        let mut out = String::new();
        R::encode(&mut out);
        out.push_str("@:");
        A::encode(&mut out);
        out
    }
}

/// The ownership of a return marker, including `Init`.
#[cfg(test)]
pub(crate) trait MsgOwnership {
    #[cfg(test)]
    const OWNERSHIP: Ownership;
}

#[cfg(test)]
impl<R: Ret> MsgOwnership for R {
    #[cfg(test)]
    const OWNERSHIP: Ownership = <R as Ret>::OWNERSHIP;
}

#[cfg(test)]
impl<X> MsgOwnership for Init<X> {
    #[cfg(test)]
    const OWNERSHIP: Ownership = Ownership::Owned;
}
