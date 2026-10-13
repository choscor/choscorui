//! ImageIO decoding and encoding between encoded files and core images.

#![deny(unsafe_code)]

use super::*;
use crate::ffi::appkit::{INIT_WITH_CG_IMAGE_SIZE, NSImage};
use crate::ffi::cf::{cf_data, cf_mutable_data, cf_string};
use crate::ffi::graphics::{
    CGContextDrawImage, CGImage, CGImageGetHeight, CGImageGetWidth, PREMULTIPLIED_RGBA,
    bitmap_context,
};
use crate::ffi::image_io::{
    CGImageDestinationAddImage, CGImageDestinationCreateWithData, CGImageDestinationFinalize,
    CGImageSourceCreateImageAtIndex, CGImageSourceCreateWithData, CGImageSourceGetCount,
};
use crate::ffi::retained::{Retained, alloc};
use choscorui_core::{Image, ImageError};

const MAX_PIXELS: usize = 64 * 1024 * 1024;

/// Decodes PNG, JPEG, GIF (first frame), TIFF, HEIC, and other formats
/// ImageIO supports into straight-alpha sRGB RGBA8 pixels. Embedded color
/// profiles are converted to sRGB.
pub fn decode_image(bytes: &[u8]) -> Result<Image, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::UnsupportedFormat);
    }
    let _pool = Pool::new();
    let data = cf_data(bytes).ok_or(ImageError::DecodeFailed)?;
    let source = CGImageSourceCreateWithData(&data, None).ok_or(ImageError::UnsupportedFormat)?;
    if CGImageSourceGetCount(&source) == 0 {
        return Err(ImageError::UnsupportedFormat);
    }
    let image =
        CGImageSourceCreateImageAtIndex(&source, 0, None).ok_or(ImageError::DecodeFailed)?;
    image_from_cg(&image)
}

/// Rasterizes a `CGImage` into straight RGBA8 pixels.
pub(crate) fn image_from_cg(cg: &CGImage) -> Result<Image, ImageError> {
    let width = CGImageGetWidth(cg);
    let height = CGImageGetHeight(cg);
    if width == 0 || height == 0 || width.saturating_mul(height) > MAX_PIXELS {
        return Err(ImageError::InvalidDimensions);
    }
    let context =
        bitmap_context(width, height, PREMULTIPLIED_RGBA).ok_or(ImageError::DecodeFailed)?;
    CGContextDrawImage(
        &context,
        NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: width as f64,
                height: height as f64,
            },
        },
        cg,
    );
    let mut premultiplied = Vec::with_capacity(width * height * 4);
    if !context.copy_bitmap_rows(0..height, 0..width * 4, &mut premultiplied) {
        return Err(ImageError::DecodeFailed);
    }
    let pixels = premultiplied
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(unpremultiply)
        .collect();
    Image::from_rgba8(width as u32, height as u32, pixels)
}

pub(crate) fn unpremultiply(px: &[u8; 4]) -> [u8; 4] {
    let a = px[3];
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let un = |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
    [un(px[0]), un(px[1]), un(px[2]), a]
}

/// Encodes straight-alpha RGBA8 pixels as an sRGB PNG.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, ImageError> {
    let _pool = Pool::new();
    let cg = paint::cg_image(image).ok_or(ImageError::EncodeFailed)?;
    let out = cf_mutable_data().ok_or(ImageError::EncodeFailed)?;
    let kind = cf_string("public.png").ok_or(ImageError::EncodeFailed)?;
    let destination =
        CGImageDestinationCreateWithData(&out, &kind, 1, None).ok_or(ImageError::EncodeFailed)?;
    CGImageDestinationAddImage(&destination, &cg, None);
    if !CGImageDestinationFinalize(&destination) {
        return Err(ImageError::EncodeFailed);
    }
    drop(destination);
    let bytes = out.to_vec();
    if bytes.is_empty() {
        return Err(ImageError::EncodeFailed);
    }
    Ok(bytes)
}

impl MacBitmap {
    /// Converts the premultiplied bitmap to a straight-alpha core image.
    pub fn to_image(&self) -> Option<Image> {
        let pixels = self
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(unpremultiply)
            .collect();
        Image::from_rgba8(self.width(), self.height(), pixels).ok()
    }

    /// Encodes the bitmap as PNG for offscreen export.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        encode_png(&self.to_image().ok_or(ImageError::InvalidDimensions)?)
    }
}

/// An NSImage sized in points from straight RGBA pixels.
pub(crate) fn ns_image(image: &Image) -> Option<Retained<NSImage>> {
    let cg = paint::cg_image(image)?;
    let size = NSSize {
        width: f64::from(image.width()),
        height: f64::from(image.height()),
    };
    INIT_WITH_CG_IMAGE_SIZE.init(alloc::<NSImage>().ok()?, (&cg, size))
}
