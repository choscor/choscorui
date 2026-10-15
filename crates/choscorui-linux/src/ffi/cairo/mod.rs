//! A safe interface to Cairo for the CPU painter.
//!
//! - `Surface` owns an ARGB32 or RGB24 image surface. A surface on a
//!   caller's buffer borrows the buffer for its life.
//! - `Context` owns a `cairo_t` and borrows its target surface.
//! - `Pattern`, `FontFace`, and `FontOptions` own their Cairo objects.
//! - `FontFace::from_ft_face` gives Cairo a FreeType face. Cairo releases
//!   the face from the `font face destroy` callback, under the FreeType
//!   lock.

pub(crate) mod sys;

use super::ctypes::{CInt, CUlong, c_void};
use super::freetype::FtFace;
use super::macros::callback;
use choscorui_core::{FillRule, LineCap, LineJoin};
use std::marker::PhantomData;
use std::ops::Range;
use sys::{
    Cairo, CairoAntialias, CairoExtend, CairoFillRule, CairoFilter, CairoFontFaceRaw,
    CairoFontOptionsRaw, CairoFormat, CairoGlyph, CairoHintMetrics, CairoHintStyle, CairoLineCap,
    CairoLineJoin, CairoMatrix, CairoOperator, CairoPatternRaw, CairoRaw, CairoStatus,
    CairoSurfaceRaw, CairoUserDataKey,
};

/// Why a Cairo object was not made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CairoFailure {
    /// libcairo did not load.
    Library(super::dl::LoadError),
    /// The size or the stride is out of range for Cairo.
    BadSize,
    /// Cairo reported an error status, such as no memory.
    Status,
}

fn cairo() -> Result<&'static Cairo, CairoFailure> {
    Cairo::get().map_err(CairoFailure::Library)
}

/// The pixel format of a surface. ARGB32 stores one premultiplied
/// native-endian 32-bit word for each pixel, with blue in the low byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Format {
    Argb32,
}

impl Format {
    fn raw(self) -> CairoFormat {
        match self {
            Self::Argb32 => CairoFormat::CAIRO_FORMAT_ARGB32,
        }
    }

    /// The smallest row stride that Cairo accepts for `width` pixels.
    pub(crate) fn stride_for_width(self, width: u32) -> Option<usize> {
        let lib = cairo().ok()?;
        let width = CInt::try_from(width).ok()?;
        // SAFETY: plain computation on integers.
        let stride = unsafe { (lib.cairo_format_stride_for_width)(self.raw(), width) };
        usize::try_from(stride).ok()
    }
}

/// An owned image surface. `'a` is the borrow of a caller's buffer.
pub(crate) struct Surface<'a> {
    raw: *mut CairoSurfaceRaw,
    width: usize,
    height: usize,
    _data: PhantomData<&'a mut [u8]>,
}

impl Surface<'static> {
    /// A cleared surface of `width × height` pixels in memory that Cairo
    /// owns.
    pub(crate) fn new(format: Format, width: u32, height: u32) -> Result<Self, CairoFailure> {
        let lib = cairo()?;
        let (Ok(w), Ok(h)) = (CInt::try_from(width), CInt::try_from(height)) else {
            return Err(CairoFailure::BadSize);
        };
        // SAFETY: plain constructor; an error gives an inert surface whose
        // status is checked below.
        let raw = unsafe { (lib.cairo_image_surface_create)(format.raw(), w, h) };
        Self::checked(lib, raw, width as usize, height as usize)
    }

    /// An ARGB32 surface with a copy of premultiplied native-endian
    /// pixels, `width * 4` bytes in each row.
    pub(crate) fn from_pixels(
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Self, CairoFailure> {
        let surface = Self::new(Format::Argb32, width, height)?;
        let lib = cairo()?;
        let row = width as usize * 4;
        if pixels.len() < row * height as usize {
            return Err(CairoFailure::BadSize);
        }
        // SAFETY: the surface is live; nothing has drawn to it yet.
        unsafe { (lib.cairo_surface_flush)(surface.raw) };
        // SAFETY: the surface is a live image surface.
        let data = unsafe { (lib.cairo_image_surface_get_data)(surface.raw) };
        // SAFETY: as above.
        let stride = unsafe { (lib.cairo_image_surface_get_stride)(surface.raw) } as usize;
        if data.is_null() || stride < row {
            return Err(CairoFailure::Status);
        }
        // SAFETY: Cairo owns `stride * height` bytes at `data` while the
        // surface lives, and no other reference to them exists.
        let target = unsafe { std::slice::from_raw_parts_mut(data, stride * height as usize) };
        for (y, source) in pixels.chunks_exact(row).take(height as usize).enumerate() {
            target[y * stride..y * stride + row].copy_from_slice(source);
        }
        // SAFETY: the surface is live; the call tells Cairo of the write.
        unsafe { (lib.cairo_surface_mark_dirty)(surface.raw) };
        Ok(surface)
    }
}

impl<'a> Surface<'a> {
    /// A surface on `data`, which holds `height` rows of `stride` bytes.
    /// The stride must be one that Cairo accepts for the width.
    pub(crate) fn for_data(
        data: &'a mut [u8],
        format: Format,
        width: u32,
        height: u32,
        stride: usize,
    ) -> Result<Self, CairoFailure> {
        let lib = cairo()?;
        let minimum = format
            .stride_for_width(width)
            .ok_or(CairoFailure::BadSize)?;
        let fits = stride >= minimum
            && stride.is_multiple_of(4)
            && stride
                .checked_mul(height as usize)
                .is_some_and(|len| len <= data.len());
        let (Ok(w), Ok(h), Ok(s)) = (
            CInt::try_from(width),
            CInt::try_from(height),
            CInt::try_from(stride),
        ) else {
            return Err(CairoFailure::BadSize);
        };
        if !fits {
            return Err(CairoFailure::BadSize);
        }
        // SAFETY: `data` holds `stride * height` bytes and stays borrowed
        // for `'a`, which the surface cannot outlive.
        let raw = unsafe {
            (lib.cairo_image_surface_create_for_data)(data.as_mut_ptr(), format.raw(), w, h, s)
        };
        Self::checked(lib, raw, width as usize, height as usize)
    }

    fn checked(
        lib: &Cairo,
        raw: *mut CairoSurfaceRaw,
        width: usize,
        height: usize,
    ) -> Result<Self, CairoFailure> {
        let surface = Self {
            raw,
            width,
            height,
            _data: PhantomData,
        };
        // SAFETY: Cairo constructors never return null; the status of an
        // error surface reports the failure.
        let status = unsafe { (lib.cairo_surface_status)(raw) };
        if status == CairoStatus::CAIRO_STATUS_SUCCESS {
            Ok(surface)
        } else {
            Err(CairoFailure::Status)
        }
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    pub(crate) fn height(&self) -> usize {
        self.height
    }

    /// Copies the bytes `columns` (byte offsets in a row) of `rows` into
    /// `out`, after Cairo finishes the drawing in progress.
    pub(crate) fn copy_rows(&self, rows: Range<usize>, columns: Range<usize>, out: &mut Vec<u8>) {
        let Ok(lib) = cairo() else {
            return;
        };
        if rows.end > self.height || columns.end > self.width * 4 || columns.start > columns.end {
            return;
        }
        // SAFETY: the surface is live.
        unsafe { (lib.cairo_surface_flush)(self.raw) };
        // SAFETY: the surface is a live image surface.
        let data = unsafe { (lib.cairo_image_surface_get_data)(self.raw) };
        // SAFETY: as above.
        let stride = unsafe { (lib.cairo_image_surface_get_stride)(self.raw) } as usize;
        if data.is_null() {
            return;
        }
        // SAFETY: the surface memory holds `stride * height` bytes while it
        // lives; after the flush, Cairo does not write it during the read.
        let bytes = unsafe { std::slice::from_raw_parts(data, stride * self.height) };
        for row in rows {
            out.extend_from_slice(&bytes[row * stride + columns.start..row * stride + columns.end]);
        }
    }
}

impl Drop for Surface<'_> {
    fn drop(&mut self) {
        if let Ok(lib) = cairo() {
            // SAFETY: the surface finishes writing before it is freed, so
            // a caller's buffer is complete when the borrow ends.
            unsafe { (lib.cairo_surface_flush)(self.raw) };
            // SAFETY: `self` owns one reference to the live surface.
            unsafe { (lib.cairo_surface_destroy)(self.raw) };
        }
    }
}

/// Image sampling of a surface pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Filter {
    Nearest,
    Bilinear,
}

/// An owned pattern: a gradient, or an image that keeps its surface.
pub(crate) struct Pattern {
    raw: *mut CairoPatternRaw,
}

impl Pattern {
    fn checked(lib: &Cairo, raw: *mut CairoPatternRaw) -> Option<Self> {
        let pattern = Self { raw };
        // SAFETY: Cairo constructors never return null; an error pattern
        // reports its status.
        let status = unsafe { (lib.cairo_pattern_status)(raw) };
        (status == CairoStatus::CAIRO_STATUS_SUCCESS).then_some(pattern)
    }

    /// A linear gradient from `start` to `end`, padded with the end
    /// colors.
    pub(crate) fn linear(start: (f64, f64), end: (f64, f64)) -> Option<Self> {
        let lib = cairo().ok()?;
        // SAFETY: plain constructor.
        let raw = unsafe { (lib.cairo_pattern_create_linear)(start.0, start.1, end.0, end.1) };
        let pattern = Self::checked(lib, raw)?;
        pattern.set_extend(lib, CairoExtend::CAIRO_EXTEND_PAD);
        Some(pattern)
    }

    /// A radial gradient from `center` out to `radius`, padded with the
    /// end colors.
    pub(crate) fn radial(center: (f64, f64), radius: f64) -> Option<Self> {
        let lib = cairo().ok()?;
        // SAFETY: plain constructor.
        let raw = unsafe {
            (lib.cairo_pattern_create_radial)(center.0, center.1, 0.0, center.0, center.1, radius)
        };
        let pattern = Self::checked(lib, raw)?;
        pattern.set_extend(lib, CairoExtend::CAIRO_EXTEND_PAD);
        Some(pattern)
    }

    /// An image pattern of `surface` at the origin. Outside the image,
    /// `pad` repeats the edge pixels; otherwise it is transparent.
    pub(crate) fn image(surface: &Surface<'static>, filter: Filter, pad: bool) -> Option<Self> {
        let lib = cairo().ok()?;
        // SAFETY: the surface is live; the pattern takes a reference.
        let raw = unsafe { (lib.cairo_pattern_create_for_surface)(surface.raw) };
        let pattern = Self::checked(lib, raw)?;
        let filter = match filter {
            Filter::Nearest => CairoFilter::CAIRO_FILTER_NEAREST,
            Filter::Bilinear => CairoFilter::CAIRO_FILTER_BILINEAR,
        };
        // SAFETY: the pattern is live.
        unsafe { (lib.cairo_pattern_set_filter)(pattern.raw, filter) };
        pattern.set_extend(
            lib,
            if pad {
                CairoExtend::CAIRO_EXTEND_PAD
            } else {
                CairoExtend::CAIRO_EXTEND_NONE
            },
        );
        Some(pattern)
    }

    fn set_extend(&self, lib: &Cairo, extend: CairoExtend) {
        // SAFETY: the pattern is live.
        unsafe { (lib.cairo_pattern_set_extend)(self.raw, extend) };
    }

    /// Adds a gradient stop with straight-alpha color channels in 0..=1.
    pub(crate) fn add_stop(&self, offset: f64, [red, green, blue, alpha]: [f64; 4]) {
        if let Ok(lib) = cairo() {
            // SAFETY: the pattern is live.
            unsafe {
                (lib.cairo_pattern_add_color_stop_rgba)(self.raw, offset, red, green, blue, alpha)
            };
        }
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        if let Ok(lib) = cairo() {
            // SAFETY: `self` owns one reference to the live pattern.
            unsafe { (lib.cairo_pattern_destroy)(self.raw) };
        }
    }
}

/// How Cairo hints glyph outlines.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum HintStyle {
    None,
    Slight,
    Medium,
    Full,
}

/// Owned font options: grayscale anti-aliasing, the hint style, and no
/// hinting of metrics, so glyphs stay at their shaped positions.
pub(crate) struct FontOptions {
    raw: *mut CairoFontOptionsRaw,
}

impl FontOptions {
    pub(crate) fn new(hint: HintStyle) -> Option<Self> {
        let lib = cairo().ok()?;
        // SAFETY: plain constructor; an error gives an inert object.
        let raw = unsafe { (lib.cairo_font_options_create)() };
        if raw.is_null() {
            return None;
        }
        let options = Self { raw };
        // SAFETY: the options are live.
        unsafe {
            (lib.cairo_font_options_set_antialias)(raw, CairoAntialias::CAIRO_ANTIALIAS_GRAY)
        };
        let style = match hint {
            HintStyle::None => CairoHintStyle::CAIRO_HINT_STYLE_NONE,
            HintStyle::Slight => CairoHintStyle::CAIRO_HINT_STYLE_SLIGHT,
            HintStyle::Medium => CairoHintStyle::CAIRO_HINT_STYLE_MEDIUM,
            HintStyle::Full => CairoHintStyle::CAIRO_HINT_STYLE_FULL,
        };
        // SAFETY: the options are live.
        unsafe { (lib.cairo_font_options_set_hint_style)(raw, style) };
        // SAFETY: the options are live.
        unsafe {
            (lib.cairo_font_options_set_hint_metrics)(raw, CairoHintMetrics::CAIRO_HINT_METRICS_OFF)
        };
        Some(options)
    }
}

impl Drop for FontOptions {
    fn drop(&mut self) {
        if let Ok(lib) = cairo() {
            // SAFETY: `self` owns the live options.
            unsafe { (lib.cairo_font_options_destroy)(self.raw) };
        }
    }
}

/// The key of the FreeType face in the user data of a Cairo font face.
static FACE_KEY: CairoUserDataKey = CairoUserDataKey { unused: 0 };

callback! {
    /// Releases the FreeType face of a Cairo font face that Cairo frees.
    ///
    /// # Safety
    ///
    /// Cairo passes the face pointer that `FontFace::from_ft_face` stored,
    /// one time.
    fn release_ft_face(data: *mut c_void)
        [boundary "cairo font face destroy", fallback ()]
    {
        // SAFETY: the user data owns one reference to the face.
        unsafe { super::freetype::release_face(data.cast()) }
    }
}

/// An owned Cairo font face on a FreeType face.
pub(crate) struct FontFace {
    raw: *mut CairoFontFaceRaw,
}

impl FontFace {
    /// A Cairo font face that owns `face`. Cairo hints with the font
    /// options of the context.
    pub(crate) fn from_ft_face(face: FtFace) -> Option<Self> {
        let lib = cairo().ok()?;
        // SAFETY: the face is live; Cairo keeps the pointer, and the user
        // data below keeps the face alive until Cairo frees the font face.
        let raw = unsafe { (lib.cairo_ft_font_face_create_for_ft_face)(face.as_raw(), 0) };
        let font = Self { raw };
        // SAFETY: Cairo constructors never return null.
        let status = unsafe { (lib.cairo_font_face_status)(raw) };
        if status != CairoStatus::CAIRO_STATUS_SUCCESS {
            return None;
        }
        let data = face.into_raw();
        // SAFETY: the font face is live; the key is a static, and the
        // callback releases the face reference that `data` now holds.
        let status = unsafe {
            (lib.cairo_font_face_set_user_data)(
                raw,
                &raw const FACE_KEY,
                data.cast(),
                Some(release_ft_face),
            )
        };
        if status != CairoStatus::CAIRO_STATUS_SUCCESS {
            drop(font);
            // SAFETY: Cairo did not take the reference, so it is released
            // here, after the font face that used it.
            unsafe { super::freetype::release_face(data) };
            return None;
        }
        Some(font)
    }
}

impl Drop for FontFace {
    fn drop(&mut self) {
        if let Ok(lib) = cairo() {
            // SAFETY: `self` owns one reference to the live font face.
            unsafe { (lib.cairo_font_face_destroy)(self.raw) };
        }
    }
}

/// An affine transform in the order of `cairo_matrix_t`:
/// `[xx, yx, xy, yy, x0, y0]`.
pub(crate) type Matrix = [f64; 6];

/// An owned drawing context on a surface.
pub(crate) struct Context<'s> {
    raw: *mut CairoRaw,
    lib: &'static Cairo,
    _target: PhantomData<&'s Surface<'s>>,
}

impl<'s> Context<'s> {
    pub(crate) fn new(target: &'s Surface<'_>) -> Result<Self, CairoFailure> {
        let lib = cairo()?;
        // SAFETY: the surface is live and outlives the context.
        let raw = unsafe { (lib.cairo_create)(target.raw) };
        let context = Self {
            raw,
            lib,
            _target: PhantomData,
        };
        if context.ok() {
            Ok(context)
        } else {
            Err(CairoFailure::Status)
        }
    }

    /// True while the context has no error.
    pub(crate) fn ok(&self) -> bool {
        // SAFETY: the context is live.
        let status = unsafe { (self.lib.cairo_status)(self.raw) };
        status == CairoStatus::CAIRO_STATUS_SUCCESS
    }

    pub(crate) fn save(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_save)(self.raw) }
    }

    pub(crate) fn restore(&self) {
        // SAFETY: the context is live; the painter balances each save.
        unsafe { (self.lib.cairo_restore)(self.raw) }
    }

    /// Starts an offscreen group; it also saves the state.
    pub(crate) fn push_group(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_push_group)(self.raw) }
    }

    /// Ends the group, restores the state, and paints the group with
    /// `alpha`.
    pub(crate) fn pop_group_with_alpha(&self, alpha: f64) {
        // SAFETY: the context is live, and a group is open.
        unsafe { (self.lib.cairo_pop_group_to_source)(self.raw) }
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_paint_with_alpha)(self.raw, alpha) }
    }

    pub(crate) fn scale(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_scale)(self.raw, x, y) }
    }

    pub(crate) fn translate(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_translate)(self.raw, x, y) }
    }

    /// Applies `matrix` before the current transform.
    pub(crate) fn transform(&self, [xx, yx, xy, yy, x0, y0]: Matrix) {
        let matrix = CairoMatrix {
            xx,
            yx,
            xy,
            yy,
            x0,
            y0,
        };
        // SAFETY: the context is live and the matrix is valid for the call.
        unsafe { (self.lib.cairo_transform)(self.raw, &raw const matrix) }
    }

    /// The transform from user space to device space.
    pub(crate) fn matrix(&self) -> Matrix {
        let mut matrix = CairoMatrix {
            xx: 1.0,
            yx: 0.0,
            xy: 0.0,
            yy: 1.0,
            x0: 0.0,
            y0: 0.0,
        };
        // SAFETY: the context is live and the out-pointer is valid.
        unsafe { (self.lib.cairo_get_matrix)(self.raw, &raw mut matrix) };
        [
            matrix.xx, matrix.yx, matrix.xy, matrix.yy, matrix.x0, matrix.y0,
        ]
    }

    /// Makes user space equal to device space.
    pub(crate) fn identity_matrix(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_identity_matrix)(self.raw) }
    }

    /// Selects the clear operator (`true`) or the over operator.
    pub(crate) fn set_clear(&self, clear: bool) {
        let op = if clear {
            CairoOperator::CAIRO_OPERATOR_CLEAR
        } else {
            CairoOperator::CAIRO_OPERATOR_OVER
        };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_operator)(self.raw, op) }
    }

    /// Sets a straight-alpha source color with channels in 0..=1.
    pub(crate) fn set_source_rgba(&self, [red, green, blue, alpha]: [f64; 4]) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_source_rgba)(self.raw, red, green, blue, alpha) }
    }

    /// Sets the source; its space is the current user space.
    pub(crate) fn set_source(&self, pattern: &Pattern) {
        // SAFETY: both are live; the context takes a reference.
        unsafe { (self.lib.cairo_set_source)(self.raw, pattern.raw) }
    }

    /// Sets `surface` as the source with its origin at `(x, y)`.
    pub(crate) fn set_source_surface(&self, surface: &Surface<'static>, x: f64, y: f64) {
        // SAFETY: both are live; the context takes a reference.
        unsafe { (self.lib.cairo_set_source_surface)(self.raw, surface.raw, x, y) }
    }

    pub(crate) fn set_fill_rule(&self, rule: FillRule) {
        let rule = match rule {
            FillRule::NonZero => CairoFillRule::CAIRO_FILL_RULE_WINDING,
            FillRule::EvenOdd => CairoFillRule::CAIRO_FILL_RULE_EVEN_ODD,
        };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_fill_rule)(self.raw, rule) }
    }

    /// Sets the line width, cap, join, miter limit, and dash pattern.
    pub(crate) fn set_stroke(&self, width: f64, cap: LineCap, join: LineJoin, dash: &[f64]) {
        let cap = match cap {
            LineCap::Butt => CairoLineCap::CAIRO_LINE_CAP_BUTT,
            LineCap::Round => CairoLineCap::CAIRO_LINE_CAP_ROUND,
            LineCap::Square => CairoLineCap::CAIRO_LINE_CAP_SQUARE,
        };
        let join = match join {
            LineJoin::Miter => CairoLineJoin::CAIRO_LINE_JOIN_MITER,
            LineJoin::Round => CairoLineJoin::CAIRO_LINE_JOIN_ROUND,
            LineJoin::Bevel => CairoLineJoin::CAIRO_LINE_JOIN_BEVEL,
        };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_line_width)(self.raw, width) };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_line_cap)(self.raw, cap) };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_line_join)(self.raw, join) };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_miter_limit)(self.raw, choscorui_core::MITER_LIMIT) };
        let count = CInt::try_from(dash.len()).unwrap_or(0);
        // SAFETY: the context is live and `dash` holds `count` values,
        // which Cairo copies.
        unsafe { (self.lib.cairo_set_dash)(self.raw, dash.as_ptr(), count, 0.0) };
    }

    pub(crate) fn new_path(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_new_path)(self.raw) }
    }

    pub(crate) fn move_to(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_move_to)(self.raw, x, y) }
    }

    pub(crate) fn line_to(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_line_to)(self.raw, x, y) }
    }

    pub(crate) fn curve_to(&self, a: (f64, f64), b: (f64, f64), end: (f64, f64)) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_curve_to)(self.raw, a.0, a.1, b.0, b.1, end.0, end.1) }
    }

    pub(crate) fn close_path(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_close_path)(self.raw) }
    }

    pub(crate) fn rectangle(&self, x: f64, y: f64, width: f64, height: f64) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_rectangle)(self.raw, x, y, width, height) }
    }

    /// Fills the path with the source and clears it.
    pub(crate) fn fill(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_fill)(self.raw) }
    }

    /// Strokes the path with the source and clears it.
    pub(crate) fn stroke(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_stroke)(self.raw) }
    }

    /// Intersects the clip with the path and clears it.
    pub(crate) fn clip(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_clip)(self.raw) }
    }

    /// Paints the source everywhere inside the clip.
    pub(crate) fn paint(&self) {
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_paint)(self.raw) }
    }

    /// Draws `glyphs` (glyph id and user-space origin) in `face` at `size`
    /// with the source.
    pub(crate) fn show_glyphs(
        &self,
        face: &FontFace,
        size: f64,
        options: &FontOptions,
        glyphs: &[(u32, f64, f64)],
    ) {
        let glyphs: Vec<CairoGlyph> = glyphs
            .iter()
            .map(|&(index, x, y)| CairoGlyph {
                index: CUlong::from(index),
                x,
                y,
            })
            .collect();
        let Ok(count) = CInt::try_from(glyphs.len()) else {
            return;
        };
        // SAFETY: the context and the face are live; the context takes a
        // reference to the face.
        unsafe { (self.lib.cairo_set_font_face)(self.raw, face.raw) };
        // SAFETY: the context is live.
        unsafe { (self.lib.cairo_set_font_size)(self.raw, size) };
        // SAFETY: the context and the options are live; Cairo copies them.
        unsafe { (self.lib.cairo_set_font_options)(self.raw, options.raw) };
        // SAFETY: the context is live and `glyphs` holds `count` glyphs.
        unsafe { (self.lib.cairo_show_glyphs)(self.raw, glyphs.as_ptr(), count) };
    }
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        // The context can hold the last reference to a font face, whose
        // release frees a FreeType face.
        // SAFETY: `self` owns one reference to the live context.
        unsafe { (self.lib.cairo_destroy)(self.raw) }
    }
}
