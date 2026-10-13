//! Core Text types and functions.

use super::appkit::NSFont;
use super::cf::{CFArray, CFAttributedString, CFDictionary, CFString, CFType, CfRetained, cf_type};
use super::geometry::{CGAffineTransform, NSPoint, NSSize};
use super::graphics::CGContext;
use super::retained::Retained;
use std::ffi::c_void;
use std::ops::Range;

cf_type!(
    CTFont,
    CTFontDescriptor,
    CTLine,
    CTTypesetter,
    CTRun,
    CTParagraphStyle,
);

/// `CTFontSymbolicTraits`: `kCTFontTraitItalic`.
pub(crate) const FONT_TRAIT_ITALIC: u32 = 1 << 0;
/// `CTFontSymbolicTraits`: `kCTFontTraitBold`.
pub(crate) const FONT_TRAIT_BOLD: u32 = 1 << 1;
/// `kCTParagraphStyleSpecifierBaseWritingDirection`.
const PARAGRAPH_BASE_WRITING_DIRECTION: u32 = 13;
/// `kCTRunStatusRightToLeft`.
const RUN_STATUS_RIGHT_TO_LEFT: u32 = 1;

/// `CFRange`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct CFRange {
    pub(crate) location: isize,
    pub(crate) length: isize,
}

/// `CTParagraphStyleSetting`.
#[repr(C)]
struct CTParagraphStyleSetting {
    spec: u32,
    value_size: usize,
    value: *const c_void,
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C-unwind" {
    pub(crate) safe static kCTFontAttributeName: &'static CFString;
    pub(crate) safe static kCTForegroundColorFromContextAttributeName: &'static CFString;
    pub(crate) safe static kCTKernAttributeName: &'static CFString;
    pub(crate) safe static kCTLanguageAttributeName: &'static CFString;
    pub(crate) safe static kCTParagraphStyleAttributeName: &'static CFString;
    pub(crate) safe static kCTFontCascadeListAttribute: &'static CFString;
    pub(crate) safe static kCTFontFamilyNameAttribute: &'static CFString;

    safe fn CTFontGetTypeID() -> usize;
    pub(crate) safe fn CTFontCreateWithName(
        name: &CFString,
        size: f64,
        matrix: Option<&CGAffineTransform>,
    ) -> Option<CfRetained<CTFont>>;
    pub(crate) safe fn CTFontCreateCopyWithSymbolicTraits(
        font: &CTFont,
        size: f64,
        matrix: Option<&CGAffineTransform>,
        value: u32,
        mask: u32,
    ) -> Option<CfRetained<CTFont>>;
    pub(crate) safe fn CTFontCopyFontDescriptor(
        font: &CTFont,
    ) -> Option<CfRetained<CTFontDescriptor>>;
    pub(crate) safe fn CTFontDescriptorCreateWithAttributes(
        attributes: &CFDictionary,
    ) -> Option<CfRetained<CTFontDescriptor>>;
    pub(crate) safe fn CTFontDescriptorCreateCopyWithAttributes(
        descriptor: &CTFontDescriptor,
        attributes: &CFDictionary,
    ) -> Option<CfRetained<CTFontDescriptor>>;
    pub(crate) safe fn CTFontCreateWithFontDescriptor(
        descriptor: &CTFontDescriptor,
        size: f64,
        matrix: Option<&CGAffineTransform>,
    ) -> Option<CfRetained<CTFont>>;
    pub(crate) safe fn CTFontCopyPostScriptName(font: &CTFont) -> Option<CfRetained<CFString>>;
    pub(crate) safe fn CTFontGetSize(font: &CTFont) -> f64;
    pub(crate) safe fn CTFontGetAscent(font: &CTFont) -> f64;
    pub(crate) safe fn CTFontGetDescent(font: &CTFont) -> f64;
    pub(crate) safe fn CTFontGetLeading(font: &CTFont) -> f64;
    fn CTFontDrawGlyphs(
        font: &CTFont,
        glyphs: *const u16,
        positions: *const NSPoint,
        count: usize,
        context: &CGContext,
    );
    pub(crate) safe fn CTFontManagerCopyAvailableFontFamilyNames()
    -> Option<CfRetained<CFArray<CFString>>>;
    fn CTParagraphStyleCreate(
        settings: *const CTParagraphStyleSetting,
        count: usize,
    ) -> Option<CfRetained<CTParagraphStyle>>;

    pub(crate) safe fn CTLineCreateWithAttributedString(
        string: &CFAttributedString,
    ) -> Option<CfRetained<CTLine>>;
    pub(crate) safe fn CTLineDraw(line: &CTLine, context: &CGContext);
    pub(crate) safe fn CTLineGetTypographicBounds(
        line: &CTLine,
        ascent: &mut f64,
        descent: &mut f64,
        leading: &mut f64,
    ) -> f64;
    pub(crate) safe fn CTLineGetOffsetForStringIndex(
        line: &CTLine,
        index: isize,
        secondary: &mut f64,
    ) -> f64;
    pub(crate) safe fn CTLineGetTrailingWhitespaceWidth(line: &CTLine) -> f64;
    pub(crate) safe fn CTLineGetPenOffsetForFlush(line: &CTLine, flush: f64, width: f64) -> f64;
    pub(crate) safe fn CTLineGetGlyphRuns(line: &CTLine) -> &CFArray<CTRun>;

    pub(crate) safe fn CTTypesetterCreateWithAttributedString(
        string: &CFAttributedString,
    ) -> Option<CfRetained<CTTypesetter>>;
    pub(crate) safe fn CTTypesetterSuggestLineBreak(
        typesetter: &CTTypesetter,
        start: isize,
        width: f64,
    ) -> isize;
    pub(crate) safe fn CTTypesetterCreateLine(
        typesetter: &CTTypesetter,
        range: CFRange,
    ) -> Option<CfRetained<CTLine>>;

    safe fn CTRunGetGlyphCount(run: &CTRun) -> isize;
    fn CTRunGetGlyphs(run: &CTRun, range: CFRange, buffer: *mut u16);
    fn CTRunGetPositions(run: &CTRun, range: CFRange, buffer: *mut NSPoint);
    fn CTRunGetAdvances(run: &CTRun, range: CFRange, buffer: *mut NSSize);
    fn CTRunGetStringIndices(run: &CTRun, range: CFRange, buffer: *mut isize);
    safe fn CTRunGetStatus(run: &CTRun) -> u32;
    safe fn CTRunGetAttributes(run: &CTRun) -> &CFDictionary;
}

impl CTFont {
    /// The font of a toll-free bridged `NSFont`.
    pub(crate) fn from_ns_font(font: Retained<NSFont>) -> CfRetained<CTFont> {
        // SAFETY: NSFont is toll-free bridged to CTFont, so the +1 object
        // reference is a +1 Core Foundation reference.
        unsafe { CfRetained::from_raw(font.into_raw().cast()) }
    }

    /// `object` as a font, or `None` when it is another type.
    pub(crate) fn downcast(object: &CFType) -> Option<&CTFont> {
        // SAFETY: the type identifier says that the object is a font.
        (object.type_id() == CTFontGetTypeID())
            .then(|| unsafe { &*std::ptr::from_ref(object).cast::<CTFont>() })
    }

    /// Draws `glyphs` at `positions` in `context`. Extra glyphs or
    /// positions past the shorter of the two are ignored.
    pub(crate) fn draw_glyphs(&self, glyphs: &[u16], positions: &[NSPoint], context: &CGContext) {
        let count = glyphs.len().min(positions.len());
        // SAFETY: both arrays hold at least `count` values.
        unsafe { CTFontDrawGlyphs(self, glyphs.as_ptr(), positions.as_ptr(), count, context) }
    }
}

/// A new paragraph style with a base writing direction.
pub(crate) fn paragraph_style(right_to_left: bool) -> Option<CfRetained<CTParagraphStyle>> {
    let direction: i8 = right_to_left.into();
    let setting = CTParagraphStyleSetting {
        spec: PARAGRAPH_BASE_WRITING_DIRECTION,
        value_size: size_of::<i8>(),
        value: (&raw const direction).cast(),
    };
    // SAFETY: the one setting points to a live `CTWritingDirection` byte,
    // which Core Text copies.
    unsafe { CTParagraphStyleCreate(&setting, 1) }
}

/// The glyphs of one run, copied out of Core Text.
pub(crate) struct RunGlyphs {
    pub(crate) glyphs: Vec<u16>,
    pub(crate) positions: Vec<NSPoint>,
    pub(crate) advances: Vec<NSSize>,
    pub(crate) string_indices: Vec<isize>,
}

impl CTRun {
    /// A copy of the glyphs, positions, advances, and string indices.
    pub(crate) fn glyphs(&self) -> RunGlyphs {
        let count = usize::try_from(CTRunGetGlyphCount(self)).unwrap_or(0);
        let mut glyphs = RunGlyphs {
            glyphs: vec![0; count],
            positions: vec![NSPoint::default(); count],
            advances: vec![NSSize::default(); count],
            string_indices: vec![0; count],
        };
        if count == 0 {
            return glyphs;
        }
        let all = CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: each buffer holds the run's `count` values, and the empty
        // range copies all of them.
        unsafe { CTRunGetGlyphs(self, all, glyphs.glyphs.as_mut_ptr()) };
        // SAFETY: as above.
        unsafe { CTRunGetPositions(self, all, glyphs.positions.as_mut_ptr()) };
        // SAFETY: as above.
        unsafe { CTRunGetAdvances(self, all, glyphs.advances.as_mut_ptr()) };
        // SAFETY: as above.
        unsafe { CTRunGetStringIndices(self, all, glyphs.string_indices.as_mut_ptr()) };
        glyphs
    }

    /// True when the run reads right to left.
    pub(crate) fn is_right_to_left(&self) -> bool {
        CTRunGetStatus(self) & RUN_STATUS_RIGHT_TO_LEFT != 0
    }

    /// The font that Core Text chose for the run.
    pub(crate) fn font(&self) -> Option<&CTFont> {
        CTRunGetAttributes(self)
            .get(kCTFontAttributeName)
            .and_then(CTFont::downcast)
    }
}

/// The UTF-16 range `range` as a `CFRange`.
pub(crate) fn cf_range(range: Range<usize>) -> Option<CFRange> {
    Some(CFRange {
        location: isize::try_from(range.start).ok()?,
        length: isize::try_from(range.end.checked_sub(range.start)?).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::cf::{CFAttributedString, cf_attributed_string, cf_dictionary, cf_string};

    fn attributed(text: &str) -> CfRetained<CFAttributedString> {
        let name = cf_string("Helvetica").unwrap();
        let font = CTFontCreateWithName(&name, 12.0, None).unwrap();
        let attributes = cf_dictionary(&[kCTFontAttributeName], &[&font]).unwrap();
        let string = cf_string(text).unwrap();
        cf_attributed_string(&string, &attributes).unwrap()
    }

    #[test]
    fn runs_copy_their_glyphs_and_font() {
        let line = CTLineCreateWithAttributedString(&attributed("abc")).unwrap();
        let runs = CTLineGetGlyphRuns(&line);
        let run = runs.get(0).unwrap();
        let glyphs = run.glyphs();
        assert_eq!(glyphs.glyphs.len(), 3);
        assert_eq!(glyphs.string_indices, [0, 1, 2]);
        assert!(!run.is_right_to_left());
        let font = run.font().unwrap();
        assert_eq!(CTFontGetSize(font), 12.0);
        assert!(paragraph_style(true).is_some());
        assert!(CTFont::downcast(&cf_string("x").unwrap()).is_none());
    }
}
