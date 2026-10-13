//! Backdrop blur: reads the pixels of a bitmap context, blurs them with
//! the shared core blur, and draws them back inside a rounded rectangle.

#![deny(unsafe_code)]

use super::*;
use crate::ffi::graphics::{
    CGAffineTransformInvert, CGContext, CGContextClip, CGContextConcatCTM, CGContextDrawImage,
    CGContextFlush, CGContextGetUserSpaceToDeviceSpaceTransform, CGContextRestoreGState,
    CGContextSaveGState, CGContextSetInterpolationQuality, Interpolation, PixelLayout,
    image_from_pixels,
};
use choscorui_core::{Path, blur_pixels};

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

/// Blurs the bitmap under `rect`, rounded with `radii` clockwise from the
/// top left, with a Gaussian of standard deviation `blur` points. A context
/// that is not a 32-bit bitmap context paints nothing.
pub(crate) fn backdrop_blur(context: &CGContext, rect: Rect, radii: [f64; 4], blur: f64) {
    let Some(bitmap) = context.bitmap_layout() else {
        return;
    };
    let (width, height) = (bitmap.width, bitmap.height);
    if bitmap.bits_per_pixel != 32
        || bitmap.bits_per_component != 8
        || bitmap.bytes_per_row < width.saturating_mul(4)
    {
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
    if !context.copy_bitmap_rows(area.r0..area.r1, area.x0 * 4..area.x1 * 4, &mut pixels) {
        return;
    }
    blur_pixels(&mut pixels, columns, rows, columns * 4, sigma);
    let Some(space) = context.bitmap_color_space() else {
        return;
    };
    let layout = PixelLayout {
        width: columns,
        height: rows,
        bytes_per_row: columns * 4,
        bitmap_info: bitmap.bitmap_info,
        interpolate: false,
    };
    let Some(image) = image_from_pixels(&pixels, layout, space) else {
        return;
    };
    CGContextSaveGState(context);
    paint::add_path(context, &Path::rounded_rect_corners(rect, radii));
    CGContextClip(context);
    // Draw in device space so that each blurred pixel lands where it was
    // read.
    CGContextConcatCTM(context, CGAffineTransformInvert(device));
    CGContextSetInterpolationQuality(context, Interpolation::None);
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
    CGContextDrawImage(context, target, &image);
    CGContextRestoreGState(context);
}
