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

/// Borrows a raw non-null `id` as a typed handle.
///
/// # Safety
///
/// `raw` is an object of class `C` that stays alive for `'a`.
pub(crate) unsafe fn borrow_non_null<'a, C>(raw: NonNull<c_void>) -> &'a Obj<C> {
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

/// Borrows a raw Core Foundation reference, or `None` for null.
///
/// # Safety
///
/// `raw` is null or an object of type `T` that stays alive for `'a`.
pub(crate) unsafe fn cf<'a, T: super::cf::CfType>(raw: *mut c_void) -> Option<&'a T> {
    // SAFETY: the caller guarantees the type and the lifetime.
    unsafe { raw.cast::<T>().as_ref() }
}

/// Releases a raw +1 Core Foundation reference.
///
/// # Safety
///
/// `raw` is a +1 reference to a live Core Foundation object that the
/// caller gives up.
pub(crate) unsafe fn cf_release(raw: *mut c_void) {
    if let Some(raw) = NonNull::new(raw.cast::<super::cf::CFType>()) {
        // SAFETY: the caller gives up its +1 reference.
        drop(unsafe { super::cf::CfRetained::from_raw(raw) });
    }
}
