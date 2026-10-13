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

/// A copy of the text of a raw `NSString`, or `None` for nil.
///
/// # Safety
///
/// `raw` is nil or a live `NSString`.
pub(crate) unsafe fn string(raw: *mut c_void) -> Option<String> {
    // SAFETY: the caller guarantees that `raw` is nil or a live NSString.
    let value = unsafe { borrow::<super::foundation::NSString>(raw) }?;
    super::foundation::string(value)
}
