//! The declarations of HarfBuzz, and safe owners of a font and a buffer.
//!
//! - `HbFont::from_face` makes the font of a FreeType face with
//!   `hb_ft_font_create_referenced`. The face is set to a large fixed
//!   size, and `HbFont::sized` makes a sub-font at the size of the text,
//!   so advances do not depend on how FreeType rounds a size.
//! - `HbBuffer` shapes one item and copies the glyphs out.

use super::ctypes::{CChar, CInt, CUint};
use super::freetype::{FT_LOAD_NO_HINTING, FtFace, FtFaceRec};
use super::macros::{ffi_enum, ffi_struct, libraries, opaque_types};
use std::ops::Range;

opaque_types! {
    /// `hb_font_t`.
    pub(crate) HbFontRaw = "hb_font_t";
    /// `hb_buffer_t`.
    pub(crate) HbBufferRaw = "hb_buffer_t";
    /// `hb_feature_t`, which the crate never passes.
    pub(crate) HbFeature = "hb_feature_t";
    /// `struct hb_language_impl_t`, the target of `hb_language_t`.
    pub(crate) HbLanguageImpl = "struct hb_language_impl_t";
}

ffi_enum! {
    /// `hb_direction_t`.
    pub(crate) enum HbDirection: u32 = "hb_direction_t" {
        HB_DIRECTION_LTR = 4,
        HB_DIRECTION_RTL = 5,
    }

    /// `hb_buffer_cluster_level_t`.
    pub(crate) enum HbClusterLevel: u32 = "hb_buffer_cluster_level_t" {
        HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES = 0,
    }
}

ffi_struct! {
    /// `hb_glyph_info_t`.
    pub(crate) struct HbGlyphInfo = "hb_glyph_info_t" {
        pub(crate) codepoint: u32,
        pub(crate) mask: u32,
        pub(crate) cluster: u32,
        pub(crate) var1: u32,
        pub(crate) var2: u32,
    }

    /// `hb_glyph_position_t`.
    pub(crate) struct HbGlyphPosition = "hb_glyph_position_t" {
        pub(crate) x_advance: i32,
        pub(crate) y_advance: i32,
        pub(crate) x_offset: i32,
        pub(crate) y_offset: i32,
        pub(crate) var: u32,
    }

    /// `hb_font_extents_t`.
    pub(crate) struct HbFontExtents = "hb_font_extents_t" {
        pub(crate) ascender: i32,
        pub(crate) descender: i32,
        pub(crate) line_gap: i32,
        pub(crate) reserved9: i32,
        pub(crate) reserved8: i32,
        pub(crate) reserved7: i32,
        pub(crate) reserved6: i32,
        pub(crate) reserved5: i32,
        pub(crate) reserved4: i32,
        pub(crate) reserved3: i32,
        pub(crate) reserved2: i32,
        pub(crate) reserved1: i32,
    }
}

libraries! {
    /// libharfbuzz: text shaping.
    pub(crate) struct HarfBuzz {
        library: "libharfbuzz",
        sonames: ["libharfbuzz.so.0"],
        headers: ["hb.h", "hb-ft.h"],
        pkg_config: "harfbuzz",
        package: "libharfbuzz-dev",
        functions {
            fn hb_ft_font_create_referenced(face: *mut FtFaceRec) -> *mut HbFontRaw;
            fn hb_ft_font_set_load_flags(font: *mut HbFontRaw, flags: CInt);
            fn hb_font_create_sub_font(parent: *mut HbFontRaw) -> *mut HbFontRaw;
            fn hb_font_set_scale(font: *mut HbFontRaw, x: CInt, y: CInt);
            fn hb_font_get_h_extents(font: *mut HbFontRaw, extents: *mut HbFontExtents) -> CInt;
            fn hb_font_destroy(font: *mut HbFontRaw);
            fn hb_buffer_create() -> *mut HbBufferRaw;
            fn hb_buffer_destroy(buffer: *mut HbBufferRaw);
            fn hb_buffer_clear_contents(buffer: *mut HbBufferRaw);
            fn hb_buffer_add_utf8(
                buffer: *mut HbBufferRaw,
                text: *const CChar,
                length: CInt,
                offset: CUint,
                item_length: CInt,
            );
            fn hb_buffer_set_direction(buffer: *mut HbBufferRaw, direction: HbDirection);
            fn hb_buffer_set_language(buffer: *mut HbBufferRaw, language: *const HbLanguageImpl);
            fn hb_buffer_set_cluster_level(buffer: *mut HbBufferRaw, level: HbClusterLevel);
            fn hb_buffer_guess_segment_properties(buffer: *mut HbBufferRaw);
            fn hb_language_from_string(text: *const CChar, length: CInt) -> *const HbLanguageImpl;
            fn hb_shape(
                font: *mut HbFontRaw,
                buffer: *mut HbBufferRaw,
                features: *const HbFeature,
                count: CUint,
            );
            fn hb_buffer_get_glyph_infos(
                buffer: *mut HbBufferRaw,
                length: *mut CUint,
            ) -> *mut HbGlyphInfo;
            fn hb_buffer_get_glyph_positions(
                buffer: *mut HbBufferRaw,
                length: *mut CUint,
            ) -> *mut HbGlyphPosition;
        }
    }
}

/// The size in pixels of the FreeType face behind each parent font.
/// Sub-fonts scale its advances to the size of the text.
const PARENT_PIXELS: f64 = 2048.0;

/// Positions in HarfBuzz units: 1/64 of a point.
const UNITS: f64 = 64.0;

/// An owned HarfBuzz font.
pub(crate) struct HbFont {
    raw: *mut HbFontRaw,
}

impl HbFont {
    /// The parent font of `face`, at a fixed large size without hinting.
    pub(crate) fn from_face(face: &FtFace) -> Option<Self> {
        let lib = HarfBuzz::get().ok()?;
        if !face.set_pixel_size(PARENT_PIXELS) {
            return None;
        }
        // SAFETY: the face is live; HarfBuzz takes its own reference.
        let raw = unsafe { (lib.hb_ft_font_create_referenced)(face.as_raw()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is a live FreeType-backed font.
        unsafe { (lib.hb_ft_font_set_load_flags)(raw, FT_LOAD_NO_HINTING) };
        Some(Self { raw })
    }

    /// A sub-font of `self` at `size` points.
    pub(crate) fn sized(&self, size: f64) -> Option<Self> {
        let lib = HarfBuzz::get().ok()?;
        // SAFETY: `self.raw` is live; the sub-font takes a reference.
        let raw = unsafe { (lib.hb_font_create_sub_font)(self.raw) };
        if raw.is_null() {
            return None;
        }
        let scale = (size * UNITS).round().clamp(1.0, f64::from(i32::MAX)) as CInt;
        // SAFETY: `raw` is a live font.
        unsafe { (lib.hb_font_set_scale)(raw, scale, scale) };
        Some(Self { raw })
    }

    /// Ascent, descent, and line gap in points, all positive down from
    /// the baseline except the ascent, which is positive up.
    pub(crate) fn extents(&self) -> Option<(f64, f64, f64)> {
        let lib = HarfBuzz::get().ok()?;
        let mut extents = HbFontExtents {
            ascender: 0,
            descender: 0,
            line_gap: 0,
            reserved9: 0,
            reserved8: 0,
            reserved7: 0,
            reserved6: 0,
            reserved5: 0,
            reserved4: 0,
            reserved3: 0,
            reserved2: 0,
            reserved1: 0,
        };
        // SAFETY: the font is live and the out-pointer is valid.
        let ok = unsafe { (lib.hb_font_get_h_extents)(self.raw, &raw mut extents) };
        (ok != 0).then(|| {
            (
                f64::from(extents.ascender) / UNITS,
                -f64::from(extents.descender) / UNITS,
                f64::from(extents.line_gap) / UNITS,
            )
        })
    }
}

impl Drop for HbFont {
    fn drop(&mut self) {
        let Ok(lib) = HarfBuzz::get() else {
            return;
        };
        // The last font of a face releases the face with `FT_Done_Face`.
        let _guard = super::freetype::lock();
        // SAFETY: `self` owns one reference to the live font.
        unsafe { (lib.hb_font_destroy)(self.raw) };
    }
}

/// One shaped glyph, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShapedGlyph {
    pub(crate) glyph: u32,
    /// Byte offset in the text of the first character of the cluster.
    pub(crate) cluster: usize,
    pub(crate) x_advance: f64,
    pub(crate) x_offset: f64,
    /// Offset up from the baseline.
    pub(crate) y_offset: f64,
}

/// What to shape: the text, the item inside it, and its properties.
pub(crate) struct ShapeRequest<'a> {
    /// The whole paragraph, which gives the item its context.
    pub(crate) text: &'a str,
    pub(crate) item: Range<usize>,
    pub(crate) right_to_left: bool,
    /// A BCP 47 tag.
    pub(crate) language: Option<&'a str>,
}

/// An owned HarfBuzz buffer.
pub(crate) struct HbBuffer {
    raw: *mut HbBufferRaw,
}

impl HbBuffer {
    pub(crate) fn new() -> Option<Self> {
        let lib = HarfBuzz::get().ok()?;
        // SAFETY: plain constructor; a failed allocation gives the inert
        // empty buffer, which is safe to use and destroy.
        let raw = unsafe { (lib.hb_buffer_create)() };
        (!raw.is_null()).then_some(Self { raw })
    }

    /// Shapes `request.item` with `font` and returns the glyphs in visual
    /// order, as HarfBuzz gives them.
    pub(crate) fn shape(&mut self, font: &HbFont, request: &ShapeRequest<'_>) -> Vec<ShapedGlyph> {
        let Ok(lib) = HarfBuzz::get() else {
            return Vec::new();
        };
        let (Ok(length), Ok(offset), Ok(item_length)) = (
            CInt::try_from(request.text.len()),
            CUint::try_from(request.item.start),
            CInt::try_from(request.item.len()),
        ) else {
            return Vec::new();
        };
        // SAFETY: the buffer is live.
        unsafe { (lib.hb_buffer_clear_contents)(self.raw) };
        // SAFETY: the text is valid UTF-8 of `length` bytes, and the item
        // lies inside it on character boundaries. HarfBuzz copies it.
        unsafe {
            (lib.hb_buffer_add_utf8)(
                self.raw,
                request.text.as_ptr().cast(),
                length,
                offset,
                item_length,
            )
        };
        let direction = if request.right_to_left {
            HbDirection::HB_DIRECTION_RTL
        } else {
            HbDirection::HB_DIRECTION_LTR
        };
        // SAFETY: the buffer is live.
        unsafe { (lib.hb_buffer_set_direction)(self.raw, direction) };
        // SAFETY: the buffer is live.
        unsafe {
            (lib.hb_buffer_set_cluster_level)(
                self.raw,
                HbClusterLevel::HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES,
            )
        };
        if let Some(tag) = request.language
            && let Ok(tag_length) = CInt::try_from(tag.len())
        {
            // SAFETY: the tag is `tag_length` bytes; HarfBuzz interns it
            // and returns a pointer that lives for the process.
            let language =
                unsafe { (lib.hb_language_from_string)(tag.as_ptr().cast(), tag_length) };
            if !language.is_null() {
                // SAFETY: the buffer is live and the language is interned.
                unsafe { (lib.hb_buffer_set_language)(self.raw, language) };
            }
        }
        // SAFETY: the buffer is live; it fills the unset script.
        unsafe { (lib.hb_buffer_guess_segment_properties)(self.raw) };
        // SAFETY: the font and the buffer are live; no features.
        unsafe { (lib.hb_shape)(font.raw, self.raw, std::ptr::null(), 0) };
        self.glyphs(lib)
    }

    /// Copies the glyphs out of the shaped buffer.
    fn glyphs(&self, lib: &HarfBuzz) -> Vec<ShapedGlyph> {
        let mut count: CUint = 0;
        // SAFETY: the buffer is live and the out-pointer is valid.
        let infos = unsafe { (lib.hb_buffer_get_glyph_infos)(self.raw, &raw mut count) };
        let mut position_count: CUint = 0;
        // SAFETY: as above.
        let positions =
            unsafe { (lib.hb_buffer_get_glyph_positions)(self.raw, &raw mut position_count) };
        if infos.is_null() || positions.is_null() || count != position_count {
            return Vec::new();
        }
        // SAFETY: HarfBuzz returns `count` infos, which live until the
        // buffer changes; they are copied before that.
        let infos = unsafe { std::slice::from_raw_parts(infos, count as usize) };
        // SAFETY: as above, for the positions.
        let positions = unsafe { std::slice::from_raw_parts(positions, count as usize) };
        infos
            .iter()
            .zip(positions)
            .map(|(info, position)| ShapedGlyph {
                glyph: info.codepoint,
                cluster: info.cluster as usize,
                x_advance: f64::from(position.x_advance) / UNITS,
                x_offset: f64::from(position.x_offset) / UNITS,
                y_offset: f64::from(position.y_offset) / UNITS,
            })
            .collect()
    }
}

impl Drop for HbBuffer {
    fn drop(&mut self) {
        if let Ok(lib) = HarfBuzz::get() {
            // SAFETY: `self` owns the live buffer.
            unsafe { (lib.hb_buffer_destroy)(self.raw) };
        }
    }
}
