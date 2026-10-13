//! Core Foundation types, their ownership, and the Core Foundation
//! functions that the crate calls.
//!
//! A Core Foundation object is borrowed as `&T` and owned as
//! [`CfRetained<T>`]. A function named `*Create*` or `*Copy*` returns a
//! `CfRetained`; any other object result is borrowed from its argument.

use super::sdk::{c_functions, sdk_const};
use std::ffi::c_void;
use std::marker::{PhantomData, PhantomPinned};
use std::ops::Deref;
use std::ptr::NonNull;

/// A Core Foundation object type. Only `cf_type!` implements it.
pub(crate) trait CfType {}

/// Declares opaque Core Foundation object types. `Name: Parent` makes a
/// borrowed `Name` dereference to its parent; every other type
/// dereferences to [`CFType`].
macro_rules! cf_type {
    ($($(#[$meta:meta])* $name:ident $(: $parent:ident)?),* $(,)?) => {$(
        $(#[$meta])*
        #[repr(C)]
        pub(crate) struct $name {
            _opaque: [u8; 0],
            _marker: ::std::marker::PhantomData<(*mut u8, ::std::marker::PhantomPinned)>,
        }
        impl $crate::ffi::cf::CfType for $name {}
        $crate::ffi::cf::cf_type!(@parent $name $(: $parent)?);
        impl $crate::ffi::msg::Arg for &'static $name {
            type In<'a> = &'a $name;
            type Raw<'a> = &'a $name;
            fn raw<'a>(value: Self::In<'a>) -> Self::Raw<'a> {
                value
            }
        }
        /// A borrowed result that the receiver keeps alive.
        impl $crate::ffi::msg::Ret for Option<&'static $name> {
            type Raw<'a> = Option<&'a $name>;
            type Out<'a> = Option<&'a $name>;
            #[cfg(test)]
            const OWNERSHIP: $crate::ffi::msg::Ownership = $crate::ffi::msg::Ownership::Borrowed;
            fn out<'a>(raw: Self::Raw<'a>) -> Self::Out<'a> {
                raw
            }
        }
        impl $crate::ffi::encode::Encode for $name {
            fn encode(out: &mut String) {
                out.push_str(concat!("{", stringify!($name), "=}"));
            }
        }
        impl $crate::ffi::encode::Encode for &$name {
            fn encode(out: &mut String) {
                out.push('^');
                <$name as $crate::ffi::encode::Encode>::encode(out);
            }
        }
        impl $crate::ffi::encode::Encode for Option<&$name> {
            fn encode(out: &mut String) {
                <&$name as $crate::ffi::encode::Encode>::encode(out);
            }
        }
        #[cfg(test)]
        impl $crate::ffi::header::Header for &$name {
            fn header(out: &mut String) {
                out.push_str(concat!(stringify!($name), "Ref"));
            }
        }
        #[cfg(test)]
        impl $crate::ffi::header::Header for Option<&$name> {
            fn header(out: &mut String) {
                <&$name as $crate::ffi::header::Header>::header(out);
            }
        }
    )*};
    (@parent $name:ident) => {
        $crate::ffi::cf::cf_type!(@parent $name: $crate::ffi::cf::CFType);
    };
    (@parent $name:ident : $parent:path) => {
        impl ::std::ops::Deref for $name {
            type Target = $parent;
            fn deref(&self) -> &$parent {
                // SAFETY: both are opaque Core Foundation objects, and the
                // object is an instance of its parent type.
                unsafe { &*::std::ptr::from_ref(self).cast::<$parent>() }
            }
        }
    };
}
pub(crate) use cf_type;

/// Any Core Foundation object, `CFTypeRef`.
#[repr(C)]
pub(crate) struct CFType {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}
impl CfType for CFType {}

cf_type!(
    CFString,
    CFData,
    /// `CFMutableDataRef`.
    CFMutableData: CFData,
    CFDictionary,
    CFAttributedString,
    CFNumber,
    CFBoolean,
);

/// The marker of an opaque, unpinned, thread-bound object whose elements
/// are `T`.
type ArrayMarker<T> = (*mut u8, PhantomPinned, fn() -> T);

/// A `CFArrayRef` whose elements are `T`.
#[repr(C)]
pub(crate) struct CFArray<T> {
    _opaque: [u8; 0],
    _marker: PhantomData<ArrayMarker<T>>,
}
impl<T> CfType for CFArray<T> {}

impl<T> Deref for CFArray<T> {
    type Target = CFType;
    fn deref(&self) -> &CFType {
        // SAFETY: an array is a Core Foundation object.
        unsafe { &*std::ptr::from_ref(self).cast::<CFType>() }
    }
}

/// One +1 reference to a Core Foundation object, released in `Drop`.
#[repr(transparent)]
pub(crate) struct CfRetained<T: CfType>(NonNull<T>);

impl<T: CfType> CfRetained<T> {
    /// Takes another reference to `object`.
    pub(crate) fn retain(object: &T) -> Self {
        let raw = NonNull::from(object).cast::<c_void>();
        // SAFETY: `object` is a live Core Foundation object. CFRetain
        // returns its argument with one more reference.
        let retained = unsafe { CFRetain(raw) };
        Self(retained.cast())
    }

    /// Gives up the reference to the caller.
    pub(in crate::ffi) fn into_raw(self) -> NonNull<T> {
        let raw = self.0;
        std::mem::forget(self);
        raw
    }

    /// Takes over a +1 reference.
    ///
    /// # Safety
    ///
    /// `raw` is a +1 reference to a live object of type `T`.
    pub(in crate::ffi) unsafe fn from_raw(raw: NonNull<T>) -> Self {
        Self(raw)
    }
}

impl<T: CfType> Deref for CfRetained<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the reference keeps the object alive while `self` lives.
        unsafe { self.0.as_ref() }
    }
}

impl<T: CfType> Clone for CfRetained<T> {
    fn clone(&self) -> Self {
        Self::retain(self)
    }
}

impl<T: CfType> Drop for CfRetained<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one reference, released exactly once.
        unsafe { CFRelease(self.0.as_ptr().cast()) }
    }
}

impl CFType {
    /// The Core Foundation type identifier of the object.
    pub(crate) fn type_id(&self) -> usize {
        CFGetTypeID(self)
    }
}

sdk_const! {
    UTF8: u32 = 0x0800_0100, sdk = kCFStringEncodingUTF8;
    NUMBER_FLOAT64: isize = 6, sdk = kCFNumberFloat64Type;
}

/// `CFDictionaryKeyCallBacks`, read only by Core Foundation.
#[repr(C)]
pub(crate) struct CFDictionaryKeyCallBacks {
    _opaque: [u8; 0],
}
/// `CFDictionaryValueCallBacks`, read only by Core Foundation.
#[repr(C)]
pub(crate) struct CFDictionaryValueCallBacks {
    _opaque: [u8; 0],
}
/// `CFArrayCallBacks`, read only by Core Foundation.
#[repr(C)]
pub(crate) struct CFArrayCallBacks {
    _opaque: [u8; 0],
}

/// Implements [`CShape`](super::sdk::CShape) for the callback tables, which
/// Rust keeps opaque.
#[cfg(test)]
macro_rules! opaque_shape {
    ($($name:ident),*) => {$(
        impl super::sdk::CShape for $name {
            const SHAPE: super::sdk::Shape = super::sdk::Shape::Opaque(stringify!($name));
        }
    )*};
}

#[cfg(test)]
opaque_shape!(
    CFDictionaryKeyCallBacks,
    CFDictionaryValueCallBacks,
    CFArrayCallBacks
);

c_functions! {
    #[link(name = "CoreFoundation", kind = "framework")]
    statics {
        pub(crate) safe static kCFBooleanTrue: &'static CFBoolean;
        unsafe static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
        unsafe static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
        unsafe static kCFTypeArrayCallBacks: CFArrayCallBacks;
    }
    functions {
        unsafe fn CFRetain(object: NonNull<c_void>) -> NonNull<c_void>;
        unsafe fn CFRelease(object: *const c_void);
        safe fn CFGetTypeID(object: &CFType) -> usize;
        unsafe fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            external: u8,
        ) -> Option<CfRetained<CFString>>;
        safe fn CFStringGetLength(string: &CFString) -> isize;
        safe fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        unsafe fn CFStringGetCString(string: &CFString, buffer: *mut u8, size: isize, encoding: u32) -> u8;
        unsafe fn CFDataCreate(
            allocator: *const c_void,
            bytes: *const u8,
            length: isize,
        ) -> Option<CfRetained<CFData>>;
        unsafe fn CFDataCreateMutable(
            allocator: *const c_void,
            capacity: isize,
        ) -> Option<CfRetained<CFMutableData>>;
        safe fn CFDataGetLength(data: &CFData) -> isize;
        safe fn CFDataGetBytePtr(data: &CFData) -> *const u8;
        unsafe fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const &CFString,
            values: *const &CFType,
            count: isize,
            key_callbacks: *const CFDictionaryKeyCallBacks,
            value_callbacks: *const CFDictionaryValueCallBacks,
        ) -> Option<CfRetained<CFDictionary>>;
        unsafe fn CFDictionaryGetValue(dictionary: &CFDictionary, key: NonNull<c_void>) -> Option<&CFType>;
        unsafe fn CFArrayCreate(
            allocator: *const c_void,
            values: *const &CFType,
            count: isize,
            callbacks: *const CFArrayCallBacks,
        ) -> Option<CfRetained<CFArray<CFType>>>;
        unsafe fn CFArrayGetCount(array: NonNull<c_void>) -> isize;
        unsafe fn CFArrayGetValueAtIndex(array: NonNull<c_void>, index: isize) -> Option<NonNull<c_void>>;
        safe fn CFAttributedStringCreate(
            allocator: Option<&CFType>,
            string: &CFString,
            attributes: &CFDictionary,
        ) -> Option<CfRetained<CFAttributedString>>;
        unsafe fn CFNumberCreate(
            allocator: *const c_void,
            kind: isize,
            value: *const c_void,
        ) -> Option<CfRetained<CFNumber>>;
    }
}

/// A new `CFString` with the UTF-8 text of `text`.
pub(crate) fn cf_string(text: &str) -> Option<CfRetained<CFString>> {
    let length = isize::try_from(text.len()).ok()?;
    // SAFETY: the bytes and the length describe `text`, which Core
    // Foundation copies.
    unsafe { CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), length, UTF8, 0) }
}

impl CFString {
    /// The text of the string, or `None` when it has no UTF-8 form.
    pub(crate) fn to_rust(&self) -> Option<String> {
        let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(self), UTF8)
            .checked_add(1)
            .filter(|size| *size > 0)?;
        let mut buffer = vec![0u8; usize::try_from(size).ok()?];
        // SAFETY: Core Foundation writes at most `size` bytes, including
        // the terminating NUL, into the buffer.
        if unsafe { CFStringGetCString(self, buffer.as_mut_ptr(), size, UTF8) } == 0 {
            return None;
        }
        let end = buffer.iter().position(|&byte| byte == 0)?;
        buffer.truncate(end);
        String::from_utf8(buffer).ok()
    }
}

/// A new `CFData` with a copy of `bytes`.
pub(crate) fn cf_data(bytes: &[u8]) -> Option<CfRetained<CFData>> {
    let length = isize::try_from(bytes.len()).ok()?;
    // SAFETY: the pointer and the length describe `bytes`, which Core
    // Foundation copies.
    unsafe { CFDataCreate(std::ptr::null(), bytes.as_ptr(), length) }
}

/// A new empty `CFMutableData` without a capacity limit.
pub(crate) fn cf_mutable_data() -> Option<CfRetained<CFMutableData>> {
    // SAFETY: a null allocator selects the default allocator.
    unsafe { CFDataCreateMutable(std::ptr::null(), 0) }
}

impl CFData {
    /// A copy of the bytes.
    pub(crate) fn to_vec(&self) -> Vec<u8> {
        let length = usize::try_from(CFDataGetLength(self)).unwrap_or(0);
        let bytes = CFDataGetBytePtr(self);
        if bytes.is_null() || length == 0 {
            return Vec::new();
        }
        // SAFETY: the data holds `length` bytes at `bytes`, and nothing
        // changes them while the slice is copied.
        unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec()
    }
}

/// A new dictionary that retains its keys and values.
pub(crate) fn cf_dictionary(
    keys: &[&CFString],
    values: &[&CFType],
) -> Option<CfRetained<CFDictionary>> {
    if keys.len() != values.len() {
        return None;
    }
    let count = isize::try_from(keys.len()).ok()?;
    // SAFETY: both arrays hold `count` live objects. The type callbacks
    // make the dictionary retain them.
    unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            count,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    }
}

impl CFDictionary {
    /// The value for `key`, borrowed from the dictionary.
    pub(crate) fn get(&self, key: &CFString) -> Option<&CFType> {
        // SAFETY: the key is a live object, and the dictionary keeps the
        // value alive while it lives.
        unsafe { CFDictionaryGetValue(self, NonNull::from(key).cast()) }
    }
}

/// A new array that retains its elements.
pub(crate) fn cf_array<T: CfType>(values: &[&T]) -> Option<CfRetained<CFArray<T>>> {
    let count = isize::try_from(values.len()).ok()?;
    let values: Vec<&CFType> = values
        .iter()
        // SAFETY: every Core Foundation object is a `CFType`.
        .map(|value| unsafe { &*std::ptr::from_ref(*value).cast::<CFType>() })
        .collect();
    // SAFETY: the array holds `count` live objects. The type callbacks
    // make the new array retain them.
    let array = unsafe {
        CFArrayCreate(
            std::ptr::null(),
            values.as_ptr(),
            count,
            &raw const kCFTypeArrayCallBacks,
        )
    }?;
    // SAFETY: the elements of the new array are the `T` values.
    Some(unsafe { CfRetained::from_raw(array.into_raw().cast()) })
}

impl<T: CfType> CFArray<T> {
    /// The number of elements.
    pub(crate) fn len(&self) -> usize {
        // SAFETY: `self` is a live array.
        let count = unsafe { CFArrayGetCount(NonNull::from(self).cast()) };
        usize::try_from(count).unwrap_or(0)
    }

    /// The element at `index`, borrowed from the array, or `None` past the
    /// end.
    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len() {
            return None;
        }
        let index = isize::try_from(index).ok()?;
        // SAFETY: the index is inside the array.
        let value = unsafe { CFArrayGetValueAtIndex(NonNull::from(self).cast(), index) }?;
        // SAFETY: the elements are `T`, and the array keeps them alive.
        Some(unsafe { value.cast::<T>().as_ref() })
    }

    /// The elements, borrowed from the array.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.len()).filter_map(|index| self.get(index))
    }
}

/// A new attributed string with one set of attributes for all of `string`.
pub(crate) fn cf_attributed_string(
    string: &CFString,
    attributes: &CFDictionary,
) -> Option<CfRetained<CFAttributedString>> {
    CFAttributedStringCreate(None, string, attributes)
}

/// A new `CFNumber` that holds `value`.
pub(crate) fn cf_number(value: f64) -> Option<CfRetained<CFNumber>> {
    // SAFETY: the pointer reads one f64, which Core Foundation copies.
    unsafe { CFNumberCreate(std::ptr::null(), NUMBER_FLOAT64, (&raw const value).cast()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        for text in ["", "abc", "Tiếng Việt", "👩‍👩‍👧"] {
            let string = cf_string(text).unwrap();
            assert_eq!(string.to_rust().as_deref(), Some(text));
        }
    }

    #[test]
    fn data_round_trips() {
        assert_eq!(cf_data(&[1, 2, 3]).unwrap().to_vec(), [1, 2, 3]);
        assert!(cf_mutable_data().unwrap().to_vec().is_empty());
    }

    #[test]
    fn containers_retain_and_borrow() {
        let key = cf_string("key").unwrap();
        let value = cf_string("value").unwrap();
        assert!(cf_dictionary(&[&key], &[]).is_none());
        let dictionary = cf_dictionary(&[&key], &[&value]).unwrap();
        drop(value);
        let found = dictionary.get(&key).unwrap();
        assert_eq!(found.type_id(), key.type_id());
        let array = cf_array(&[&*key, &*key]).unwrap();
        assert_eq!(array.len(), 2);
        assert!(array.get(2).is_none());
        assert_eq!(array.iter().filter_map(CFString::to_rust).count(), 2);
    }
}
