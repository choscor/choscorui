//! Font creation shared by line shaping, text layout, and painting. The
//! generic family names `system-ui` and `ui-monospace` select the system
//! faces (SF Pro and SF Mono) at the requested weight; any other name is a
//! family that Core Text looks up.

#![deny(unsafe_code)]

use crate::ffi::appkit::{MONOSPACED_SYSTEM_FONT_OF_SIZE_WEIGHT, SYSTEM_FONT_OF_SIZE_WEIGHT};
use crate::ffi::cf::{CFNumber, CFString, CfRetained, cf_number, cf_string};
use crate::ffi::text::{
    CTFont, CTFontCreateCopyWithSymbolicTraits, CTFontCreateWithName, FONT_TRAIT_BOLD,
    FONT_TRAIT_ITALIC, kCTKernAttributeName,
};
use choscorui_core::{FontSlant, FontWeight};

/// The generic family that selects the system user-interface face.
pub(crate) const SYSTEM_UI: &str = "system-ui";
/// The generic family that selects the system monospaced face.
pub(crate) const SYSTEM_MONO: &str = "ui-monospace";

/// The AppKit `NSFontWeight` value nearest to a CSS weight.
fn appkit_weight(weight: FontWeight) -> f64 {
    match weight.0 {
        0..=149 => -0.8,
        150..=249 => -0.6,
        250..=349 => -0.4,
        350..=449 => 0.0,
        450..=549 => 0.23,
        550..=649 => 0.3,
        650..=749 => 0.4,
        750..=849 => 0.56,
        _ => 0.62,
    }
}

/// A CTFont for `family` at `size`, `weight`, and `slant`. A family
/// without the requested face keeps its regular face.
pub(crate) fn create_font(
    family: &str,
    size: f64,
    weight: FontWeight,
    slant: FontSlant,
) -> Option<CfRetained<CTFont>> {
    if family.is_empty() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    let mut font = match family {
        SYSTEM_UI | SYSTEM_MONO => {
            let message = if family == SYSTEM_UI {
                SYSTEM_FONT_OF_SIZE_WEIGHT
            } else {
                MONOSPACED_SYSTEM_FONT_OF_SIZE_WEIGHT
            };
            let font = message
                .send_class((size, appkit_weight(weight)))
                .ok()
                .flatten()?;
            CTFont::from_ns_font(font)
        }
        _ => {
            let name = cf_string(family)?;
            let font = CTFontCreateWithName(&name, size, None)?;
            apply_traits(font, weight >= FontWeight::SEMIBOLD, false)
        }
    };
    if slant == FontSlant::Italic {
        font = apply_traits(font, false, true);
    }
    Some(font)
}

/// Adds the bold and italic symbolic traits when the family has them.
fn apply_traits(font: CfRetained<CTFont>, bold: bool, italic: bool) -> CfRetained<CTFont> {
    let mut traits = 0;
    if bold {
        traits |= FONT_TRAIT_BOLD;
    }
    if italic {
        traits |= FONT_TRAIT_ITALIC;
    }
    if traits == 0 {
        return font;
    }
    CTFontCreateCopyWithSymbolicTraits(&font, 0.0, None, traits, traits).unwrap_or(font)
}

/// The kern attribute key and a CFNumber for `tracking` points, or `None`
/// when the tracking is zero.
pub(crate) fn kern_attribute(tracking: f64) -> Option<(&'static CFString, CfRetained<CFNumber>)> {
    if tracking == 0.0 || !tracking.is_finite() {
        return None;
    }
    Some((kCTKernAttributeName, cf_number(tracking)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_faces_resolve_at_every_weight() {
        for family in [SYSTEM_UI, SYSTEM_MONO, "Helvetica"] {
            for weight in [FontWeight::REGULAR, FontWeight::MEDIUM, FontWeight::BOLD] {
                let font = create_font(family, 14.0, weight, FontSlant::Upright);
                assert!(font.is_some(), "{family} {weight:?}");
            }
        }
        assert!(create_font("", 14.0, FontWeight::REGULAR, FontSlant::Upright).is_none());
        assert!(kern_attribute(0.0).is_none());
        assert!(kern_attribute(0.5).is_some());
    }
}
