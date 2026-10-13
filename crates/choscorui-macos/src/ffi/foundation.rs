//! Foundation classes, messages, and helpers.

use super::class::{AnyClass, ClassType, Obj, objc_class};
use super::msg::{Autoreleased, Init, Utf8, messages};
use super::retained::Retained;
use crate::Error;
use std::ffi::{CStr, CString, c_void};

objc_class!(
    /// The root class. As an argument type it stands for any object.
    NSObject
);
objc_class!(NSAutoreleasePool: NSObject);
objc_class!(NSString: NSObject);
objc_class!(NSValue: NSObject);
objc_class!(NSNumber: NSValue: NSObject);
objc_class!(NSDictionary: NSObject);
objc_class!(NSArray: NSObject);
objc_class!(NSData: NSObject);
objc_class!(NsUrl = "NSURL": NSObject);

messages! {
    pub(crate) RETAIN: instance NSObject, c"retain", () -> Option<&Obj<NSObject>>;
    pub(crate) RELEASE: instance NSObject, c"release", () -> ();
    pub(crate) ALLOC: class NSObject, c"alloc", () -> Option<Retained<NSObject>>;
    pub(crate) INIT: instance NSObject, c"init", () -> Init<NSObject>;
    IS_KIND_OF_CLASS: instance NSObject, c"isKindOfClass:", (&AnyClass) -> bool;
    pub(crate) DRAIN: instance NSAutoreleasePool, c"drain", () -> ();
    STRING_WITH_UTF8_STRING: class NSString, c"stringWithUTF8String:",
        (&CStr) -> Autoreleased<NSString>;
    UTF8_STRING: instance NSString, c"UTF8String", () -> Utf8;
    pub(crate) PRECOMPOSED: instance NSString, c"precomposedStringWithCanonicalMapping",
        () -> Autoreleased<NSString>;
    pub(crate) NUMBER_WITH_INTEGER: class NSNumber, c"numberWithInteger:",
        (isize) -> Autoreleased<NSNumber>;
    DICTIONARY_WITH_OBJECTS: class NSDictionary, c"dictionaryWithObjects:forKeys:count:",
        (*const &Obj<NSObject>, *const &Obj<NSObject>, usize) -> Autoreleased<NSDictionary>;
    ARRAY_WITH_OBJECTS: class NSArray, c"arrayWithObjects:count:",
        (*const &Obj<NSObject>, usize) -> Autoreleased<NSArray>;
    COUNT: instance NSArray, c"count", () -> usize;
    OBJECT_AT_INDEX: instance NSArray, c"objectAtIndex:", (usize) -> Option<&Obj<NSObject>>;
    DATA_WITH_BYTES: class NSData, c"dataWithBytes:length:",
        (*const c_void, usize) -> Autoreleased<NSData>;
    LENGTH: instance NSData, c"length", () -> usize;
    BYTES: instance NSData, c"bytes", () -> *const c_void;
    pub(crate) FILE_URL_WITH_PATH: class NsUrl, c"fileURLWithPath:",
        (&Obj<NSString>) -> Autoreleased<NsUrl>;
    pub(crate) URL_WITH_STRING: class NsUrl, c"URLWithString:",
        (&Obj<NSString>) -> Autoreleased<NsUrl>;
    pub(crate) ABSOLUTE_STRING: instance NsUrl, c"absoluteString", () -> Autoreleased<NSString>;
    pub(crate) PATH: instance NsUrl, c"path", () -> Autoreleased<NSString>;
    pub(crate) IS_FILE_URL: instance NsUrl, c"isFileURL", () -> bool;
}

impl<C> Obj<C> {
    /// This object as an instance of `D`, when `isKindOfClass:` says so.
    pub(crate) fn downcast<D: ClassType>(&self) -> Option<&Obj<D>> {
        let class = D::class().ok()?;
        IS_KIND_OF_CLASS
            .send(self.cast(), (class,))
            .then(|| self.cast())
    }
}

/// Why a Rust string did not become an `NSString`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextError {
    /// The string contains a NUL character.
    InteriorNul,
    /// Foundation returned nil.
    Unavailable,
}

impl From<TextError> for Error {
    fn from(error: TextError) -> Self {
        match error {
            TextError::InteriorNul => Error::InvalidTitle,
            TextError::Unavailable => Error::AppKitUnavailable,
        }
    }
}

/// A new `NSString` with the UTF-8 text of `value`.
pub(crate) fn ns_string(value: &str) -> Result<Retained<NSString>, TextError> {
    let value = CString::new(value).map_err(|_| TextError::InteriorNul)?;
    STRING_WITH_UTF8_STRING
        .send_class((value.as_c_str(),))
        .ok()
        .flatten()
        .ok_or(TextError::Unavailable)
}

/// A copy of the text of `value`, or `None` when it has no UTF-8 form.
/// Invalid sequences become U+FFFD.
pub(crate) fn string(value: &Obj<NSString>) -> Option<String> {
    let text = UTF8_STRING.send(value, ());
    if text.is_null() {
        return None;
    }
    // SAFETY: `UTF8String` returns a NUL-terminated buffer that lives at
    // least as long as `value` and the current autorelease pool; the text
    // is copied before this function returns.
    let text = unsafe { CStr::from_ptr(text) };
    Some(text.to_string_lossy().into_owned())
}

/// A new `NSDictionary` with the key and value of each entry.
pub(crate) fn ns_dictionary(
    entries: &[(&Obj<NSObject>, &Obj<NSObject>)],
) -> Result<Retained<NSDictionary>, Error> {
    let (keys, values): (Vec<_>, Vec<_>) = entries.iter().copied().unzip();
    DICTIONARY_WITH_OBJECTS
        .send_class((values.as_ptr().cast(), keys.as_ptr().cast(), entries.len()))?
        .ok_or(Error::AppKitUnavailable)
}

/// A new `NSArray` that retains each of `values`.
pub(crate) fn ns_array(values: &[&Obj<NSObject>]) -> Result<Retained<NSArray>, Error> {
    ARRAY_WITH_OBJECTS
        .send_class((values.as_ptr().cast(), values.len()))?
        .ok_or(Error::AppKitUnavailable)
}

/// The objects of `array`, borrowed from it.
pub(crate) fn array_items(array: &Obj<NSArray>) -> impl Iterator<Item = &Obj<NSObject>> {
    (0..COUNT.send(array, ())).filter_map(move |index| OBJECT_AT_INDEX.send(array, (index,)))
}

/// A new `NSData` with a copy of `bytes`.
pub(crate) fn ns_data(bytes: &[u8]) -> Result<Retained<NSData>, Error> {
    DATA_WITH_BYTES
        .send_class((bytes.as_ptr().cast(), bytes.len()))?
        .ok_or(Error::AppKitUnavailable)
}

/// A copy of the bytes of `data`.
pub(crate) fn data_bytes(data: &Obj<NSData>) -> Vec<u8> {
    let length = LENGTH.send(data, ());
    let bytes = BYTES.send(data, ());
    if length == 0 || bytes.is_null() {
        return Vec::new();
    }
    // SAFETY: `bytes` points to `length` readable bytes that `data` owns;
    // they are copied before `data` can change or be released.
    let bytes = unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), length) };
    bytes.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::Pool;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;

    const SIGABRT: i32 = 6;

    /// Set in the child process of the exception test.
    const CHILD: &str = "CHOSCORUI_OBJC_EXCEPTION_CHILD";
    const CHILD_TEST: &str = "ffi::foundation::tests::out_of_range_index_raises";

    #[test]
    #[ignore = "raises an Objective-C exception; an_objective_c_exception_is_loud runs it"]
    fn out_of_range_index_raises() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        let _pool = Pool::new().unwrap();
        let array = ns_array(&[]).unwrap();
        OBJECT_AT_INDEX.send(&array, (0,));
    }

    #[test]
    fn an_objective_c_exception_is_loud() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The exception unwinds as a foreign exception and aborts the
        // process; it never returns or exits normally.
        assert_eq!(output.status.signal(), Some(SIGABRT), "{stderr}");
    }
}
