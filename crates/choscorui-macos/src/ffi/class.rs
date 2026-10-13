//! Typed Objective-C object handles, class and protocol markers, and
//! selectors.
//!
//! `objc_class!(NSWindow: NSResponder: NSObject)` declares a marker type
//! with its superclass chain. `&Obj<NSWindow>` converts to a handle of any
//! superclass through `Deref`. A conversion to a subclass is checked with
//! `isKindOfClass:` and returns `Option`.

#[cfg(test)]
use super::encode::Encode;
use super::runtime;
use crate::Error;
use std::ffi::{CStr, c_void};
use std::marker::{PhantomData, PhantomPinned};
use std::ptr::NonNull;

/// A borrowed Objective-C object whose class is `C` or a subclass of `C`.
///
/// Only references exist: `&Obj<C>` is the object pointer, so a borrow
/// cannot outlive its owner. The marker fields make the handle `!Send`,
/// `!Sync`, and `!Unpin`, because AppKit objects belong to the main thread.
#[repr(C)]
pub(crate) struct Obj<C> {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut C, PhantomPinned)>,
}

/// A class object, the receiver of class messages.
#[repr(C)]
pub(crate) struct AnyClass {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// What a message descriptor names as its receiver.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReceiverKind {
    Class,
}

/// A class or protocol marker that can receive messages.
pub(crate) trait Receiver: 'static {
    /// The runtime name, for example `NSWindow`.
    const NAME: &'static CStr;
    /// Whether `NAME` is a class or a protocol.
    #[cfg(test)]
    const KIND: ReceiverKind;
}

/// A class marker. The class object exists in the runtime after its
/// framework loads or after the crate registers it.
pub(crate) trait ClassType: Receiver {
    /// The class object.
    fn class() -> Result<&'static AnyClass, Error> {
        lookup_class(Self::NAME).ok_or(Error::AppKitUnavailable)
    }
}

/// Implemented for `C` and for each superclass of `C`.
pub(crate) trait IsA<S: ClassType>: ClassType {}

/// The runtime class with `name`, or `None` when no loaded image defines it.
pub(crate) fn lookup_class(name: &CStr) -> Option<&'static AnyClass> {
    runtime::get_class(name)
}

impl<C> Obj<C> {
    /// This object as an instance of its superclass `S`.
    pub(crate) fn upcast<S: ClassType>(&self) -> &Obj<S>
    where
        C: IsA<S>,
    {
        self.cast()
    }

    /// The object address, a key for the tables that map AppKit objects to
    /// Rust state. It is unique while the object lives.
    pub(crate) fn addr(&self) -> usize {
        std::ptr::from_ref(self).addr()
    }

    /// Reinterprets the handle as another class or protocol. The caller
    /// inside `ffi` knows that the object has that type.
    pub(in crate::ffi) fn cast<D>(&self) -> &Obj<D> {
        let pointer = NonNull::from(self).cast::<Obj<D>>();
        // SAFETY: `Obj` is a zero-sized opaque handle, so the cast keeps
        // the same object pointer and the same borrow.
        unsafe { pointer.as_ref() }
    }
}

#[cfg(test)]
impl<C> Encode for &Obj<C>
where
    C: Receiver,
{
    fn encode(out: &mut String) {
        out.push('@');
    }
}

/// A registered selector.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Sel(NonNull<c_void>);

impl Sel {
    /// Registers `name` with the runtime, or finds it when it exists.
    pub(crate) fn register(name: &CStr) -> Self {
        runtime::register_selector(name)
    }

    /// The raw selector, for the modules that still send messages through
    /// their own casts.
    pub(crate) fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }
}

/// Declares a class marker with its superclass chain, for example
/// `objc_class!(NSWindow: NSResponder: NSObject)`. A marker whose Rust name
/// differs from the runtime name gives it as a string, for example
/// `objc_class!(NsUrl = "NSURL": NSObject)`.
macro_rules! objc_class {
    ($(#[$meta:meta])* $name:ident $(= $runtime:literal)?) => {
        $(#[$meta])*
        pub(crate) enum $name {}
        $crate::ffi::class::objc_class!(@receiver $name, $crate::ffi::class::objc_class!(@runtime $name $($runtime)?));
        impl $crate::ffi::class::IsA<$name> for $name {}
    };
    ($(#[$meta:meta])* $name:ident $(= $runtime:literal)? : $superclass:ident $(: $ancestor:ident)*) => {
        $(#[$meta])*
        pub(crate) enum $name {}
        $crate::ffi::class::objc_class!(@receiver $name, $crate::ffi::class::objc_class!(@runtime $name $($runtime)?));
        $crate::ffi::class::objc_class!(@chain $name: $superclass $(: $ancestor)*);
    };
    (@runtime $name:ident) => {
        stringify!($name)
    };
    (@runtime $name:ident $runtime:literal) => {
        $runtime
    };
    (@receiver $name:ident, $runtime:expr) => {
        impl $crate::ffi::class::Receiver for $name {
            const NAME: &'static ::std::ffi::CStr =
                match ::std::ffi::CStr::from_bytes_with_nul(concat!($runtime, "\0").as_bytes()) {
                    Ok(name) => name,
                    Err(_) => panic!("class names contain no NUL"),
                };
            #[cfg(test)]
            const KIND: $crate::ffi::class::ReceiverKind = $crate::ffi::class::ReceiverKind::Class;
        }
        impl $crate::ffi::class::ClassType for $name {}
    };
    (@chain $name:ident : $superclass:ident $(: $ancestor:ident)*) => {
        impl $crate::ffi::class::IsA<$name> for $name {}
        impl $crate::ffi::class::IsA<$superclass> for $name {}
        $(impl $crate::ffi::class::IsA<$ancestor> for $name {})*
        impl ::std::ops::Deref for $crate::ffi::class::Obj<$name> {
            type Target = $crate::ffi::class::Obj<$superclass>;
            fn deref(&self) -> &Self::Target {
                self.cast()
            }
        }
    };
}
pub(crate) use objc_class;

#[cfg(test)]
impl Encode for &'static AnyClass {
    fn encode(out: &mut String) {
        out.push('#');
    }
}

#[cfg(test)]
impl Encode for Sel {
    fn encode(out: &mut String) {
        out.push(':');
    }
}
