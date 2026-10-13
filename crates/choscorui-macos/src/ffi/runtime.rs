//! Raw declarations of the Objective-C runtime (`objc/runtime.h` and
//! `objc/message.h`) and their safe wrappers.
//!
//! The message functions are `C-unwind`: an Objective-C exception that
//! leaves a message send unwinds through Rust as a defined foreign unwind.

use super::class::{AnyClass, AnyProtocol, Obj, Sel};
use super::encode::Bool;
use super::foundation::NSObject;
use std::ffi::{CStr, c_char};

#[link(name = "objc")]
unsafe extern "C-unwind" {
    fn objc_getClass(name: *const c_char) -> Option<&'static AnyClass>;
    fn objc_getProtocol(name: *const c_char) -> Option<&'static AnyProtocol>;
    fn sel_registerName(name: *const c_char) -> Sel;
    safe fn sel_getName(name: Sel) -> *const c_char;
    safe fn object_getClassName(object: &Obj<NSObject>) -> *const c_char;
    fn objc_allocateClassPair(
        superclass: &AnyClass,
        name: *const c_char,
        extra_bytes: usize,
    ) -> Option<&'static AnyClass>;
    safe fn objc_registerClassPair(class: &AnyClass);
    fn objc_disposeClassPair(class: &AnyClass);
    fn class_addMethod(class: &AnyClass, name: Sel, imp: *const (), types: *const c_char) -> Bool;
    safe fn class_addProtocol(class: &AnyClass, protocol: &AnyProtocol) -> Bool;
    fn objc_msgSend();
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

/// A copy of a name that the runtime keeps for the life of the process.
fn runtime_name(name: *const c_char) -> String {
    if name.is_null() {
        return String::new();
    }
    // SAFETY: the runtime returns a NUL-terminated name that it never frees;
    // the pointer is not null.
    let name = unsafe { CStr::from_ptr(name) };
    name.to_string_lossy().into_owned()
}

/// The name of the class of `object`.
pub(super) fn class_name(object: &Obj<NSObject>) -> String {
    runtime_name(object_getClassName(object))
}

/// The name of `selector`.
pub(super) fn selector_name(selector: Sel) -> String {
    runtime_name(sel_getName(selector))
}

/// A new class pair named `name` with `superclass`, which the caller
/// completes with methods and then registers or disposes. `None` when a
/// class with `name` exists.
pub(super) fn allocate_class(superclass: &AnyClass, name: &CStr) -> Option<&'static AnyClass> {
    // SAFETY: `name` is a NUL-terminated string that lives for the call;
    // the runtime copies it.
    unsafe { objc_allocateClassPair(superclass, name.as_ptr(), 0) }
}

/// Adds `protocol` to a class that is not registered yet.
pub(super) fn add_protocol(class: &AnyClass, protocol: &AnyProtocol) -> bool {
    class_addProtocol(class, protocol).get()
}

/// Adds the method `name` to a class that is not registered yet.
///
/// # Safety
///
/// `imp` is an `extern "C"` function whose parameters and return type
/// match `types`: the receiver, the selector, and then the arguments.
/// `types` lives as long as the class.
pub(super) unsafe fn add_method(
    class: &AnyClass,
    name: Sel,
    imp: *const (),
    types: &'static CStr,
) -> bool {
    // SAFETY: the caller guarantees that `imp` matches `types`.
    unsafe { class_addMethod(class, name, imp, types.as_ptr()) }.get()
}

/// Registers a completed class pair.
pub(super) fn register_class(class: &AnyClass) {
    objc_registerClassPair(class);
}

/// Destroys a class pair.
///
/// # Safety
///
/// `class` is a class pair from [`allocate_class`] that is not registered,
/// so it has no instances and no subclasses. Nothing uses it afterward.
pub(super) unsafe fn dispose_class(class: &'static AnyClass) {
    // SAFETY: the caller guarantees an unused, unregistered class pair.
    unsafe { objc_disposeClassPair(class) };
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
