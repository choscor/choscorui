//! The Linux text system: HarfBuzz shaping on FreeType faces, with
//! fontconfig matching and fallback.
//!
//! `LinuxTextShaper` implements the core `TextShaper` contract. It keeps
//! no state of its own, so a `static` can hold it; faces and fallback
//! choices are cached for each thread.

mod bidi;
mod breaks;
pub(crate) mod fonts;
mod shape;

use crate::ffi::{fontconfig, freetype::FreeType, harfbuzz::HarfBuzz};
use choscorui_core::{Support, TextLayout, TextLayoutError, TextShaper, TextStyle};

pub use fonts::set_desktop_font;

/// Shapes text with HarfBuzz on FreeType faces that fontconfig chooses.
/// Pair it with [`choscorui_core::TextLayoutCache`] to reuse layouts.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxTextShaper;

impl TextShaper for LinuxTextShaper {
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError> {
        shape::shape_text(text, style, max_width)
    }
}

/// `Supported` when FreeType, HarfBuzz, and fontconfig load.
pub fn text_shaping_support() -> Support {
    if FreeType::available() && HarfBuzz::available() && fontconfig::available() {
        Support::Supported
    } else {
        Support::Unsupported
    }
}

/// The installed font family names, sorted. Hidden families (names that
/// start with `.`) are left out.
pub fn available_font_families() -> Vec<String> {
    fontconfig::families()
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .collect()
}
