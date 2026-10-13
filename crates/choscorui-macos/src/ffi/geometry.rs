//! Core Graphics and Foundation structs that cross the boundary by value.

#[cfg(test)]
use super::encode::encode_as;

/// `NSPoint`, the same type as `CGPoint`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NSPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// `NSSize`, the same type as `CGSize`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NSSize {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// `NSRect`, the same type as `CGRect`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NSRect {
    pub(crate) origin: NSPoint,
    pub(crate) size: NSSize,
}

/// `NSRange`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NSRange {
    pub(crate) location: usize,
    pub(crate) length: usize,
}

/// `CGAffineTransform`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CGAffineTransform {
    pub(crate) a: f64,
    pub(crate) b: f64,
    pub(crate) c: f64,
    pub(crate) d: f64,
    pub(crate) tx: f64,
    pub(crate) ty: f64,
}

#[cfg(test)]
encode_as!(NSPoint, "{CGPoint=dd}");
#[cfg(test)]
encode_as!(NSSize, "{CGSize=dd}");
#[cfg(test)]
encode_as!(NSRect, "{CGRect={CGPoint=dd}{CGSize=dd}}");
#[cfg(test)]
encode_as!(NSRange, "{_NSRange=QQ}");
#[cfg(test)]
encode_as!(CGAffineTransform, "{CGAffineTransform=dddddd}");
