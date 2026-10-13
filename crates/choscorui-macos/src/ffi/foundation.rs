//! Foundation classes, messages, and helpers.

use super::class::{Obj, objc_class};
use super::msg::{Autoreleased, Init, messages};
use super::retained::Retained;
use crate::Error;
use std::ffi::CString;

objc_class!(
    /// The root class. As an argument type it stands for any object.
    NSObject
);
objc_class!(NSAutoreleasePool: NSObject);
objc_class!(NSString: NSObject);
objc_class!(NSValue: NSObject);
objc_class!(NSNumber: NSValue: NSObject);
objc_class!(NSDictionary: NSObject);

messages! {
    pub(crate) RETAIN: instance NSObject, c"retain", () -> Option<&Obj<NSObject>>;
    pub(crate) RELEASE: instance NSObject, c"release", () -> ();
    pub(crate) ALLOC: class NSObject, c"alloc", () -> Option<Retained<NSObject>>;
    pub(crate) INIT: instance NSObject, c"init", () -> Init<NSObject>;
    pub(crate) DRAIN: instance NSAutoreleasePool, c"drain", () -> ();
    STRING_WITH_UTF8_STRING: class NSString, c"stringWithUTF8String:",
        (&CStr) -> Autoreleased<NSString>;
    pub(crate) NUMBER_WITH_INTEGER: class NSNumber, c"numberWithInteger:",
        (isize) -> Autoreleased<NSNumber>;
    DICTIONARY_WITH_OBJECTS: class NSDictionary, c"dictionaryWithObjects:forKeys:count:",
        (*const &Obj<NSObject>, *const &Obj<NSObject>, usize) -> Autoreleased<NSDictionary>;
}

use std::ffi::CStr;

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

/// A new `NSDictionary` with the key and value of each entry.
pub(crate) fn ns_dictionary(
    entries: &[(&Obj<NSObject>, &Obj<NSObject>)],
) -> Result<Retained<NSDictionary>, Error> {
    let (keys, values): (Vec<_>, Vec<_>) = entries.iter().copied().unzip();
    DICTIONARY_WITH_OBJECTS
        .send_class((values.as_ptr().cast(), keys.as_ptr().cast(), entries.len()))?
        .ok_or(Error::AppKitUnavailable)
}
