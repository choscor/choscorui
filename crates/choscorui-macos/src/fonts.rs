//! Font creation shared by line shaping, text layout, and painting. The
//! generic family names `system-ui` and `ui-monospace` select the system
//! faces (SF Pro and SF Mono) at the requested weight; any other name is a
//! family that Core Text looks up.

use super::*;
use choscorui_core::{FontSlant, FontWeight};

const FONT_TRAIT_ITALIC: u32 = 1 << 0;
const FONT_TRAIT_BOLD: u32 = 1 << 1;
const NUMBER_FLOAT64: isize = 6;

/// The generic family that selects the system user-interface face.
pub(crate) const SYSTEM_UI: &str = "system-ui";
/// The generic family that selects the system monospaced face.
pub(crate) const SYSTEM_MONO: &str = "ui-monospace";

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTKernAttributeName: Id;
    fn CTFontCreateCopyWithSymbolicTraits(
        font: Id,
        size: f64,
        matrix: Id,
        value: u32,
        mask: u32,
    ) -> Id;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(object: *const c_void) -> *const c_void;
    fn CFNumberCreate(allocator: *const c_void, kind: isize, value: *const c_void) -> Id;
}

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

/// A +1 CTFont for `family` at `size`, `weight`, and `slant`. A family
/// without the requested face keeps its regular face.
pub(crate) unsafe fn create_font(
    family: &str,
    size: f64,
    weight: FontWeight,
    slant: FontSlant,
) -> Option<CfOwned> {
    if family.is_empty() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    // SAFETY: Each NSFont class method returns an autoreleased font, which
    // CFRetain turns into a +1 reference; NSFont is toll-free bridged to
    // CTFont. CTFontCreateWithName returns a +1 font or null.
    unsafe {
        let mut font = match family {
            SYSTEM_UI | SYSTEM_MONO => {
                let selector_name = if family == SYSTEM_UI {
                    c"systemFontOfSize:weight:"
                } else {
                    c"monospacedSystemFontOfSize:weight:"
                };
                let class = class(c"NSFont").ok()?;
                let send: unsafe extern "C" fn(Id, Sel, f64, f64) -> Id =
                    std::mem::transmute(objc_msgSend as *const ());
                let font = send(class, selector(selector_name), size, appkit_weight(weight));
                if font.is_null() {
                    return None;
                }
                CfOwned::new(CFRetain(font).cast_mut())?
            }
            _ => {
                let name = cf_utf8(family)?;
                let font = CfOwned::new(CTFontCreateWithName(
                    name.as_ptr(),
                    size,
                    std::ptr::null_mut(),
                ))?;
                apply_traits(font, weight >= FontWeight::SEMIBOLD, false)
            }
        };
        if slant == FontSlant::Italic {
            font = apply_traits(font, false, true);
        }
        Some(font)
    }
}

/// Adds the bold and italic symbolic traits when the family has them.
unsafe fn apply_traits(font: CfOwned, bold: bool, italic: bool) -> CfOwned {
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
    // SAFETY: `font` is a live CTFont; the copy is +1 or null.
    unsafe {
        CfOwned::new(CTFontCreateCopyWithSymbolicTraits(
            font.as_ptr(),
            0.0,
            std::ptr::null_mut(),
            traits,
            traits,
        ))
        .unwrap_or(font)
    }
}

/// The kern attribute key and a +1 CFNumber for `tracking` points, or
/// `None` when the tracking is zero.
pub(crate) unsafe fn kern_attribute(tracking: f64) -> Option<(Id, CfOwned)> {
    if tracking == 0.0 || !tracking.is_finite() {
        return None;
    }
    // SAFETY: CFNumberCreate copies the f64 that the pointer reads.
    unsafe {
        let number = CfOwned::new(CFNumberCreate(
            std::ptr::null(),
            NUMBER_FLOAT64,
            (&raw const tracking).cast(),
        ))?;
        Some((kCTKernAttributeName, number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_faces_resolve_at_every_weight() {
        for family in [SYSTEM_UI, SYSTEM_MONO, "Helvetica"] {
            for weight in [FontWeight::REGULAR, FontWeight::MEDIUM, FontWeight::BOLD] {
                // SAFETY: the returned font is owned and released on drop.
                let font = unsafe { create_font(family, 14.0, weight, FontSlant::Upright) };
                assert!(font.is_some(), "{family} {weight:?}");
            }
        }
        // SAFETY: as above.
        assert!(
            unsafe { create_font("", 14.0, FontWeight::REGULAR, FontSlant::Upright) }.is_none()
        );
        assert!(unsafe { kern_attribute(0.0) }.is_none());
        assert!(unsafe { kern_attribute(0.5) }.is_some());
    }
}
