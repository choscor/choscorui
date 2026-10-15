//! libwebp: WebP decoding through the simple decoding API, which reports
//! errors by its result.

use super::{CodecError, PixelLayout, Raster, checked_size};
use crate::ffi::ctypes::{CInt, c_void};
use crate::ffi::macros::libraries;

libraries! {
    /// libwebp: WebP decoding.
    pub(crate) struct WebP {
        library: "libwebp",
        sonames: ["libwebp.so.7"],
        headers: ["webp/decode.h"],
        pkg_config: "libwebp",
        package: "libwebp-dev",
        functions {
            fn WebPGetInfo(data: *const u8, size: usize, width: *mut CInt, height: *mut CInt) -> CInt;
            fn WebPDecodeRGBA(
                data: *const u8,
                size: usize,
                width: *mut CInt,
                height: *mut CInt,
            ) -> *mut u8;
            fn WebPFree(pointer: *mut c_void);
        }
    }
}

/// Pixels that libwebp allocated, freed on drop.
struct Pixels {
    webp: &'static WebP,
    pointer: *mut u8,
}

impl Drop for Pixels {
    fn drop(&mut self) {
        // SAFETY: libwebp allocated the pointer, and nothing uses it after
        // the drop.
        unsafe { (self.webp.WebPFree)(self.pointer.cast()) };
    }
}

/// Decodes a still WebP image to straight-alpha RGBA. An animated image
/// fails, because the simple API decodes only still images.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<Raster, CodecError> {
    let webp = WebP::get()?;
    let (mut width, mut height) = (0, 0);
    // SAFETY: `bytes` holds `bytes.len()` bytes, and the outputs point at
    // local integers.
    let valid = unsafe { (webp.WebPGetInfo)(bytes.as_ptr(), bytes.len(), &mut width, &mut height) };
    if valid == 0 {
        return Err(CodecError::Invalid);
    }
    let (width, height, count) = checked_size(width, height, limit)?;
    let (mut decoded_width, mut decoded_height) = (0, 0);
    // SAFETY: as above.
    let pointer = unsafe {
        (webp.WebPDecodeRGBA)(
            bytes.as_ptr(),
            bytes.len(),
            &mut decoded_width,
            &mut decoded_height,
        )
    };
    if pointer.is_null() {
        return Err(CodecError::Invalid);
    }
    let decoded = Pixels { webp, pointer };
    if checked_size(decoded_width, decoded_height, limit)? != (width, height, count) {
        return Err(CodecError::Invalid);
    }
    // SAFETY: libwebp returns `width * height` RGBA pixels without row
    // padding, and they live until `decoded` drops.
    let pixels = unsafe { std::slice::from_raw_parts(decoded.pointer, count * 4) }.to_vec();
    Ok(Raster {
        width,
        height,
        layout: PixelLayout::Rgba,
        pixels,
    })
}
