//! Raw declarations of the Objective-C runtime (`objc/runtime.h` and
//! `objc/message.h`) and their safe wrappers.
//!
//! The message functions are `C-unwind`: an Objective-C exception that
//! leaves a message send unwinds through Rust as a defined foreign unwind.

use super::class::{AnyClass, AnyProtocol, Sel};
use std::ffi::{CStr, c_char};

#[link(name = "objc")]
unsafe extern "C-unwind" {
    fn objc_getClass(name: *const c_char) -> Option<&'static AnyClass>;
    fn objc_getProtocol(name: *const c_char) -> Option<&'static AnyProtocol>;
    fn sel_registerName(name: *const c_char) -> Sel;
    pub(crate) fn objc_msgSend();
    fn objc_msgSendSuper();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSendSuper_stret();
}

/// The runtime class with `name`.
pub(super) fn get_class(name: &CStr) -> Option<&'static AnyClass> {
    // SAFETY: `name` is a NUL-terminated string that lives for the call.
    unsafe { objc_getClass(name.as_ptr()) }
}

/// The runtime protocol with `name`. A protocol exists when a loaded
/// image uses it.
pub(super) fn get_protocol(name: &CStr) -> Option<&'static AnyProtocol> {
    // SAFETY: `name` is a NUL-terminated string that lives for the call.
    unsafe { objc_getProtocol(name.as_ptr()) }
}

/// The selector with `name`, registered when it is new.
pub(super) fn register_selector(name: &CStr) -> Sel {
    // SAFETY: `name` is a NUL-terminated string that lives for the call;
    // the runtime copies it and never returns null.
    unsafe { sel_registerName(name.as_ptr()) }
}

/// The address of `objc_msgSend`, or of `objc_msgSendSuper` for a send to
/// the superclass implementation. On x86-64 a struct return larger than 16
/// bytes uses the `_stret` variant; arm64 has no such variant.
pub(super) fn msg_send(to_super: bool, struct_return: bool) -> *const () {
    #[cfg(target_arch = "x86_64")]
    if struct_return {
        return if to_super {
            objc_msgSendSuper_stret as *const ()
        } else {
            objc_msgSend_stret as *const ()
        };
    }
    let _ = struct_return;
    if to_super {
        objc_msgSendSuper as *const ()
    } else {
        objc_msgSend as *const ()
    }
}

/// An opaque `Method` of the runtime.
#[cfg(test)]
#[repr(C)]
struct Method {
    _opaque: [u8; 0],
}

#[cfg(test)]
#[link(name = "objc")]
unsafe extern "C-unwind" {
    safe fn class_getInstanceMethod(class: &AnyClass, name: Sel) -> Option<&'static Method>;
    safe fn class_getClassMethod(class: &AnyClass, name: Sel) -> Option<&'static Method>;
    safe fn method_getTypeEncoding(method: &Method) -> *const c_char;
    safe fn protocol_getMethodDescription(
        protocol: &AnyProtocol,
        name: Sel,
        required: bool,
        instance: bool,
    ) -> MethodDescription;
}

/// `struct objc_method_description`; both fields are null when the
/// protocol has no such method.
#[cfg(test)]
#[repr(C)]
struct MethodDescription {
    name: Option<Sel>,
    types: *const c_char,
}

/// Copies a type encoding that the runtime owns.
#[cfg(test)]
fn copy_types(types: *const c_char) -> Option<String> {
    if types.is_null() {
        return None;
    }
    // SAFETY: the runtime keeps method type strings for the life of the
    // process; the pointer is not null.
    let types = unsafe { CStr::from_ptr(types) };
    Some(types.to_string_lossy().into_owned())
}

/// The type encoding of a method that `class` or a superclass declares.
#[cfg(test)]
pub(super) fn method_types(class: &AnyClass, name: Sel, instance: bool) -> Option<String> {
    let method = if instance {
        class_getInstanceMethod(class, name)
    } else {
        class_getClassMethod(class, name)
    }?;
    copy_types(method_getTypeEncoding(method))
}

/// The type encoding of a required or optional method of `protocol` or of
/// a protocol that it incorporates.
#[cfg(test)]
pub(super) fn protocol_method_types(
    protocol: &AnyProtocol,
    name: Sel,
    instance: bool,
) -> Option<String> {
    [true, false].into_iter().find_map(|required| {
        let description = protocol_getMethodDescription(protocol, name, required, instance);
        description.name.and_then(|_| copy_types(description.types))
    })
}
