//! Core Graphics and Foundation structs that cross the boundary by value.

use super::encode::encode_as;
use super::sdk::ffi_struct;

ffi_struct! {
    /// `NSPoint`, the same type as `CGPoint`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct NSPoint = CGPoint {
        pub(crate) x: f64,
        pub(crate) y: f64,
    }

    /// `NSSize`, the same type as `CGSize`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct NSSize = CGSize {
        pub(crate) width: f64,
        pub(crate) height: f64,
    }

    /// `NSRect`, the same type as `CGRect`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct NSRect = CGRect {
        pub(crate) origin: NSPoint,
        pub(crate) size: NSSize,
    }

    /// `NSRange`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct NSRange = NSRange {
        pub(crate) location: usize,
        pub(crate) length: usize,
    }

    /// `CGAffineTransform`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct CGAffineTransform = CGAffineTransform {
        pub(crate) a: f64,
        pub(crate) b: f64,
        pub(crate) c: f64,
        pub(crate) d: f64,
        pub(crate) tx: f64,
        pub(crate) ty: f64,
    }
}

impl NSRange {
    /// A range without a location, as the text input callbacks report it.
    pub(crate) const NONE: Self = Self {
        location: usize::MAX,
        length: 0,
    };
}

encode_as!(NSPoint, "{CGPoint=dd}");
encode_as!(NSSize, "{CGSize=dd}");
encode_as!(NSRect, "{CGRect={CGPoint=dd}{CGSize=dd}}");
encode_as!(NSRange, "{_NSRange=QQ}");
encode_as!(CGAffineTransform, "{CGAffineTransform=dddddd}");
