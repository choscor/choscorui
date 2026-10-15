//! libturbojpeg: JPEG decoding through the TurboJPEG API, which reports
//! errors by its result.

use super::{CodecError, PixelLayout, Raster, checked_size};
use crate::ffi::ctypes::{CInt, CUlong, c_void};
use crate::ffi::macros::{c_consts, libraries};

c_consts! {
    /// The constants of TurboJPEG, for the binding check.
    pub(crate) fn constants;
    const TJPF_RGBA: CInt = 7;
    const TJPF_CMYK: CInt = 11;
    const TJCS_CMYK: CInt = 3;
    const TJCS_YCCK: CInt = 4;
    const TJERR_WARNING: CInt = 0;
}

libraries! {
    /// libturbojpeg: JPEG decoding.
    pub(crate) struct TurboJpeg {
        library: "libturbojpeg",
        sonames: ["libturbojpeg.so.0"],
        headers: ["turbojpeg.h"],
        pkg_config: "libturbojpeg",
        package: "libturbojpeg0-dev",
        functions {
            fn tjInitDecompress() -> *mut c_void;
            fn tjDecompressHeader3(
                handle: *mut c_void,
                jpeg: *const u8,
                size: CUlong,
                width: *mut CInt,
                height: *mut CInt,
                subsampling: *mut CInt,
                colorspace: *mut CInt,
            ) -> CInt;
            fn tjDecompress2(
                handle: *mut c_void,
                jpeg: *const u8,
                size: CUlong,
                destination: *mut u8,
                width: CInt,
                pitch: CInt,
                height: CInt,
                pixel_format: CInt,
                flags: CInt,
            ) -> CInt;
            fn tjDestroy(handle: *mut c_void) -> CInt;
        }
        optional {
            /// Tells a warning from an error. libjpeg-turbo 2.0 added it.
            fn tjGetErrorCode(handle: *mut c_void) -> CInt;
        }
    }
}

/// A TurboJPEG decompressor, destroyed on drop.
struct Decompressor {
    tj: &'static TurboJpeg,
    handle: *mut c_void,
}

impl Drop for Decompressor {
    fn drop(&mut self) {
        // SAFETY: the handle came from `tjInitDecompress`, and nothing
        // uses it after the drop.
        unsafe { (self.tj.tjDestroy)(self.handle) };
    }
}

impl Decompressor {
    /// True when the last failure of the handle was only a warning, so
    /// the output is complete.
    fn only_warned(&self) -> bool {
        self.tj.tjGetErrorCode.is_some_and(|code| {
            // SAFETY: the handle came from `tjInitDecompress`.
            let code = unsafe { code(self.handle) };
            code == TJERR_WARNING
        })
    }
}

/// Decodes a JPEG image. Gray and YCbCr images give opaque RGBA, and
/// CMYK and YCCK images give inverted CMYK. The EXIF orientation is not
/// applied.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<Raster, CodecError> {
    let tj = TurboJpeg::get()?;
    // SAFETY: the call has no arguments.
    let handle = unsafe { (tj.tjInitDecompress)() };
    if handle.is_null() {
        return Err(CodecError::Invalid);
    }
    let decompressor = Decompressor { tj, handle };
    let size = CUlong::try_from(bytes.len()).map_err(|_| CodecError::Invalid)?;
    let (mut width, mut height, mut subsampling, mut colorspace) = (0, 0, 0, 0);
    // SAFETY: the handle is a live decompressor, `bytes` holds `size`
    // bytes, and each output points at a local integer.
    let header = unsafe {
        (tj.tjDecompressHeader3)(
            decompressor.handle,
            bytes.as_ptr(),
            size,
            &mut width,
            &mut height,
            &mut subsampling,
            &mut colorspace,
        )
    };
    if header != 0 {
        return Err(CodecError::Invalid);
    }
    let (width, height, count) = checked_size(width, height, limit)?;
    let pitch = width
        .checked_mul(4)
        .and_then(|pitch| CInt::try_from(pitch).ok())
        .ok_or(CodecError::TooLarge)?;
    let cmyk = colorspace == TJCS_CMYK || colorspace == TJCS_YCCK;
    let mut pixels = vec![0_u8; count * 4];
    // SAFETY: `pixels` holds `height` rows of `pitch` bytes, the size of
    // both 4-byte pixel formats. The width and height come from the
    // header, so they fit a C int.
    let result = unsafe {
        (tj.tjDecompress2)(
            decompressor.handle,
            bytes.as_ptr(),
            size,
            pixels.as_mut_ptr(),
            width as CInt,
            pitch,
            height as CInt,
            if cmyk { TJPF_CMYK } else { TJPF_RGBA },
            0,
        )
    };
    if result != 0 && !decompressor.only_warned() {
        return Err(CodecError::Invalid);
    }
    Ok(Raster {
        width,
        height,
        layout: if cmyk {
            PixelLayout::Cmyk
        } else {
            PixelLayout::Rgba
        },
        pixels,
    })
}
