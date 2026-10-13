//! Temporary bridges between typed handles and the raw `id` pointers of
//! the modules that a later phase of the FFI migration converts. The
//! migration deletes this module with its last caller.

use super::class::Obj;
use std::ffi::c_void;
use std::ptr::NonNull;

/// The raw `id` of `object`.
pub(crate) fn id<C>(object: &Obj<C>) -> *mut c_void {
    NonNull::from(object).as_ptr().cast()
}

/// Borrows a raw `id` as a typed handle, or `None` for nil.
///
/// # Safety
///
/// `raw` is nil or an object of class `C` that stays alive for `'a`.
pub(crate) unsafe fn borrow<'a, C>(raw: *mut c_void) -> Option<&'a Obj<C>> {
    // SAFETY: the caller guarantees the class and the lifetime.
    unsafe { raw.cast::<Obj<C>>().as_ref() }
}

/// Borrows a raw block pointer, or `None` for nil.
///
/// # Safety
///
/// `raw` is nil or a block that takes `A` and stays alive for `'a`.
pub(crate) unsafe fn block<'a, A>(raw: *mut c_void) -> Option<&'a super::block::Block<A>> {
    // SAFETY: the caller guarantees the signature and the lifetime.
    unsafe { raw.cast::<super::block::Block<A>>().as_ref() }
}

/// Gives a +1 object to the current autorelease pool and returns it as the
/// +0 raw result of a callback.
pub(crate) fn autoreleased<C>(object: super::retained::Retained<C>) -> *mut c_void {
    let raw = id(&object);
    super::foundation::AUTORELEASE.send(object.cast(), ());
    std::mem::forget(object);
    raw
}
