//! ImageIO decoding and encoding between encoded files and core images.

use super::*;
use choscorui_core::{Image, ImageError};

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithData(data: Id, options: Id) -> Id;
    fn CGImageSourceGetCount(source: Id) -> usize;
    fn CGImageSourceCreateImageAtIndex(source: Id, index: usize, options: Id) -> Id;
    fn CGImageDestinationCreateWithData(data: Id, kind: Id, count: usize, options: Id) -> Id;
    fn CGImageDestinationAddImage(destination: Id, image: Id, properties: Id);
    fn CGImageDestinationFinalize(destination: Id) -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageGetWidth(image: Id) -> usize;
    fn CGImageGetHeight(image: Id) -> usize;
    fn CGContextDrawImage(context: Id, rect: NSRect, image: Id);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreate(allocator: Id, bytes: *const u8, length: isize) -> Id;
    fn CFDataCreateMutable(allocator: Id, capacity: isize) -> Id;
    fn CFDataGetLength(data: Id) -> isize;
    fn CFDataGetBytePtr(data: Id) -> *const u8;
}

const MAX_PIXELS: usize = 64 * 1024 * 1024;

/// Decodes PNG, JPEG, GIF (first frame), TIFF, HEIC, and other formats
/// ImageIO supports into straight-alpha sRGB RGBA8 pixels. Embedded color
/// profiles are converted to sRGB.
pub fn decode_image(bytes: &[u8]) -> Result<Image, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::UnsupportedFormat);
    }
    // SAFETY: CFData copies `bytes`. Every Create result is owned by
    // CfOwned and released when this function returns.
    unsafe {
        let _pool = Pool::new();
        let data = CfOwned::new(CFDataCreate(
            std::ptr::null_mut(),
            bytes.as_ptr(),
            bytes.len() as isize,
        ))
        .ok_or(ImageError::DecodeFailed)?;
        let source = CfOwned::new(CGImageSourceCreateWithData(
            data.as_ptr(),
            std::ptr::null_mut(),
        ))
        .ok_or(ImageError::UnsupportedFormat)?;
        if CGImageSourceGetCount(source.as_ptr()) == 0 {
            return Err(ImageError::UnsupportedFormat);
        }
        let image = CfOwned::new(CGImageSourceCreateImageAtIndex(
            source.as_ptr(),
            0,
            std::ptr::null_mut(),
        ))
        .ok_or(ImageError::DecodeFailed)?;
        image_from_cg(image.as_ptr())
    }
}

/// Rasterizes a `CGImage` into straight RGBA8 pixels.
pub(crate) unsafe fn image_from_cg(cg: Id) -> Result<Image, ImageError> {
    unsafe {
        let width = CGImageGetWidth(cg);
        let height = CGImageGetHeight(cg);
        if width == 0 || height == 0 || width.saturating_mul(height) > MAX_PIXELS {
            return Err(ImageError::InvalidDimensions);
        }
        let context = CfOwned::new(CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            0,
            paint::srgb(),
            1 | (4 << 12), // premultiplied last, 32-bit big endian: RGBA bytes
        ))
        .ok_or(ImageError::DecodeFailed)?;
        CGContextDrawImage(
            context.as_ptr(),
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: width as f64,
                    height: height as f64,
                },
            },
            cg,
        );
        let data = NonNull::new(CGBitmapContextGetData(context.as_ptr()))
            .ok_or(ImageError::DecodeFailed)?;
        let stride = CGBitmapContextGetBytesPerRow(context.as_ptr());
        if stride < width * 4 {
            return Err(ImageError::DecodeFailed);
        }
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let row =
                std::slice::from_raw_parts((data.as_ptr() as *const u8).add(y * stride), width * 4);
            for px in row.chunks_exact(4) {
                pixels.extend_from_slice(&unpremultiply(px));
            }
        }
        Image::from_rgba8(width as u32, height as u32, pixels)
    }
}

pub(crate) fn unpremultiply(px: &[u8]) -> [u8; 4] {
    let a = px[3];
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let un = |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
    [un(px[0]), un(px[1]), un(px[2]), a]
}

/// Encodes straight-alpha RGBA8 pixels as an sRGB PNG.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, ImageError> {
    // SAFETY: cg_image copies pixels. The destination writes into a
    // mutable CFData we own; its bytes are copied out before release.
    unsafe {
        let _pool = Pool::new();
        let cg = paint::cg_image(image).ok_or(ImageError::EncodeFailed)?;
        let out = CfOwned::new(CFDataCreateMutable(std::ptr::null_mut(), 0))
            .ok_or(ImageError::EncodeFailed)?;
        let kind = cf_utf8("public.png").ok_or(ImageError::EncodeFailed)?;
        let destination = CfOwned::new(CGImageDestinationCreateWithData(
            out.as_ptr(),
            kind.as_ptr(),
            1,
            std::ptr::null_mut(),
        ))
        .ok_or(ImageError::EncodeFailed)?;
        CGImageDestinationAddImage(destination.as_ptr(), cg.as_ptr(), std::ptr::null_mut());
        if !CGImageDestinationFinalize(destination.as_ptr()) {
            return Err(ImageError::EncodeFailed);
        }
        drop(destination);
        let len = CFDataGetLength(out.as_ptr());
        let ptr = CFDataGetBytePtr(out.as_ptr());
        if ptr.is_null() || len <= 0 {
            return Err(ImageError::EncodeFailed);
        }
        Ok(std::slice::from_raw_parts(ptr, len as usize).to_vec())
    }
}

impl MacBitmap {
    /// Converts the premultiplied bitmap to a straight-alpha core image.
    pub fn to_image(&self) -> Option<Image> {
        let pixels = self
            .pixels()
            .chunks_exact(4)
            .flat_map(unpremultiply)
            .collect();
        Image::from_rgba8(self.width(), self.height(), pixels).ok()
    }

    /// Encodes the bitmap as PNG for offscreen export.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        encode_png(&self.to_image().ok_or(ImageError::InvalidDimensions)?)
    }
}

/// Autoreleased NSImage sized in points from straight RGBA pixels.
/// Requires an autorelease pool.
pub(crate) unsafe fn ns_image(image: &Image) -> Option<Id> {
    unsafe {
        let cg = paint::cg_image(image)?;
        let size = NSSize {
            width: f64::from(image.width()),
            height: f64::from(image.height()),
        };
        let f: unsafe extern "C" fn(Id, Sel, Id, NSSize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let ns_image = f(
            send_id(class(c"NSImage").ok()?, c"alloc"),
            selector(c"initWithCGImage:size:"),
            cg.as_ptr(),
            size,
        );
        if ns_image.is_null() {
            return None;
        }
        send_id(ns_image, c"autorelease");
        Some(ns_image)
    }
}
