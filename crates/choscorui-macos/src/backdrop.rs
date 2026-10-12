//! Backdrop blur: reads the pixels of a bitmap context, blurs them with
//! the shared core blur, and draws them back inside a rounded rectangle.

use super::*;
use choscorui_core::{Path, blur_pixels};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGBitmapContextGetWidth(context: Id) -> usize;
    fn CGBitmapContextGetHeight(context: Id) -> usize;
    fn CGBitmapContextGetBitsPerPixel(context: Id) -> usize;
    fn CGBitmapContextGetBitsPerComponent(context: Id) -> usize;
    fn CGBitmapContextGetBitmapInfo(context: Id) -> u32;
    fn CGBitmapContextGetColorSpace(context: Id) -> Id;
    fn CGContextGetUserSpaceToDeviceSpaceTransform(context: Id) -> CGAffineTransform;
    fn CGAffineTransformInvert(transform: CGAffineTransform) -> CGAffineTransform;
    fn CGContextConcatCTM(context: Id, transform: CGAffineTransform);
    fn CGContextClip(context: Id);
    fn CGContextFlush(context: Id);
    fn CGContextSetInterpolationQuality(context: Id, quality: i32);
    fn CGContextDrawImage(context: Id, rect: NSRect, image: Id);
    fn CGDataProviderCreateWithCFData(data: Id) -> Id;
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: Id,
        bitmap_info: u32,
        provider: Id,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> Id;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreate(allocator: Id, bytes: *const u8, length: isize) -> Id;
}

/// `kCGInterpolationNone`: the blurred pixels land on device pixels.
const INTERPOLATION_NONE: i32 = 1;

/// The device pixels that the blur reads, as a column and row range in
/// bitmap memory, where row 0 is the top.
struct Region {
    x0: usize,
    x1: usize,
    r0: usize,
    r1: usize,
}

/// The pixels under `rect` and three standard deviations around it.
/// `device` maps user space to device space, whose y axis points up.
fn region(
    device: CGAffineTransform,
    rect: Rect,
    margin: f64,
    width: usize,
    height: usize,
) -> Region {
    let corners = [
        (rect.min_x(), rect.min_y()),
        (rect.max_x(), rect.min_y()),
        (rect.min_x(), rect.max_y()),
        (rect.max_x(), rect.max_y()),
    ]
    .map(|(x, y)| {
        (
            device.a * x + device.c * y + device.tx,
            device.b * x + device.d * y + device.ty,
        )
    });
    let fold = |pick: fn(&(f64, f64)) -> f64, start: f64, combine: fn(f64, f64) -> f64| {
        corners.iter().map(pick).fold(start, combine)
    };
    let min_x = fold(|c| c.0, f64::INFINITY, f64::min);
    let max_x = fold(|c| c.0, f64::NEG_INFINITY, f64::max);
    let min_y = fold(|c| c.1, f64::INFINITY, f64::min);
    let max_y = fold(|c| c.1, f64::NEG_INFINITY, f64::max);
    let rows = height as f64;
    let clamp = |value: f64, limit: usize| value.clamp(0.0, limit as f64) as usize;
    Region {
        x0: clamp((min_x).floor() - margin, width),
        x1: clamp((max_x).ceil() + margin, width),
        r0: clamp((rows - max_y).floor() - margin, height),
        r1: clamp((rows - min_y).ceil() + margin, height),
    }
}

/// Blurs the bitmap under the rounded `rect` with a Gaussian of standard
/// deviation `blur` points. A context that is not a 32-bit bitmap context
/// paints nothing.
///
/// # Safety
///
/// `context` must be a valid `CGContext` for the duration of the call.
pub(crate) unsafe fn backdrop_blur(context: Id, rect: Rect, radius: f64, blur: f64) {
    // SAFETY: The caller passes a live context. The bitmap getters return
    // null or zero for other contexts, which return early. Row reads stay
    // inside `stride * height` bytes after the stride check. The blurred
    // copy is moved into CFData before the image uses it, and every +1
    // object is released by `CfOwned`.
    unsafe {
        let data = CGBitmapContextGetData(context);
        if data.is_null()
            || CGBitmapContextGetBitsPerPixel(context) != 32
            || CGBitmapContextGetBitsPerComponent(context) != 8
        {
            return;
        }
        let width = CGBitmapContextGetWidth(context);
        let height = CGBitmapContextGetHeight(context);
        let stride = CGBitmapContextGetBytesPerRow(context);
        if stride < width.saturating_mul(4) {
            return;
        }
        CGContextFlush(context);
        let device = CGContextGetUserSpaceToDeviceSpaceTransform(context);
        let sigma = blur * (device.a * device.d - device.b * device.c).abs().sqrt();
        let area = region(device, rect, (sigma * 3.0).ceil(), width, height);
        if area.x1 <= area.x0 || area.r1 <= area.r0 {
            return;
        }
        let (columns, rows) = (area.x1 - area.x0, area.r1 - area.r0);
        let mut pixels = Vec::with_capacity(columns * rows * 4);
        for row in area.r0..area.r1 {
            let start = (data as *const u8).add(row * stride + area.x0 * 4);
            pixels.extend_from_slice(std::slice::from_raw_parts(start, columns * 4));
        }
        blur_pixels(&mut pixels, columns, rows, columns * 4, sigma);
        let Some(bytes) = CfOwned::new(CFDataCreate(
            std::ptr::null_mut(),
            pixels.as_ptr(),
            pixels.len() as isize,
        )) else {
            return;
        };
        let Some(provider) = CfOwned::new(CGDataProviderCreateWithCFData(bytes.as_ptr())) else {
            return;
        };
        let Some(image) = CfOwned::new(CGImageCreate(
            columns,
            rows,
            8,
            32,
            columns * 4,
            CGBitmapContextGetColorSpace(context),
            CGBitmapContextGetBitmapInfo(context),
            provider.as_ptr(),
            std::ptr::null(),
            false,
            0,
        )) else {
            return;
        };
        CGContextSaveGState(context);
        paint::add_path(context, &Path::rounded_rect(rect, radius));
        CGContextClip(context);
        // Draw in device space so that each blurred pixel lands where it
        // was read.
        CGContextConcatCTM(context, CGAffineTransformInvert(device));
        CGContextSetInterpolationQuality(context, INTERPOLATION_NONE);
        let target = NSRect {
            origin: NSPoint {
                x: area.x0 as f64,
                y: (height - area.r1) as f64,
            },
            size: NSSize {
                width: columns as f64,
                height: rows as f64,
            },
        };
        CGContextDrawImage(context, target, image.as_ptr());
        CGContextRestoreGState(context);
    }
}
