//! The declarations of libcairo, with its FreeType font backend.

use crate::ffi::ctypes::{CInt, CUlong, c_void};
use crate::ffi::freetype::FtFaceRec;
use crate::ffi::macros::{ffi_enum, ffi_struct, libraries, opaque_types};

opaque_types! {
    /// `cairo_t`.
    pub(crate) CairoRaw = "cairo_t";
    /// `cairo_surface_t`.
    pub(crate) CairoSurfaceRaw = "cairo_surface_t";
    /// `cairo_pattern_t`.
    pub(crate) CairoPatternRaw = "cairo_pattern_t";
    /// `cairo_font_face_t`.
    pub(crate) CairoFontFaceRaw = "cairo_font_face_t";
    /// `cairo_font_options_t`.
    pub(crate) CairoFontOptionsRaw = "cairo_font_options_t";
}

ffi_struct! {
    /// `cairo_matrix_t`: `x' = xx * x + xy * y + x0`,
    /// `y' = yx * x + yy * y + y0`.
    pub(crate) struct CairoMatrix = "cairo_matrix_t" {
        pub(crate) xx: f64,
        pub(crate) yx: f64,
        pub(crate) xy: f64,
        pub(crate) yy: f64,
        pub(crate) x0: f64,
        pub(crate) y0: f64,
    }

    /// `cairo_glyph_t`.
    pub(crate) struct CairoGlyph = "cairo_glyph_t" {
        pub(crate) index: CUlong,
        pub(crate) x: f64,
        pub(crate) y: f64,
    }

    /// `cairo_user_data_key_t`. Only its address matters.
    pub(crate) struct CairoUserDataKey = "cairo_user_data_key_t" {
        pub(crate) unused: CInt,
    }
}

ffi_enum! {
    /// `cairo_status_t`.
    pub(crate) enum CairoStatus: u32 = "cairo_status_t" {
        CAIRO_STATUS_SUCCESS = 0,
    }

    /// `cairo_format_t`.
    pub(crate) enum CairoFormat: i32 = "cairo_format_t" {
        CAIRO_FORMAT_ARGB32 = 0,
        CAIRO_FORMAT_RGB24 = 1,
    }

    /// `cairo_operator_t`.
    pub(crate) enum CairoOperator: u32 = "cairo_operator_t" {
        CAIRO_OPERATOR_CLEAR = 0,
        CAIRO_OPERATOR_OVER = 2,
    }

    /// `cairo_fill_rule_t`.
    pub(crate) enum CairoFillRule: u32 = "cairo_fill_rule_t" {
        CAIRO_FILL_RULE_WINDING = 0,
        CAIRO_FILL_RULE_EVEN_ODD = 1,
    }

    /// `cairo_line_cap_t`.
    pub(crate) enum CairoLineCap: u32 = "cairo_line_cap_t" {
        CAIRO_LINE_CAP_BUTT = 0,
        CAIRO_LINE_CAP_ROUND = 1,
        CAIRO_LINE_CAP_SQUARE = 2,
    }

    /// `cairo_line_join_t`.
    pub(crate) enum CairoLineJoin: u32 = "cairo_line_join_t" {
        CAIRO_LINE_JOIN_MITER = 0,
        CAIRO_LINE_JOIN_ROUND = 1,
        CAIRO_LINE_JOIN_BEVEL = 2,
    }

    /// `cairo_filter_t`.
    pub(crate) enum CairoFilter: u32 = "cairo_filter_t" {
        CAIRO_FILTER_NEAREST = 3,
        CAIRO_FILTER_BILINEAR = 4,
    }

    /// `cairo_extend_t`.
    pub(crate) enum CairoExtend: u32 = "cairo_extend_t" {
        CAIRO_EXTEND_NONE = 0,
        CAIRO_EXTEND_PAD = 3,
    }

    /// `cairo_antialias_t`.
    pub(crate) enum CairoAntialias: u32 = "cairo_antialias_t" {
        CAIRO_ANTIALIAS_GRAY = 2,
    }

    /// `cairo_hint_style_t`.
    pub(crate) enum CairoHintStyle: u32 = "cairo_hint_style_t" {
        CAIRO_HINT_STYLE_NONE = 1,
        CAIRO_HINT_STYLE_SLIGHT = 2,
        CAIRO_HINT_STYLE_MEDIUM = 3,
        CAIRO_HINT_STYLE_FULL = 4,
    }

    /// `cairo_hint_metrics_t`.
    pub(crate) enum CairoHintMetrics: u32 = "cairo_hint_metrics_t" {
        CAIRO_HINT_METRICS_OFF = 1,
    }
}

/// `cairo_destroy_func_t`.
pub(crate) type CairoDestroyFunc = Option<unsafe extern "C" fn(*mut c_void)>;

libraries! {
    /// libcairo: the CPU painter.
    pub(crate) struct Cairo {
        library: "libcairo",
        sonames: ["libcairo.so.2"],
        headers: ["cairo.h", "cairo-ft.h"],
        pkg_config: "cairo-ft",
        package: "libcairo2-dev",
        functions {
            fn cairo_format_stride_for_width(format: CairoFormat, width: CInt) -> CInt;
            fn cairo_image_surface_create(format: CairoFormat, width: CInt, height: CInt) -> *mut CairoSurfaceRaw;
            fn cairo_image_surface_create_for_data(
                data: *mut u8,
                format: CairoFormat,
                width: CInt,
                height: CInt,
                stride: CInt,
            ) -> *mut CairoSurfaceRaw;
            fn cairo_image_surface_get_data(surface: *mut CairoSurfaceRaw) -> *mut u8;
            fn cairo_image_surface_get_stride(surface: *mut CairoSurfaceRaw) -> CInt;
            fn cairo_surface_status(surface: *mut CairoSurfaceRaw) -> CairoStatus;
            fn cairo_surface_flush(surface: *mut CairoSurfaceRaw);
            fn cairo_surface_mark_dirty(surface: *mut CairoSurfaceRaw);
            fn cairo_surface_destroy(surface: *mut CairoSurfaceRaw);
            fn cairo_create(target: *mut CairoSurfaceRaw) -> *mut CairoRaw;
            fn cairo_destroy(cr: *mut CairoRaw);
            fn cairo_status(cr: *mut CairoRaw) -> CairoStatus;
            fn cairo_save(cr: *mut CairoRaw);
            fn cairo_restore(cr: *mut CairoRaw);
            fn cairo_push_group(cr: *mut CairoRaw);
            fn cairo_pop_group_to_source(cr: *mut CairoRaw);
            fn cairo_scale(cr: *mut CairoRaw, x: f64, y: f64);
            fn cairo_translate(cr: *mut CairoRaw, x: f64, y: f64);
            fn cairo_transform(cr: *mut CairoRaw, matrix: *const CairoMatrix);
            fn cairo_get_matrix(cr: *mut CairoRaw, matrix: *mut CairoMatrix);
            fn cairo_identity_matrix(cr: *mut CairoRaw);
            fn cairo_set_operator(cr: *mut CairoRaw, op: CairoOperator);
            fn cairo_set_source_rgba(cr: *mut CairoRaw, red: f64, green: f64, blue: f64, alpha: f64);
            fn cairo_set_source(cr: *mut CairoRaw, source: *mut CairoPatternRaw);
            fn cairo_set_source_surface(cr: *mut CairoRaw, surface: *mut CairoSurfaceRaw, x: f64, y: f64);
            fn cairo_set_fill_rule(cr: *mut CairoRaw, rule: CairoFillRule);
            fn cairo_set_line_width(cr: *mut CairoRaw, width: f64);
            fn cairo_set_line_cap(cr: *mut CairoRaw, cap: CairoLineCap);
            fn cairo_set_line_join(cr: *mut CairoRaw, join: CairoLineJoin);
            fn cairo_set_miter_limit(cr: *mut CairoRaw, limit: f64);
            fn cairo_set_dash(cr: *mut CairoRaw, dashes: *const f64, count: CInt, offset: f64);
            fn cairo_new_path(cr: *mut CairoRaw);
            fn cairo_move_to(cr: *mut CairoRaw, x: f64, y: f64);
            fn cairo_line_to(cr: *mut CairoRaw, x: f64, y: f64);
            fn cairo_curve_to(cr: *mut CairoRaw, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64);
            fn cairo_close_path(cr: *mut CairoRaw);
            fn cairo_rectangle(cr: *mut CairoRaw, x: f64, y: f64, width: f64, height: f64);
            fn cairo_fill(cr: *mut CairoRaw);
            fn cairo_stroke(cr: *mut CairoRaw);
            fn cairo_clip(cr: *mut CairoRaw);
            fn cairo_paint(cr: *mut CairoRaw);
            fn cairo_paint_with_alpha(cr: *mut CairoRaw, alpha: f64);
            fn cairo_pattern_create_linear(x0: f64, y0: f64, x1: f64, y1: f64) -> *mut CairoPatternRaw;
            fn cairo_pattern_create_radial(
                cx0: f64,
                cy0: f64,
                radius0: f64,
                cx1: f64,
                cy1: f64,
                radius1: f64,
            ) -> *mut CairoPatternRaw;
            fn cairo_pattern_create_for_surface(surface: *mut CairoSurfaceRaw) -> *mut CairoPatternRaw;
            fn cairo_pattern_add_color_stop_rgba(
                pattern: *mut CairoPatternRaw,
                offset: f64,
                red: f64,
                green: f64,
                blue: f64,
                alpha: f64,
            );
            fn cairo_pattern_set_filter(pattern: *mut CairoPatternRaw, filter: CairoFilter);
            fn cairo_pattern_set_extend(pattern: *mut CairoPatternRaw, extend: CairoExtend);
            fn cairo_pattern_status(pattern: *mut CairoPatternRaw) -> CairoStatus;
            fn cairo_pattern_destroy(pattern: *mut CairoPatternRaw);
            fn cairo_set_font_face(cr: *mut CairoRaw, face: *mut CairoFontFaceRaw);
            fn cairo_set_font_size(cr: *mut CairoRaw, size: f64);
            fn cairo_set_font_options(cr: *mut CairoRaw, options: *const CairoFontOptionsRaw);
            fn cairo_show_glyphs(cr: *mut CairoRaw, glyphs: *const CairoGlyph, count: CInt);
            fn cairo_font_options_create() -> *mut CairoFontOptionsRaw;
            fn cairo_font_options_destroy(options: *mut CairoFontOptionsRaw);
            fn cairo_font_options_set_antialias(options: *mut CairoFontOptionsRaw, antialias: CairoAntialias);
            fn cairo_font_options_set_hint_style(options: *mut CairoFontOptionsRaw, style: CairoHintStyle);
            fn cairo_font_options_set_hint_metrics(options: *mut CairoFontOptionsRaw, metrics: CairoHintMetrics);
            fn cairo_ft_font_face_create_for_ft_face(face: *mut FtFaceRec, load_flags: CInt) -> *mut CairoFontFaceRaw;
            fn cairo_font_face_status(face: *mut CairoFontFaceRaw) -> CairoStatus;
            fn cairo_font_face_set_user_data(
                face: *mut CairoFontFaceRaw,
                key: *const CairoUserDataKey,
                data: *mut c_void,
                destroy: CairoDestroyFunc,
            ) -> CairoStatus;
            fn cairo_font_face_destroy(face: *mut CairoFontFaceRaw);
        }
    }
}

/// The constants of the enums of this module, for the binding check.
#[cfg(test)]
pub(crate) fn constants() -> Vec<crate::ffi::checks::ConstItem> {
    [
        CairoStatus::constants(),
        CairoFormat::constants(),
        CairoOperator::constants(),
        CairoFillRule::constants(),
        CairoLineCap::constants(),
        CairoLineJoin::constants(),
        CairoFilter::constants(),
        CairoExtend::constants(),
        CairoAntialias::constants(),
        CairoHintStyle::constants(),
        CairoHintMetrics::constants(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
