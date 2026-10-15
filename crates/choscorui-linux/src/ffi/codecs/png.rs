//! libpng16 through its simplified API. That API catches the libpng
//! errors inside libpng and reports them by its result, so no error
//! long-jumps across a Rust frame.

use super::{CodecError, PixelLayout, Raster, checked_size};
use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::{c_consts, ffi_struct, libraries, opaque_types};

opaque_types! {
    /// `struct png_control`: the private state of a `png_image`.
    pub(crate) PngControl = "struct png_control";
    /// `png_color`: the crate passes only null background colors.
    pub(crate) PngColor = "png_color";
}

ffi_struct! {
    /// `png_image`: the state of one simplified read or write.
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct PngImage = "png_image" {
        pub(crate) opaque: *mut PngControl,
        pub(crate) version: u32,
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) format: u32,
        pub(crate) flags: u32,
        pub(crate) colormap_entries: u32,
        pub(crate) warning_or_error: u32,
        pub(crate) message: [CChar; 64],
    }
}

c_consts! {
    /// The constants of libpng, for the binding check.
    pub(crate) fn constants;
    const PNG_IMAGE_VERSION: u32 = 1;
    const PNG_FORMAT_RGBA: u32 = 3;
    const PNG_IMAGE_ERROR: u32 = 2;
}

libraries! {
    /// libpng16: PNG decoding and encoding.
    pub(crate) struct Png {
        library: "libpng",
        sonames: ["libpng16.so.16"],
        headers: ["png.h"],
        pkg_config: "libpng",
        package: "libpng-dev",
        functions {
            fn png_image_begin_read_from_memory(
                image: *mut PngImage,
                memory: *const c_void,
                size: usize,
            ) -> CInt;
            fn png_image_finish_read(
                image: *mut PngImage,
                background: *const PngColor,
                buffer: *mut c_void,
                row_stride: i32,
                colormap: *mut c_void,
            ) -> CInt;
            fn png_image_write_to_memory(
                image: *mut PngImage,
                memory: *mut c_void,
                memory_bytes: *mut usize,
                convert_to_8_bit: CInt,
                buffer: *const c_void,
                row_stride: i32,
                colormap: *const c_void,
            ) -> CInt;
            fn png_image_free(image: *mut PngImage);
        }
    }
}

/// One `png_image` with its libpng state, freed on drop.
struct Image {
    png: &'static Png,
    image: PngImage,
}

impl Image {
    fn new(png: &'static Png) -> Self {
        Self {
            png,
            image: PngImage {
                opaque: std::ptr::null_mut(),
                version: PNG_IMAGE_VERSION,
                width: 0,
                height: 0,
                format: 0,
                flags: 0,
                colormap_entries: 0,
                warning_or_error: 0,
                message: [CChar(0); 64],
            },
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: the image is initialized. `png_image_free` frees the
        // state of an unfinished read and does nothing when `opaque` is
        // null, after `png_image_finish_read` or a write.
        unsafe { (self.png.png_image_free)(&mut self.image) };
    }
}

/// The row stride of `width` RGBA pixels, in bytes.
fn stride(width: u32) -> Result<i32, CodecError> {
    width
        .checked_mul(4)
        .and_then(|stride| i32::try_from(stride).ok())
        .ok_or(CodecError::TooLarge)
}

/// Decodes a PNG image to straight-alpha sRGB RGBA. libpng converts the
/// gamma of the file to sRGB.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<Raster, CodecError> {
    let mut image = Image::new(Png::get()?);
    // SAFETY: the image is initialized with the version and a null
    // `opaque`. libpng reads `bytes.len()` bytes, which live until the
    // read finishes in this function.
    let begun = unsafe {
        (image.png.png_image_begin_read_from_memory)(
            &mut image.image,
            bytes.as_ptr().cast(),
            bytes.len(),
        )
    };
    if begun == 0 {
        return Err(CodecError::Invalid);
    }
    let (width, height, count) = checked_size(image.image.width, image.image.height, limit)?;
    let row_stride = stride(width)?;
    image.image.format = PNG_FORMAT_RGBA;
    let mut pixels = vec![0_u8; count * 4];
    // SAFETY: the read began. `pixels` holds `height` rows of
    // `row_stride` bytes, the size that the RGBA format needs. The
    // background and the colormap are null, which the RGBA format allows.
    let finished = unsafe {
        (image.png.png_image_finish_read)(
            &mut image.image,
            std::ptr::null(),
            pixels.as_mut_ptr().cast(),
            row_stride,
            std::ptr::null_mut(),
        )
    };
    if finished == 0 {
        return Err(CodecError::Invalid);
    }
    Ok(Raster {
        width,
        height,
        layout: PixelLayout::Rgba,
        pixels,
    })
}

/// Encodes `width * height` straight-alpha sRGB RGBA pixels as a PNG
/// file.
pub(crate) fn encode(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, CodecError> {
    let png = Png::get()?;
    let row_stride = stride(width)?;
    if pixels.len() != (width as usize * 4).saturating_mul(height as usize) {
        return Err(CodecError::Invalid);
    }
    // Deflate can make incompressible data a little larger. When the
    // first guess is too small, libpng gives the exact size.
    let mut capacity = pixels.len() + pixels.len() / 64 + 1024;
    for _ in 0..2 {
        let mut image = Image::new(png);
        image.image.width = width;
        image.image.height = height;
        image.image.format = PNG_FORMAT_RGBA;
        let mut output = vec![0_u8; capacity];
        let mut size = capacity;
        // SAFETY: the image is initialized for a write. `output` holds
        // `size` writable bytes, and `pixels` holds `height` rows of
        // `row_stride` bytes. The colormap is null, which the RGBA format
        // allows.
        let written = unsafe {
            (png.png_image_write_to_memory)(
                &mut image.image,
                output.as_mut_ptr().cast(),
                &mut size,
                0,
                pixels.as_ptr().cast(),
                row_stride,
                std::ptr::null(),
            )
        };
        if written != 0 && size <= output.len() {
            output.truncate(size);
            return Ok(output);
        }
        if image.image.warning_or_error & PNG_IMAGE_ERROR != 0 || size <= capacity {
            return Err(CodecError::Invalid);
        }
        capacity = size;
    }
    Err(CodecError::Invalid)
}
