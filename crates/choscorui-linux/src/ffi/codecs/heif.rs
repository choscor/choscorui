//! libheif: HEIC and HEIF decoding. Each call returns its error as a
//! `struct heif_error` value.

use super::{CodecError, PixelLayout, Raster, checked_size};
use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::{ffi_enum, ffi_struct, libraries, opaque_types};

opaque_types! {
    /// `struct heif_context`: a parsed file.
    pub(crate) HeifContext = "struct heif_context";
    /// `struct heif_image_handle`: an image of a file, not yet decoded.
    pub(crate) HeifImageHandle = "struct heif_image_handle";
    /// `struct heif_image`: decoded pixels.
    pub(crate) HeifImage = "struct heif_image";
    /// `struct heif_reading_options`: the crate passes only null.
    pub(crate) HeifReadingOptions = "struct heif_reading_options";
    /// `struct heif_decoding_options`: the crate passes only null, so
    /// libheif applies the rotation and mirror transformations.
    pub(crate) HeifDecodingOptions = "struct heif_decoding_options";
}

ffi_enum! {
    /// `enum heif_error_code`.
    pub(crate) enum HeifErrorCode: CInt = "enum heif_error_code" {
        #[allow(non_upper_case_globals, reason = "the constant keeps its C name")]
        heif_error_Ok = 0,
    }

    /// `enum heif_colorspace`.
    pub(crate) enum HeifColorspace: CInt = "enum heif_colorspace" {
        #[allow(non_upper_case_globals, reason = "the constant keeps its C name")]
        heif_colorspace_RGB = 1,
    }

    /// `enum heif_chroma`.
    pub(crate) enum HeifChroma: CInt = "enum heif_chroma" {
        #[allow(non_upper_case_globals, reason = "the constant keeps its C name")]
        heif_chroma_interleaved_RGBA = 11,
    }

    /// `enum heif_channel`.
    pub(crate) enum HeifChannel: CInt = "enum heif_channel" {
        #[allow(non_upper_case_globals, reason = "the constant keeps its C name")]
        heif_channel_interleaved = 10,
    }
}

ffi_struct! {
    /// `struct heif_error`: the result of a libheif call.
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only the code")]
    pub(crate) struct HeifError = "struct heif_error" {
        pub(crate) code: HeifErrorCode,
        pub(crate) subcode: CInt,
        pub(crate) message: *const CChar,
    }
}

impl HeifError {
    fn check(self) -> Result<(), CodecError> {
        if self.code == HeifErrorCode::heif_error_Ok {
            Ok(())
        } else {
            Err(CodecError::Invalid)
        }
    }
}

libraries! {
    /// libheif: HEIC and HEIF decoding.
    pub(crate) struct Heif {
        library: "libheif",
        sonames: ["libheif.so.1"],
        headers: ["libheif/heif.h"],
        pkg_config: "libheif",
        package: "libheif-dev",
        functions {
            fn heif_context_alloc() -> *mut HeifContext;
            fn heif_context_free(context: *mut HeifContext);
            fn heif_context_read_from_memory_without_copy(
                context: *mut HeifContext,
                memory: *const c_void,
                size: usize,
                options: *const HeifReadingOptions,
            ) -> HeifError;
            fn heif_context_get_primary_image_handle(
                context: *mut HeifContext,
                handle: *mut *mut HeifImageHandle,
            ) -> HeifError;
            fn heif_image_handle_get_width(handle: *const HeifImageHandle) -> CInt;
            fn heif_image_handle_get_height(handle: *const HeifImageHandle) -> CInt;
            fn heif_image_handle_release(handle: *const HeifImageHandle);
            fn heif_decode_image(
                handle: *const HeifImageHandle,
                image: *mut *mut HeifImage,
                colorspace: HeifColorspace,
                chroma: HeifChroma,
                options: *const HeifDecodingOptions,
            ) -> HeifError;
            fn heif_image_get_width(image: *const HeifImage, channel: HeifChannel) -> CInt;
            fn heif_image_get_height(image: *const HeifImage, channel: HeifChannel) -> CInt;
            fn heif_image_get_plane_readonly(
                image: *const HeifImage,
                channel: HeifChannel,
                stride: *mut CInt,
            ) -> *const u8;
            fn heif_image_release(image: *const HeifImage);
        }
        optional {
            fn heif_image_handle_is_premultiplied_alpha(handle: *const HeifImageHandle) -> CInt;
        }
    }
}

/// A context, freed on drop.
struct Context {
    heif: &'static Heif,
    context: *mut HeifContext,
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the context came from `heif_context_alloc`, and its
        // handles and images dropped before it.
        unsafe { (self.heif.heif_context_free)(self.context) };
    }
}

/// An image handle, released on drop.
struct Handle {
    heif: &'static Heif,
    handle: *mut HeifImageHandle,
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: libheif gave the handle, and nothing uses it after the
        // drop.
        unsafe { (self.heif.heif_image_handle_release)(self.handle) };
    }
}

/// Decoded pixels, released on drop.
struct Image {
    heif: &'static Heif,
    image: *mut HeifImage,
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: `heif_decode_image` gave the image, and nothing uses it
        // after the drop.
        unsafe { (self.heif.heif_image_release)(self.image) };
    }
}

/// Decodes the primary image of a HEIF file to RGBA. libheif applies the
/// rotation and mirror properties of the file.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<Raster, CodecError> {
    let heif = Heif::get()?;
    // SAFETY: the call has no arguments.
    let context = unsafe { (heif.heif_context_alloc)() };
    if context.is_null() {
        return Err(CodecError::Invalid);
    }
    let context = Context { heif, context };
    // SAFETY: the context is live. libheif keeps a pointer to `bytes`,
    // which outlive the context: it drops before this function returns.
    unsafe {
        (heif.heif_context_read_from_memory_without_copy)(
            context.context,
            bytes.as_ptr().cast(),
            bytes.len(),
            std::ptr::null(),
        )
    }
    .check()?;
    let mut handle = std::ptr::null_mut();
    // SAFETY: the context read the file, and `handle` is a local pointer.
    unsafe { (heif.heif_context_get_primary_image_handle)(context.context, &mut handle) }
        .check()?;
    if handle.is_null() {
        return Err(CodecError::Invalid);
    }
    let handle = Handle { heif, handle };
    // SAFETY: the handle is live.
    let width = unsafe { (heif.heif_image_handle_get_width)(handle.handle) };
    // SAFETY: the handle is live.
    let height = unsafe { (heif.heif_image_handle_get_height)(handle.handle) };
    checked_size(width, height, limit)?;
    let premultiplied = heif
        .heif_image_handle_is_premultiplied_alpha
        .is_some_and(|call| {
            // SAFETY: the handle is live.
            let value = unsafe { call(handle.handle) };
            value != 0
        });
    let mut image = std::ptr::null_mut();
    // SAFETY: the handle is live, `image` is a local pointer, and null
    // options select the defaults.
    unsafe {
        (heif.heif_decode_image)(
            handle.handle,
            &mut image,
            HeifColorspace::heif_colorspace_RGB,
            HeifChroma::heif_chroma_interleaved_RGBA,
            std::ptr::null(),
        )
    }
    .check()?;
    if image.is_null() {
        return Err(CodecError::Invalid);
    }
    let image = Image { heif, image };
    let channel = HeifChannel::heif_channel_interleaved;
    // SAFETY: the image is live.
    let width = unsafe { (heif.heif_image_get_width)(image.image, channel) };
    // SAFETY: the image is live.
    let height = unsafe { (heif.heif_image_get_height)(image.image, channel) };
    let (width, height, count) = checked_size(width, height, limit)?;
    let mut stride = 0;
    // SAFETY: the image is live, and `stride` is a local integer.
    let plane = unsafe { (heif.heif_image_get_plane_readonly)(image.image, channel, &mut stride) };
    let stride = usize::try_from(stride).map_err(|_| CodecError::Invalid)?;
    let row = width as usize * 4;
    if plane.is_null() || stride < row {
        return Err(CodecError::Invalid);
    }
    let length = stride * (height as usize - 1) + row;
    // SAFETY: the interleaved plane holds `height` rows of `stride`
    // bytes, and it lives until `image` drops.
    let plane = unsafe { std::slice::from_raw_parts(plane, length) };
    let mut pixels = Vec::with_capacity(count * 4);
    for y in 0..height as usize {
        pixels.extend_from_slice(&plane[y * stride..y * stride + row]);
    }
    Ok(Raster {
        width,
        height,
        layout: if premultiplied {
            PixelLayout::PremultipliedRgba
        } else {
            PixelLayout::Rgba
        },
        pixels,
    })
}
