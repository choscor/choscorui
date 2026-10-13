//! Owned Objective-C references and the autorelease pool guard.

use super::class::{ClassType, IsA, Obj};
use super::foundation::{ALLOC, DRAIN, INIT, NSAutoreleasePool, NSObject, RELEASE, RETAIN};
use crate::Error;
use std::ptr::NonNull;

/// One +1 reference to an Objective-C object. `Drop` sends `release` and
/// `Clone` sends `retain`.
#[repr(transparent)]
pub(crate) struct Retained<C> {
    pointer: NonNull<Obj<C>>,
}

impl<C> Retained<C> {
    /// Gives up the reference without a release. The caller in `ffi` passes
    /// it to a method that consumes it, such as an `init` message.
    pub(in crate::ffi) fn into_raw(self) -> NonNull<Obj<C>> {
        let pointer = self.pointer;
        std::mem::forget(self);
        pointer
    }

    /// The same reference as an instance of the superclass `S`.
    pub(crate) fn into_super<S: ClassType>(self) -> Retained<S>
    where
        C: IsA<S>,
    {
        self.cast_owned()
    }

    /// Reinterprets the reference as another class. The caller in `ffi`
    /// knows that the object has that type.
    pub(in crate::ffi) fn cast_owned<D>(self) -> Retained<D> {
        Retained {
            pointer: self.into_raw().cast(),
        }
    }
}

impl<C> std::ops::Deref for Retained<C> {
    type Target = Obj<C>;

    fn deref(&self) -> &Obj<C> {
        // SAFETY: the handle owns a +1 reference, so the object is alive
        // while the handle is borrowed.
        unsafe { self.pointer.as_ref() }
    }
}

impl<C> Drop for Retained<C> {
    fn drop(&mut self) {
        RELEASE.send(Obj::cast(self), ());
    }
}

impl<C> Clone for Retained<C> {
    fn clone(&self) -> Self {
        Obj::retain(self)
    }
}

impl<C> Obj<C> {
    /// A new +1 reference to this object.
    pub(crate) fn retain(&self) -> Retained<C> {
        RETAIN.send(self.cast(), ());
        Retained {
            pointer: NonNull::from(self),
        }
    }
}

/// A +1 instance of `S` from `+alloc`. Initialize it with an `init` message.
pub(crate) fn alloc<S: ClassType + IsA<NSObject>>() -> Result<Retained<S>, Error> {
    ALLOC
        .class_send_to(S::class()?, ())?
        .map(Retained::cast_owned)
        .ok_or(Error::AppKitUnavailable)
}

/// A +1 instance of `S` from `+alloc` and `-init`.
pub(crate) fn alloc_init<S: ClassType + IsA<NSObject>>() -> Result<Retained<S>, Error> {
    INIT.init(alloc::<S>()?, ()).ok_or(Error::AppKitUnavailable)
}

/// An object that lives for the whole process, such as a delegate that
/// AppKit holds weakly. Only the main thread borrows it.
pub(crate) struct ProcessObject<C> {
    pointer: NonNull<Obj<C>>,
}

// SAFETY: `get` borrows the object only on the main thread, and nothing
// releases the object.
unsafe impl<C> Send for ProcessObject<C> {}
// SAFETY: as above.
unsafe impl<C> Sync for ProcessObject<C> {}

impl<C> ProcessObject<C> {
    /// Keeps the +1 reference of `object` until the process exits.
    pub(crate) fn new(object: Retained<C>) -> Self {
        Self {
            pointer: object.into_raw(),
        }
    }

    /// The object, or `None` off the main thread.
    pub(crate) fn get(&self) -> Option<&'static Obj<C>> {
        if !super::is_main_thread() {
            return None;
        }
        // SAFETY: the +1 reference is never released, so the object lives
        // until the process exits.
        Some(unsafe { self.pointer.as_ref() })
    }
}

/// An `NSAutoreleasePool` for the current scope. Objects autoreleased
/// while it lives are released when it drops. Create and drop it on one
/// thread.
pub(crate) struct Pool {
    pool: NonNull<Obj<NSAutoreleasePool>>,
}

impl Pool {
    pub(crate) fn new() -> Result<Self, Error> {
        let pool = alloc_init::<NSAutoreleasePool>()?;
        Ok(Self {
            pool: pool.into_raw(),
        })
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // `drain` releases the pool, so it consumes the +1 reference.
        let pool = Retained { pointer: self.pool };
        DRAIN.send(&pool, ());
        std::mem::forget(pool);
    }
}
