//! Callbacks: the Objective-C classes that the crate defines, and the
//! trampolines of their methods.
//!
//! `define_classes!` declares each class with its superclass, the
//! protocols that it adopts, and its methods. A method names its origin,
//! its selector, its argument and return types, a fallback, and a Rust
//! handler. The macro generates an `extern "C"` trampoline for each method
//! and derives the type encoding from the same types, so no type string is
//! written by hand. Each trampoline catches a panic of its handler,
//! reports it on standard error, and returns the fallback.

use super::class::{AnyClass, Obj, Sel, lookup_class};
use super::encode::{Bool, Encode};
use super::foundation::{AUTORELEASE, NSObject};
use super::geometry::{NSPoint, NSRange, NSRect};
use super::retained::Retained;
use super::runtime;
use crate::Error;
use std::ffi::{CStr, CString};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::OnceLock;

/// Where the selector of a method is declared. The callback check compares
/// each method with its origin.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Origin {
    /// A method of the superclass, or of one of its ancestors.
    Super(&'static CStr),
    /// A required or optional method of a protocol.
    Protocol(&'static CStr),
    /// A selector that the crate invents. It must not exist in the
    /// superclass or in the adopted protocols.
    Own,
}

/// A function that appends the type encoding of a method.
pub(crate) type EncodeFn = fn(&mut String);

/// One method of a class definition. Only `define_classes!` makes one, so
/// `imp` always matches `encode`.
pub(crate) struct Method {
    pub(in crate::ffi) selector: &'static CStr,
    pub(in crate::ffi) imp: *const (),
    pub(in crate::ffi) encode: EncodeFn,
    #[cfg(test)]
    pub(in crate::ffi) origin: Origin,
}

impl Method {
    /// The type encoding: the return type, the receiver, the selector, and
    /// the arguments.
    pub(in crate::ffi) fn encoding(&self) -> String {
        let mut out = String::new();
        (self.encode)(&mut out);
        out
    }

    /// True for a selector that the crate invents.
    #[cfg(test)]
    pub(in crate::ffi) fn is_own(&self) -> bool {
        matches!(self.origin, Origin::Own)
    }
}

/// The argument types of a method, as a tuple.
pub(crate) trait MethodArgs {
    fn encode(out: &mut String);
}

/// Implements [`MethodArgs`] for a tuple of [`Encode`] types.
macro_rules! method_args {
    ($($arg:ident),*) => {
        impl<$($arg: Encode),*> MethodArgs for ($($arg,)*) {
            fn encode(_out: &mut String) {
                $($arg::encode(_out);)*
            }
        }
    };
}

method_args!();
method_args!(A0);
method_args!(A0, A1);
method_args!(A0, A1, A2);

/// Appends the encoding of a method that returns `R` and takes `A`. Methods
/// with one signature share one instance.
pub(in crate::ffi) fn method_encoding<R: Encode, A: MethodArgs>(out: &mut String) {
    R::encode(out);
    out.push_str("@:");
    A::encode(out);
}

/// A class that the crate defines. The tables are constants, so they cost
/// no code.
pub(crate) struct ClassDef {
    pub(in crate::ffi) name: &'static CStr,
    pub(in crate::ffi) superclass: &'static CStr,
    pub(in crate::ffi) protocols: &'static [&'static CStr],
    pub(in crate::ffi) methods: &'static [Method],
}

/// A class of a `define_classes!` table, for the checks.
#[cfg(test)]
pub(crate) struct ClassEntry {
    pub(crate) register: fn() -> Result<(), Error>,
    pub(crate) definition: fn() -> ClassDef,
}

/// Creates and registers `class`. A class with the same name must not
/// exist; then this is an error.
pub(in crate::ffi) fn define(class: ClassDef) -> Result<(), Error> {
    let superclass = lookup_class(class.superclass).ok_or(Error::AppKitUnavailable)?;
    let protocols = class
        .protocols
        .iter()
        .map(|name| runtime::get_protocol(name).ok_or(Error::AppKitUnavailable))
        .collect::<Result<Vec<_>, _>>()?;
    let new = runtime::allocate_class(superclass, class.name).ok_or(Error::AppKitUnavailable)?;
    let complete = protocols
        .into_iter()
        .all(|protocol| runtime::add_protocol(new, protocol))
        && class.methods.iter().all(|method| add_method(new, method));
    if !complete {
        // SAFETY: `new` is the class pair allocated above. It is not
        // registered, so it has no instances and no subclasses.
        unsafe { runtime::dispose_class(new) };
        return Err(Error::AppKitUnavailable);
    }
    runtime::register_class(new);
    Ok(())
}

/// Adds `method` to a class pair. The type string lives as long as the
/// class: the process.
fn add_method(class: &AnyClass, method: &Method) -> bool {
    let Ok(types) = CString::new(method.encoding()) else {
        return false;
    };
    let types: &'static CStr = Box::leak(types.into_boxed_c_str());
    // SAFETY: `define_classes!` builds `imp` and `encoding` from one list of
    // argument and return types, so the trampoline matches `types`.
    unsafe { runtime::add_method(class, Sel::register(method.selector), method.imp, types) }
}

/// The return type of a handler, and the ABI type that its trampoline
/// returns to the runtime.
pub(crate) trait CallbackRet {
    type Raw: Encode;
    fn into_raw(self) -> Self::Raw;
}

/// Implements [`CallbackRet`] for a type that crosses unchanged.
macro_rules! plain_return {
    ($($ty:ty),* $(,)?) => {$(
        impl CallbackRet for $ty {
            type Raw = $ty;
            fn into_raw(self) -> Self::Raw {
                self
            }
        }
    )*};
}

plain_return!((), usize, isize, NSPoint, NSRange, NSRect);

impl CallbackRet for bool {
    type Raw = Bool;
    fn into_raw(self) -> Self::Raw {
        Bool::new(self)
    }
}

/// A +1 object result. The trampoline autoreleases it and returns it at +0,
/// as the Cocoa naming rules require for a method that is not in the
/// `alloc`, `new`, `copy`, `mutableCopy`, or `init` family.
impl<C> CallbackRet for Option<Retained<C>> {
    type Raw = ReturnedObject<C>;
    fn into_raw(self) -> Self::Raw {
        ReturnedObject(self.map(|object| {
            AUTORELEASE.send(object.cast(), ());
            object.into_raw()
        }))
    }
}

/// An autoreleased object, or nil, that a trampoline returns.
#[repr(transparent)]
pub(crate) struct ReturnedObject<C>(Option<NonNull<Obj<C>>>);

impl<C> Encode for ReturnedObject<C> {
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// A pointer argument that can be null, such as the `actualRange` of the
/// text input methods.
impl<T: Encode> Encode for Option<&mut T> {
    fn encode(out: &mut String) {
        out.push('^');
        T::encode(out);
    }
}

/// Runs the handler of a trampoline. A panic does not cross into the
/// runtime: the panic hook prints it, this reports the method, and the
/// trampoline returns `fallback`.
///
/// The report reads the class and the selector from the runtime, so the
/// trampolines of one handler and signature are identical and the linker
/// can merge them.
pub(in crate::ffi) fn guard<C, R: CallbackRet>(
    this: &Obj<C>,
    selector: Sel,
    fallback: R,
    handler: impl FnOnce() -> R,
) -> R::Raw {
    match catch_unwind(AssertUnwindSafe(handler)) {
        Ok(result) => result.into_raw(),
        Err(_) => {
            report_method_panic(this.cast(), selector);
            fallback.into_raw()
        }
    }
}

/// Reports a panic of the handler of `selector` on `object`.
#[cold]
#[inline(never)]
fn report_method_panic(object: &Obj<NSObject>, selector: Sel) {
    report_panic(&format!(
        "-[{} {}]",
        runtime::class_name(object),
        runtime::selector_name(selector)
    ));
}

/// Defines a class one time and keeps the result in `cell`.
pub(in crate::ffi) fn register_once(
    cell: &OnceLock<Result<(), Error>>,
    definition: fn() -> ClassDef,
) -> Result<(), Error> {
    *cell.get_or_init(|| define(definition()))
}

/// Reports a panic that a callback caught. The panic hook has already
/// printed the panic message; this names the callback.
#[cold]
#[inline(never)]
pub(in crate::ffi) fn report_panic(callback: &str) {
    eprintln!("choscorui-macos: {callback} panicked; the callback returned its fallback");
}

/// Defines Objective-C classes and their methods:
///
/// ```ignore
/// define_classes! {
///     ChoscoruiV2WindowDelegate: NSObject, adopts [] {
///         Protocol(NSWindowDelegate) c"windowDidResize:"
///             (notification: Option<&Obj<NSNotification>>) => crate::window_did_resize;
///         Super(NSObject) c"isEqual:" (other: Option<&Obj<NSObject>>) -> bool = false
///             => crate::is_equal;
///         Own c"choscoruiAction:" (sender: Option<&Obj<NSObject>>) => crate::action;
///     }
/// }
/// ```
///
/// Each class marker gets `register()`, which defines the class one time,
/// and `definition()`. A handler takes `&Obj<Class>` and the arguments.
/// A method with a return type also declares the fallback that its
/// trampoline returns after a panic. In tests the macro also emits
/// `CLASSES`, the table that the checks read.
macro_rules! define_classes {
    ($(
        $class:ident : $superclass:ident, adopts [$($protocol:ident),* $(,)?] {
            $(
                $origin:ident $(($marker:ident))? $selector:literal
                ($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty = $fallback:expr)?
                => $handler:path;
            )*
        }
    )*) => {
        $(
            impl $class {
                /// Defines the class in the runtime on first use.
                pub(crate) fn register() -> Result<(), $crate::Error> {
                    static CLASS: ::std::sync::OnceLock<Result<(), $crate::Error>> =
                        ::std::sync::OnceLock::new();
                    $crate::ffi::callback::register_once(&CLASS, Self::definition)
                }

                /// The superclass, protocols, and methods of the class.
                pub(in crate::ffi) fn definition() -> $crate::ffi::callback::ClassDef {
                    $crate::ffi::callback::ClassDef {
                        name: <Self as $crate::ffi::class::Receiver>::NAME,
                        superclass: <$superclass as $crate::ffi::class::Receiver>::NAME,
                        protocols: &[$(<$protocol as $crate::ffi::class::Receiver>::NAME),*],
                        methods: const { &[$($crate::ffi::callback::Method {
                            selector: $selector,
                            imp: {
                                extern "C" fn trampoline(
                                    this: &$crate::ffi::class::Obj<$class>,
                                    selector: $crate::ffi::class::Sel,
                                    $($arg: $ty),*
                                ) -> <$crate::ffi::callback::define_classes!(@ret $($ret)?)
                                    as $crate::ffi::callback::CallbackRet>::Raw
                                {
                                    $crate::ffi::callback::guard(
                                        this,
                                        selector,
                                        $crate::ffi::callback::define_classes!(@fallback $($fallback)?),
                                        || $handler(this $(, $arg)*),
                                    )
                                }
                                trampoline as *const ()
                            },
                            encode: $crate::ffi::callback::method_encoding::<
                                <$crate::ffi::callback::define_classes!(@ret $($ret)?)
                                    as $crate::ffi::callback::CallbackRet>::Raw,
                                ($($ty,)*),
                            >,
                            #[cfg(test)]
                            origin: $crate::ffi::callback::define_classes!(@origin $origin $(($marker))?),
                        }),*] },
                    }
                }
            }
        )*

        /// Every class of this table, for the checks.
        #[cfg(test)]
        pub(crate) const CLASSES: &[$crate::ffi::callback::ClassEntry] = &[$(
            $crate::ffi::callback::ClassEntry {
                register: $class::register,
                definition: $class::definition,
            }
        ),*];
    };
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };
    (@fallback) => { () };
    (@fallback $fallback:expr) => { $fallback };
    (@origin Super($marker:ident)) => {
        $crate::ffi::callback::Origin::Super(<$marker as $crate::ffi::class::Receiver>::NAME)
    };
    (@origin Protocol($marker:ident)) => {
        $crate::ffi::callback::Origin::Protocol(<$marker as $crate::ffi::class::Receiver>::NAME)
    };
    (@origin Own) => {
        $crate::ffi::callback::Origin::Own
    };
}
pub(crate) use define_classes;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::class::objc_class;
    use crate::ffi::foundation::NSObject;
    use crate::ffi::msg::Msg;
    use crate::ffi::retained::alloc_init;
    use crate::ffi::{Obj, Pool};

    objc_class!(ChoscoruiV2PanickingTarget: NSObject);

    fn panicking_value(_: &Obj<ChoscoruiV2PanickingTarget>) -> isize {
        panic!("a deliberate panic in a callback");
    }

    fn panicking_flag(_: &Obj<ChoscoruiV2PanickingTarget>, _: Bool) -> bool {
        panic!("a deliberate panic in a callback");
    }

    define_classes! {
        ChoscoruiV2PanickingTarget: NSObject, adopts [] {
            Own c"choscoruiPanickingValue" () -> isize = -1 => panicking_value;
            Own c"choscoruiPanickingFlag:" (flag: Bool) -> bool = true => panicking_flag;
        }
    }

    const PANICKING_VALUE: Msg<ChoscoruiV2PanickingTarget, (), isize> =
        Msg::instance(c"choscoruiPanickingValue");
    const PANICKING_FLAG: Msg<ChoscoruiV2PanickingTarget, (bool,), bool> =
        Msg::instance(c"choscoruiPanickingFlag:");

    #[test]
    fn a_panicking_callback_returns_its_fallback() {
        let _pool = Pool::new().unwrap();
        for class in CLASSES {
            (class.register)().unwrap();
        }
        let target = alloc_init::<ChoscoruiV2PanickingTarget>().unwrap();
        assert_eq!(PANICKING_VALUE.send(&target, ()), -1);
        assert!(PANICKING_FLAG.send(&target, (false,)));
    }

    #[test]
    fn encodings_come_from_the_rust_types() {
        let definition = ChoscoruiV2PanickingTarget::definition();
        let encodings: Vec<String> = definition.methods.iter().map(Method::encoding).collect();
        let flag = if cfg!(target_arch = "aarch64") {
            "B@:B"
        } else {
            "c@:c"
        };
        assert_eq!(encodings, ["q@:", flag]);
        assert_eq!(definition.superclass, c"NSObject");
    }
}
