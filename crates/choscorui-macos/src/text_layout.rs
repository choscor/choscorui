//! Core Text implementation of the core text-layout contract. Shaping,
//! fallback, bidi, and line breaking come from Core Text; the result is
//! copied into owned glyph runs that both measurement and painting use.

use super::*;
use crate::ffi::cf::{
    CFType, CfRetained, cf_array, cf_attributed_string, cf_dictionary, cf_string, kCFBooleanTrue,
};
use crate::ffi::graphics::{
    CGContext, CGContextGetTextMatrix, CGContextRestoreGState, CGContextSaveGState,
    CGContextScaleCTM, CGContextSetTextMatrix, CGContextTranslateCTM,
};
use crate::ffi::text::{
    CTFont, CTFontCopyFontDescriptor, CTFontCopyPostScriptName, CTFontCreateWithFontDescriptor,
    CTFontCreateWithName, CTFontDescriptorCreateCopyWithAttributes,
    CTFontDescriptorCreateWithAttributes, CTFontGetAscent, CTFontGetDescent, CTFontGetLeading,
    CTFontGetSize, CTFontManagerCopyAvailableFontFamilyNames, CTLine, CTLineGetGlyphRuns,
    CTLineGetOffsetForStringIndex, CTLineGetPenOffsetForFlush, CTLineGetTrailingWhitespaceWidth,
    CTLineGetTypographicBounds, CTTypesetterCreateLine, CTTypesetterCreateWithAttributedString,
    CTTypesetterSuggestLineBreak, cf_range, kCTFontAttributeName, kCTFontCascadeListAttribute,
    kCTFontFamilyNameAttribute, kCTForegroundColorFromContextAttributeName,
    kCTLanguageAttributeName, kCTParagraphStyleAttributeName, paragraph_style,
};
use choscorui_core::{
    CaretStop, GlyphRun, LayoutLine, TextLayout, TextLayoutError, TextShaper, TextStyle,
    grapheme_boundaries,
};

/// Grapheme boundary tables for one paragraph, so every position mapping
/// is a lookup or binary search instead of a rescan.
struct Boundaries {
    bytes: Vec<usize>,
    utf16: Vec<usize>,
}

impl Boundaries {
    fn new(text: &str) -> Self {
        let bytes = grapheme_boundaries(text);
        let mut utf16 = Vec::with_capacity(bytes.len());
        let (mut units, mut previous) = (0, 0);
        for &byte in &bytes {
            units += text[previous..byte].encode_utf16().count();
            utf16.push(units);
            previous = byte;
        }
        Self { bytes, utf16 }
    }
    fn count(&self) -> usize {
        self.bytes.len() - 1
    }
    /// Grapheme that contains UTF-16 unit `unit`.
    fn grapheme_at_utf16(&self, unit: usize) -> usize {
        match self.utf16.binary_search(&unit) {
            Ok(index) => index.min(self.count().saturating_sub(1)),
            Err(index) => index.saturating_sub(1),
        }
    }
}

/// Installed font family names, sorted by the system for display. Hidden
/// system families (names starting with `.`) are omitted.
pub fn available_font_families() -> Vec<String> {
    let Some(array) = CTFontManagerCopyAvailableFontFamilyNames() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|name| name.to_rust())
        .filter(|name| !name.starts_with('.'))
        .collect()
}

/// The CTFont for `style`: the face from [`fonts::create_font`], then the
/// style's fallback cascade.
fn create_font(style: &TextStyle) -> Result<CfRetained<CTFont>, TextLayoutError> {
    let unavailable = TextLayoutError::Unavailable;
    let font = fonts::create_font(&style.family, style.size, style.weight, style.slant)
        .ok_or(unavailable)?;
    if style.fallback.is_empty() {
        return Ok(font);
    }
    let mut descriptors = Vec::new();
    for family in &style.fallback {
        let family = cf_string(family).ok_or(unavailable)?;
        let attributes =
            cf_dictionary(&[kCTFontFamilyNameAttribute], &[&family]).ok_or(unavailable)?;
        descriptors.push(CTFontDescriptorCreateWithAttributes(&attributes).ok_or(unavailable)?);
    }
    let borrowed: Vec<_> = descriptors.iter().map(|descriptor| &**descriptor).collect();
    let cascade = cf_array(&borrowed).ok_or(unavailable)?;
    let attributes =
        cf_dictionary(&[kCTFontCascadeListAttribute], &[&cascade]).ok_or(unavailable)?;
    let base = CTFontCopyFontDescriptor(&font).ok_or(unavailable)?;
    let descriptor =
        CTFontDescriptorCreateCopyWithAttributes(&base, &attributes).ok_or(unavailable)?;
    CTFontCreateWithFontDescriptor(&descriptor, style.size, None).ok_or(unavailable)
}

/// Shapes text with Core Text. Stateless; pair it with
/// [`choscorui_core::TextLayoutCache`] to reuse layouts.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreTextShaper;

impl TextShaper for CoreTextShaper {
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError> {
        style.validate()?;
        if max_width.is_some_and(|w| !w.is_finite() || w <= 0.0) {
            return Err(TextLayoutError::InvalidWidth);
        }
        let source = grapheme_boundaries(text);
        let lines = {
            let font = create_font(style)?;
            let mut lines = Vec::new();
            let mut top = 0.0;
            let mut paragraph_start = 0usize;
            for raw in text.split('\n') {
                let start = paragraph_start;
                paragraph_start += raw.len() + 1;
                let paragraph = raw.strip_suffix('\r').unwrap_or(raw);
                let first = source
                    .binary_search(&start)
                    .map_err(|_| TextLayoutError::Unavailable)?;
                shape_paragraph(
                    paragraph, start, first, &font, style, max_width, &mut top, &mut lines,
                )?;
            }
            lines
        };
        TextLayout::from_lines(text, style.clone(), max_width, lines)
    }
}

#[allow(clippy::too_many_arguments)]
fn shape_paragraph(
    paragraph: &str,
    byte_start: usize,
    grapheme_start: usize,
    font: &CTFont,
    style: &TextStyle,
    max_width: Option<f64>,
    top: &mut f64,
    lines: &mut Vec<LayoutLine>,
) -> Result<(), TextLayoutError> {
    let unavailable = TextLayoutError::Unavailable;
    let rtl = style.direction.is_rtl_for(paragraph);
    if paragraph.is_empty() {
        let (ascent, descent, leading) = (
            CTFontGetAscent(font),
            CTFontGetDescent(font),
            CTFontGetLeading(font),
        );
        let offset_x = if rtl { max_width.unwrap_or(0.0) } else { 0.0 };
        lines.push(LayoutLine {
            bytes: byte_start..byte_start,
            graphemes: grapheme_start..grapheme_start,
            baseline: *top + ascent,
            ascent,
            descent,
            leading,
            width: 0.0,
            offset_x,
            right_to_left: rtl,
            runs: Vec::new(),
            carets: vec![CaretStop {
                primary: 0.0,
                secondary: 0.0,
            }],
            extents: Vec::new(),
        });
        *top += (ascent + descent + leading) * style.line_height;
        return Ok(());
    }
    let string = cf_string(paragraph).ok_or(unavailable)?;
    let paragraph_style = paragraph_style(rtl).ok_or(unavailable)?;
    let mut keys = vec![
        kCTFontAttributeName,
        kCTForegroundColorFromContextAttributeName,
        kCTParagraphStyleAttributeName,
    ];
    let mut values: Vec<&CFType> = vec![font, kCFBooleanTrue, &paragraph_style];
    let language = match &style.locale {
        Some(tag) => Some(cf_string(tag).ok_or(unavailable)?),
        None => None,
    };
    if let Some(language) = &language {
        keys.push(kCTLanguageAttributeName);
        values.push(language);
    }
    let kern = fonts::kern_attribute(style.tracking);
    if let Some((key, number)) = &kern {
        keys.push(key);
        values.push(number);
    }
    let attributes = cf_dictionary(&keys, &values).ok_or(unavailable)?;
    let attributed = cf_attributed_string(&string, &attributes).ok_or(unavailable)?;
    let typesetter = CTTypesetterCreateWithAttributedString(&attributed).ok_or(unavailable)?;
    let local = Boundaries::new(paragraph);
    let total = local.count();
    let mut start = 0;
    while start < total {
        let start_utf16 = local.utf16[start];
        let end = match max_width {
            None => total,
            Some(width) => {
                let count = CTTypesetterSuggestLineBreak(&typesetter, start_utf16 as isize, width);
                let limit = start_utf16 + usize::try_from(count).unwrap_or(0);
                let mut end = start;
                while end < total && local.utf16[end + 1] <= limit {
                    end += 1;
                }
                end.max(start + 1)
            }
        };
        let end_utf16 = local.utf16[end];
        let range = cf_range(start_utf16..end_utf16).ok_or(unavailable)?;
        let line = CTTypesetterCreateLine(&typesetter, range).ok_or(unavailable)?;
        let built = build_line(
            &line,
            &local,
            start..end,
            byte_start,
            grapheme_start,
            rtl,
            max_width,
            *top,
        )?;
        *top += built.height() * style.line_height;
        lines.push(built);
        start = end;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_line(
    line: &CTLine,
    local: &Boundaries,
    graphemes: Range<usize>,
    byte_start: usize,
    grapheme_start: usize,
    rtl: bool,
    max_width: Option<f64>,
    top: f64,
) -> Result<LayoutLine, TextLayoutError> {
    let unavailable = TextLayoutError::Unavailable;
    let (mut ascent, mut descent, mut leading) = (0.0, 0.0, 0.0);
    let advance = CTLineGetTypographicBounds(line, &mut ascent, &mut descent, &mut leading);
    let width = (advance - CTLineGetTrailingWhitespaceWidth(line)).max(0.0);
    let offset_x = match max_width {
        Some(w) if rtl => CTLineGetPenOffsetForFlush(line, 1.0, w),
        _ => 0.0,
    };
    let mut carets = Vec::with_capacity(graphemes.len() + 1);
    for g in graphemes.start..=graphemes.end {
        let mut secondary = 0.0;
        let primary = CTLineGetOffsetForStringIndex(line, local.utf16[g] as isize, &mut secondary);
        carets.push(CaretStop { primary, secondary });
    }
    let mut extents: Vec<Option<(f64, f64)>> = vec![None; graphemes.len()];
    let mut runs = Vec::new();
    for run in CTLineGetGlyphRuns(line).iter() {
        let copied = run.glyphs();
        let count = copied.glyphs.len();
        if count == 0 {
            continue;
        }
        let (glyphs, points, advances, indices) = (
            copied.glyphs,
            copied.positions,
            copied.advances,
            copied.string_indices,
        );
        let run_font = run.font().ok_or(unavailable)?;
        let name = CTFontCopyPostScriptName(run_font).ok_or(unavailable)?;
        let font = name.to_rust().ok_or(unavailable)?;
        let size = CTFontGetSize(run_font);
        let (run_ascent, run_descent) = (CTFontGetAscent(run_font), CTFontGetDescent(run_font));
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let (mut first, mut last) = (usize::MAX, 0);
        for i in 0..count {
            let x0 = points[i].x;
            let x1 = x0 + advances[i].width;
            left = left.min(x0.min(x1));
            right = right.max(x0.max(x1));
            let g = local.grapheme_at_utf16(usize::try_from(indices[i]).unwrap_or(0));
            first = first.min(g);
            last = last.max(g + 1);
            if graphemes.contains(&g) {
                let slot = &mut extents[g - graphemes.start];
                *slot = Some(match *slot {
                    None => (x0.min(x1), x0.max(x1)),
                    Some((l, r)) => (l.min(x0).min(x1), r.max(x0).max(x1)),
                });
            }
        }
        let bytes = local.bytes[first]..local.bytes[last.min(local.count())];
        runs.push(std::sync::Arc::new(GlyphRun {
            font,
            size,
            glyphs,
            positions: points.iter().map(|p| Point::new(p.x, -p.y)).collect(),
            text: byte_start + bytes.start..byte_start + bytes.end,
            right_to_left: run.is_right_to_left(),
            bounds: choscorui_core::Rect::new(
                Point::new(left, -run_ascent),
                Size::new(right - left, run_ascent + run_descent),
            ),
        }));
    }
    // Graphemes without glyphs (for example a ligature tail) collapse
    // to their caret position.
    let extents = extents
        .into_iter()
        .enumerate()
        .map(|(i, extent)| {
            extent.unwrap_or_else(|| {
                let x = carets[i].primary;
                (x, x)
            })
        })
        .collect();
    Ok(LayoutLine {
        bytes: byte_start + local.bytes[graphemes.start]..byte_start + local.bytes[graphemes.end],
        graphemes: grapheme_start + graphemes.start..grapheme_start + graphemes.end,
        baseline: top + ascent,
        ascent,
        descent,
        leading,
        width,
        offset_x,
        right_to_left: rtl,
        runs,
        carets,
        extents,
    })
}

thread_local! {
    /// CTFont objects keyed by PostScript name and size bits, bounded.
    static FONTS: RefCell<HashMap<(String, u64), CfRetained<CTFont>>> = RefCell::new(HashMap::new());
}
const FONT_CACHE_LIMIT: usize = 64;

/// Draws a pre-shaped run with its baseline origin at `origin`.
pub(crate) fn draw_glyph_run(
    context: &CGContext,
    origin: Point,
    run: &GlyphRun,
    color: choscorui_core::Color,
) -> bool {
    if run.glyphs.is_empty() {
        return true;
    }
    let Some(font) = run_font(&run.font, run.size) else {
        return false;
    };
    // Positions are stored y-down; the context is flipped around the
    // baseline so Core Text draws upright glyphs.
    let points: Vec<NSPoint> = run
        .positions
        .iter()
        .map(|p| NSPoint { x: p.x, y: -p.y })
        .collect();
    let previous = CGContextGetTextMatrix(context);
    CGContextSaveGState(context);
    paint::set_fill_color(context, color);
    CGContextTranslateCTM(context, origin.x, origin.y);
    CGContextScaleCTM(context, 1.0, -1.0);
    CGContextSetTextMatrix(
        context,
        CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        },
    );
    font.draw_glyphs(&run.glyphs, &points, context);
    CGContextRestoreGState(context);
    CGContextSetTextMatrix(context, previous);
    true
}

/// The font of a pre-shaped glyph run, by PostScript name and size. Both
/// painters use it, so they draw a run with the same font.
pub(crate) fn run_font(name: &str, size: f64) -> Option<CfRetained<CTFont>> {
    FONTS.with(|fonts| {
        let mut fonts = fonts.borrow_mut();
        let key = (name.to_owned(), size.to_bits());
        if let Some(font) = fonts.get(&key) {
            return Some(font.clone());
        }
        if fonts.len() >= FONT_CACHE_LIMIT {
            fonts.clear();
        }
        let font = CTFontCreateWithName(&*cf_string(name)?, size, None)?;
        fonts.insert(key, font.clone());
        Some(font)
    })
}
