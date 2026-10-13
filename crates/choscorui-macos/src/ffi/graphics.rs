//! Core Graphics types and functions.

use super::cf::{CFData, CFDictionary, CFString, CfRetained, cf_data, cf_type};
use super::geometry::{CGAffineTransform, NSPoint, NSRect};
use std::ffi::c_void;
use std::ops::Range;
use std::ptr::NonNull;
use std::sync::OnceLock;

cf_type!(
    CGContext,
    CGColorSpace,
    CGColor,
    CGGradient,
    CGDataProvider,
    CGImage,
);

/// `kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big`: RGBA bytes
/// with premultiplied alpha.
pub(crate) const PREMULTIPLIED_RGBA: u32 = 1 | (4 << 12);
/// `kCGImageAlphaLast`: straight (non-premultiplied) RGBA pixels.
pub(crate) const STRAIGHT_RGBA: u32 = 3;
/// `kCGGradientDrawsBeforeStartLocation | kCGGradientDrawsAfterEndLocation`.
pub(crate) const GRADIENT_EXTEND: u32 = 1 | 2;

/// `CGLineCap`.
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

/// `CGLineJoin`.
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

/// `CGInterpolationQuality`.
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Interpolation {
    None = 1,
    High = 3,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C-unwind" {
    safe static kCGColorSpaceSRGB: &'static CFString;

    safe fn CGColorSpaceCreateWithName(name: &CFString) -> Option<CfRetained<CGColorSpace>>;
    fn CGColorCreate(space: &CGColorSpace, components: *const f64) -> Option<CfRetained<CGColor>>;
    fn CGGradientCreateWithColorComponents(
        space: &CGColorSpace,
        components: *const f64,
        locations: *const f64,
        count: usize,
    ) -> Option<CfRetained<CGGradient>>;
    safe fn CGDataProviderCreateWithCFData(data: &CFData) -> Option<CfRetained<CGDataProvider>>;
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: &CGColorSpace,
        bitmap_info: u32,
        provider: &CGDataProvider,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> Option<CfRetained<CGImage>>;
    pub(crate) safe fn CGImageGetWidth(image: &CGImage) -> usize;
    pub(crate) safe fn CGImageGetHeight(image: &CGImage) -> usize;
    pub(crate) safe fn CGAffineTransformInvert(transform: CGAffineTransform) -> CGAffineTransform;

    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: &CGColorSpace,
        bitmap_info: u32,
    ) -> Option<CfRetained<CGContext>>;
    safe fn CGBitmapContextGetData(context: &CGContext) -> *mut c_void;
    safe fn CGBitmapContextGetWidth(context: &CGContext) -> usize;
    safe fn CGBitmapContextGetHeight(context: &CGContext) -> usize;
    safe fn CGBitmapContextGetBytesPerRow(context: &CGContext) -> usize;
    safe fn CGBitmapContextGetBitsPerPixel(context: &CGContext) -> usize;
    safe fn CGBitmapContextGetBitsPerComponent(context: &CGContext) -> usize;
    safe fn CGBitmapContextGetBitmapInfo(context: &CGContext) -> u32;
    safe fn CGBitmapContextGetColorSpace(context: &CGContext) -> Option<&CGColorSpace>;

    pub(crate) safe fn CGContextSaveGState(context: &CGContext);
    pub(crate) safe fn CGContextRestoreGState(context: &CGContext);
    pub(crate) safe fn CGContextFlush(context: &CGContext);
    pub(crate) safe fn CGContextClipToRect(context: &CGContext, rect: NSRect);
    pub(crate) safe fn CGContextClip(context: &CGContext);
    pub(crate) safe fn CGContextEOClip(context: &CGContext);
    pub(crate) safe fn CGContextSetAlpha(context: &CGContext, alpha: f64);
    pub(crate) safe fn CGContextSetRGBFillColor(
        context: &CGContext,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    );
    pub(crate) safe fn CGContextSetFillColorWithColor(context: &CGContext, color: &CGColor);
    pub(crate) safe fn CGContextSetStrokeColorWithColor(context: &CGContext, color: &CGColor);
    pub(crate) safe fn CGContextFillRect(context: &CGContext, rect: NSRect);
    pub(crate) safe fn CGContextStrokeRectWithWidth(context: &CGContext, rect: NSRect, width: f64);
    pub(crate) safe fn CGContextBeginPath(context: &CGContext);
    pub(crate) safe fn CGContextMoveToPoint(context: &CGContext, x: f64, y: f64);
    pub(crate) safe fn CGContextAddLineToPoint(context: &CGContext, x: f64, y: f64);
    pub(crate) safe fn CGContextAddQuadCurveToPoint(
        context: &CGContext,
        cx: f64,
        cy: f64,
        x: f64,
        y: f64,
    );
    pub(crate) safe fn CGContextAddCurveToPoint(
        context: &CGContext,
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    );
    pub(crate) safe fn CGContextClosePath(context: &CGContext);
    pub(crate) safe fn CGContextFillPath(context: &CGContext);
    pub(crate) safe fn CGContextEOFillPath(context: &CGContext);
    pub(crate) safe fn CGContextStrokePath(context: &CGContext);
    pub(crate) safe fn CGContextReplacePathWithStrokedPath(context: &CGContext);
    pub(crate) safe fn CGContextSetLineWidth(context: &CGContext, width: f64);
    pub(crate) safe fn CGContextSetLineCap(context: &CGContext, cap: LineCap);
    pub(crate) safe fn CGContextSetLineJoin(context: &CGContext, join: LineJoin);
    pub(crate) safe fn CGContextSetMiterLimit(context: &CGContext, limit: f64);
    fn CGContextSetLineDash(context: &CGContext, phase: f64, lengths: *const f64, count: usize);
    pub(crate) safe fn CGContextConcatCTM(context: &CGContext, transform: CGAffineTransform);
    pub(crate) safe fn CGContextTranslateCTM(context: &CGContext, x: f64, y: f64);
    pub(crate) safe fn CGContextScaleCTM(context: &CGContext, x: f64, y: f64);
    pub(crate) safe fn CGContextGetUserSpaceToDeviceSpaceTransform(
        context: &CGContext,
    ) -> CGAffineTransform;
    pub(crate) safe fn CGContextBeginTransparencyLayer(
        context: &CGContext,
        auxiliary: Option<&CFDictionary>,
    );
    pub(crate) safe fn CGContextEndTransparencyLayer(context: &CGContext);
    pub(crate) safe fn CGContextSetInterpolationQuality(
        context: &CGContext,
        quality: Interpolation,
    );
    pub(crate) safe fn CGContextDrawImage(context: &CGContext, rect: NSRect, image: &CGImage);
    pub(crate) safe fn CGContextDrawLinearGradient(
        context: &CGContext,
        gradient: &CGGradient,
        start: NSPoint,
        end: NSPoint,
        options: u32,
    );
    pub(crate) safe fn CGContextDrawRadialGradient(
        context: &CGContext,
        gradient: &CGGradient,
        start_center: NSPoint,
        start_radius: f64,
        end_center: NSPoint,
        end_radius: f64,
        options: u32,
    );
    pub(crate) safe fn CGContextSetTextPosition(context: &CGContext, x: f64, y: f64);
    pub(crate) safe fn CGContextSetTextMatrix(context: &CGContext, matrix: CGAffineTransform);
    pub(crate) safe fn CGContextGetTextMatrix(context: &CGContext) -> CGAffineTransform;
}

/// The process-wide sRGB color space, created once and kept for the life
/// of the process.
pub(crate) fn srgb() -> Option<&'static CGColorSpace> {
    /// The address of the leaked color space, which any thread may use.
    static SPACE: OnceLock<Option<usize>> = OnceLock::new();
    let address = (*SPACE.get_or_init(|| {
        CGColorSpaceCreateWithName(kCGColorSpaceSRGB)
            .map(|space| space.into_raw().as_ptr() as usize)
    }))?;
    // SAFETY: the address holds a color space whose reference is never
    // released. Color spaces are immutable and thread-safe.
    Some(unsafe { &*(address as *const CGColorSpace) })
}

/// A new sRGB color with red, green, blue, and alpha components.
pub(crate) fn srgb_color(components: [f64; 4]) -> Option<CfRetained<CGColor>> {
    // SAFETY: sRGB has three color components plus alpha, so the color
    // reads the four values of the array.
    unsafe { CGColorCreate(srgb()?, components.as_ptr()) }
}

/// A new sRGB gradient with four components per location.
pub(crate) fn srgb_gradient(
    components: &[f64],
    locations: &[f64],
) -> Option<CfRetained<CGGradient>> {
    if components.len() != locations.len().checked_mul(4)? {
        return None;
    }
    // SAFETY: the arrays hold four components and one location for each
    // of the `locations.len()` stops.
    unsafe {
        CGGradientCreateWithColorComponents(
            srgb()?,
            components.as_ptr(),
            locations.as_ptr(),
            locations.len(),
        )
    }
}

/// The 8-bit, 32-bit-per-pixel layout of an image.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PixelLayout {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) bytes_per_row: usize,
    pub(crate) bitmap_info: u32,
    pub(crate) interpolate: bool,
}

/// A new image with a copy of `pixels`, or `None` when the pixels are
/// shorter than `layout` needs.
pub(crate) fn image_from_pixels(
    pixels: &[u8],
    layout: PixelLayout,
    space: &CGColorSpace,
) -> Option<CfRetained<CGImage>> {
    let needed = layout.bytes_per_row.checked_mul(layout.height)?;
    if layout.bytes_per_row < layout.width.checked_mul(4)? || pixels.len() < needed {
        return None;
    }
    let data = cf_data(pixels)?;
    let provider = CGDataProviderCreateWithCFData(&data)?;
    // SAFETY: the provider holds `bytes_per_row * height` bytes of 8-bit
    // RGBA pixels, and a null decode array selects the default.
    unsafe {
        CGImageCreate(
            layout.width,
            layout.height,
            8,
            32,
            layout.bytes_per_row,
            space,
            layout.bitmap_info,
            &provider,
            std::ptr::null(),
            layout.interpolate,
            0,
        )
    }
}

/// A new 8-bit RGBA sRGB bitmap context whose memory Core Graphics
/// allocates.
pub(crate) fn bitmap_context(
    width: usize,
    height: usize,
    bitmap_info: u32,
) -> Option<CfRetained<CGContext>> {
    // SAFETY: null data makes Core Graphics allocate and own the memory,
    // and a zero row length lets it choose the row length.
    unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            0,
            srgb()?,
            bitmap_info,
        )
    }
}

/// The memory layout of a bitmap context.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BitmapLayout {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) bytes_per_row: usize,
    pub(crate) bits_per_pixel: usize,
    pub(crate) bits_per_component: usize,
    pub(crate) bitmap_info: u32,
}

impl CGContext {
    /// The memory layout, or `None` when the context is not a bitmap
    /// context.
    pub(crate) fn bitmap_layout(&self) -> Option<BitmapLayout> {
        if CGBitmapContextGetData(self).is_null() {
            return None;
        }
        Some(BitmapLayout {
            width: CGBitmapContextGetWidth(self),
            height: CGBitmapContextGetHeight(self),
            bytes_per_row: CGBitmapContextGetBytesPerRow(self),
            bits_per_pixel: CGBitmapContextGetBitsPerPixel(self),
            bits_per_component: CGBitmapContextGetBitsPerComponent(self),
            bitmap_info: CGBitmapContextGetBitmapInfo(self),
        })
    }

    /// The color space of a bitmap context.
    pub(crate) fn bitmap_color_space(&self) -> Option<&CGColorSpace> {
        CGBitmapContextGetColorSpace(self)
    }

    /// Appends bytes `columns` of each row in `rows` of the bitmap memory
    /// to `out`. Returns `false`, and appends nothing, when the range is
    /// outside the bitmap or the context is not a bitmap context.
    pub(crate) fn copy_bitmap_rows(
        &self,
        rows: Range<usize>,
        columns: Range<usize>,
        out: &mut Vec<u8>,
    ) -> bool {
        let Some(layout) = self.bitmap_layout() else {
            return false;
        };
        let Some(data) = NonNull::new(CGBitmapContextGetData(self).cast::<u8>()) else {
            return false;
        };
        if rows.start > rows.end
            || rows.end > layout.height
            || columns.start > columns.end
            || columns.end > layout.bytes_per_row
        {
            return false;
        }
        out.reserve(rows.len().saturating_mul(columns.len()));
        for row in rows {
            let start = row * layout.bytes_per_row + columns.start;
            // SAFETY: the bitmap holds `bytes_per_row * height` bytes, and
            // the row and the columns are inside it.
            let first = unsafe { data.as_ptr().add(start) };
            // SAFETY: `columns.len()` bytes from `first` are inside the
            // bitmap. No drawing happens while the bytes are copied.
            let bytes = unsafe { std::slice::from_raw_parts(first, columns.len()) };
            out.extend_from_slice(bytes);
        }
        true
    }

    /// Sets the dash pattern of strokes; an empty pattern draws solid
    /// lines.
    pub(crate) fn set_line_dash(&self, lengths: &[f64]) {
        // SAFETY: the pointer and the count describe `lengths`, which Core
        // Graphics copies.
        unsafe { CGContextSetLineDash(self, 0.0, lengths.as_ptr(), lengths.len()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmaps_draw_and_copy_rows() {
        let context = bitmap_context(4, 2, PREMULTIPLIED_RGBA).unwrap();
        CGContextSetRGBFillColor(&context, 1.0, 0.0, 0.0, 1.0);
        let all = NSRect {
            size: super::super::geometry::NSSize {
                width: 4.0,
                height: 2.0,
            },
            ..NSRect::default()
        };
        CGContextFillRect(&context, all);
        let layout = context.bitmap_layout().unwrap();
        assert_eq!(
            (layout.width, layout.height, layout.bits_per_pixel),
            (4, 2, 32)
        );
        let mut out = Vec::new();
        assert!(context.copy_bitmap_rows(0..2, 0..16, &mut out));
        assert_eq!(out.len(), 32);
        assert_eq!(out[..4], [255, 0, 0, 255]);
        assert!(!context.copy_bitmap_rows(0..3, 0..16, &mut out));
        assert!(!context.copy_bitmap_rows(0..1, 0..layout.bytes_per_row + 1, &mut out));
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn short_pixels_make_no_image() {
        let layout = PixelLayout {
            width: 2,
            height: 2,
            bytes_per_row: 8,
            bitmap_info: STRAIGHT_RGBA,
            interpolate: true,
        };
        let space = srgb().unwrap();
        assert!(image_from_pixels(&[0; 15], layout, space).is_none());
        let image = image_from_pixels(&[0; 16], layout, space).unwrap();
        assert_eq!((CGImageGetWidth(&image), CGImageGetHeight(&image)), (2, 2));
        assert!(srgb_gradient(&[0.0; 4], &[0.0, 1.0]).is_none());
        assert!(srgb_gradient(&[0.0; 8], &[0.0, 1.0]).is_some());
        assert!(srgb_color([1.0, 0.0, 0.0, 1.0]).is_some());
    }
}
