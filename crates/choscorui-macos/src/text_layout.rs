//! Core Text implementation of the core text-layout contract. Shaping,
//! fallback, bidi, and line breaking come from Core Text; the result is
//! copied into owned glyph runs that both measurement and painting use.

use super::*;
use choscorui_core::{
    CaretStop, GlyphRun, LayoutLine, TextLayout, TextLayoutError, TextShaper, TextStyle,
    grapheme_boundaries,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

#[repr(C)]
struct CTParagraphStyleSetting {
    spec: u32,
    value_size: usize,
    value: *const c_void,
}

const PARAGRAPH_BASE_WRITING_DIRECTION: u32 = 13;
const RUN_STATUS_RIGHT_TO_LEFT: u32 = 1;
const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTLanguageAttributeName: Id;
    static kCTParagraphStyleAttributeName: Id;
    static kCTFontCascadeListAttribute: Id;
    static kCTFontFamilyNameAttribute: Id;
    fn CTFontManagerCopyAvailableFontFamilyNames() -> Id;
    fn CTFontCopyFontDescriptor(font: Id) -> Id;
    fn CTFontDescriptorCreateWithAttributes(attributes: Id) -> Id;
    fn CTFontDescriptorCreateCopyWithAttributes(descriptor: Id, attributes: Id) -> Id;
    fn CTFontCreateWithFontDescriptor(descriptor: Id, size: f64, matrix: Id) -> Id;
    fn CTFontCopyPostScriptName(font: Id) -> Id;
    fn CTFontGetSize(font: Id) -> f64;
    fn CTFontGetAscent(font: Id) -> f64;
    fn CTFontGetDescent(font: Id) -> f64;
    fn CTFontGetLeading(font: Id) -> f64;
    fn CTFontDrawGlyphs(
        font: Id,
        glyphs: *const u16,
        positions: *const NSPoint,
        count: usize,
        context: Id,
    );
    fn CTParagraphStyleCreate(settings: *const CTParagraphStyleSetting, count: usize) -> Id;
    fn CTTypesetterCreateLine(typesetter: Id, range: CFRange) -> Id;
    fn CTLineGetTrailingWhitespaceWidth(line: Id) -> f64;
    fn CTLineGetPenOffsetForFlush(line: Id, flush: f64, width: f64) -> f64;
    fn CTLineGetGlyphRuns(line: Id) -> Id;
    fn CTRunGetGlyphCount(run: Id) -> isize;
    fn CTRunGetGlyphs(run: Id, range: CFRange, buffer: *mut u16);
    fn CTRunGetPositions(run: Id, range: CFRange, buffer: *mut NSPoint);
    fn CTRunGetAdvances(run: Id, range: CFRange, buffer: *mut NSSize);
    fn CTRunGetStringIndices(run: Id, range: CFRange, buffer: *mut isize);
    fn CTRunGetStatus(run: Id) -> u32;
    fn CTRunGetAttributes(run: Id) -> Id;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFTypeArrayCallBacks: c_void;
    fn CFArrayCreate(
        allocator: Id,
        values: *const Id,
        count: isize,
        callbacks: *const c_void,
    ) -> Id;
    fn CFArrayGetCount(array: Id) -> isize;
    fn CFArrayGetValueAtIndex(array: Id, index: isize) -> Id;
    fn CFDictionaryGetValue(dictionary: Id, key: Id) -> Id;
    fn CFStringGetCString(string: Id, buffer: *mut i8, size: isize, encoding: u32) -> u8;
}

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

/// +1 dictionary that retains its keys and values.
unsafe fn cf_dictionary(keys: &[Id], values: &[Id]) -> Option<CfOwned> {
    unsafe {
        CfOwned::new(CFDictionaryCreate(
            std::ptr::null_mut(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as isize,
            &raw const kCFTypeDictionaryKeyCallBacks as Id,
            &raw const kCFTypeDictionaryValueCallBacks as Id,
        ))
    }
}

/// Installed font family names, sorted by the system for display. Hidden
/// system families (names starting with `.`) are omitted.
pub fn available_font_families() -> Vec<String> {
    // SAFETY: The Copy function returns a +1 CFArray of CFStrings owned by
    // `CfOwned`; each element is borrowed only while the array lives.
    unsafe {
        let Some(array) = CfOwned::new(CTFontManagerCopyAvailableFontFamilyNames()) else {
            return Vec::new();
        };
        (0..CFArrayGetCount(array.as_ptr()))
            .filter_map(|index| rust_string(CFArrayGetValueAtIndex(array.as_ptr(), index)))
            .filter(|name| !name.starts_with('.'))
            .collect()
    }
}

unsafe fn rust_string(string: Id) -> Option<String> {
    let mut buffer = [0i8; 512];
    // SAFETY: CFStringGetCString writes at most buffer.len() bytes,
    // including the terminating NUL, and reports failure for overflow.
    unsafe {
        if CFStringGetCString(string, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) == 0 {
            return None;
        }
        CStr::from_ptr(buffer.as_ptr())
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}

/// +1 CTFont for `style`: the face from [`fonts::create_font`], then the
/// style's fallback cascade.
unsafe fn create_font(style: &TextStyle) -> Result<CfOwned, TextLayoutError> {
    unsafe {
        let font = fonts::create_font(&style.family, style.size, style.weight, style.slant)
            .ok_or(TextLayoutError::Unavailable)?;
        if style.fallback.is_empty() {
            return Ok(font);
        }
        let mut descriptors = Vec::new();
        for family in &style.fallback {
            let family = cf_utf8(family).ok_or(TextLayoutError::Unavailable)?;
            let attributes = cf_dictionary(&[kCTFontFamilyNameAttribute], &[family.as_ptr()])
                .ok_or(TextLayoutError::Unavailable)?;
            descriptors.push(
                CfOwned::new(CTFontDescriptorCreateWithAttributes(attributes.as_ptr()))
                    .ok_or(TextLayoutError::Unavailable)?,
            );
        }
        let raw: Vec<Id> = descriptors.iter().map(CfOwned::as_ptr).collect();
        let cascade = CfOwned::new(CFArrayCreate(
            std::ptr::null_mut(),
            raw.as_ptr(),
            raw.len() as isize,
            &raw const kCFTypeArrayCallBacks,
        ))
        .ok_or(TextLayoutError::Unavailable)?;
        let attributes = cf_dictionary(&[kCTFontCascadeListAttribute], &[cascade.as_ptr()])
            .ok_or(TextLayoutError::Unavailable)?;
        let base = CfOwned::new(CTFontCopyFontDescriptor(font.as_ptr()))
            .ok_or(TextLayoutError::Unavailable)?;
        let descriptor = CfOwned::new(CTFontDescriptorCreateCopyWithAttributes(
            base.as_ptr(),
            attributes.as_ptr(),
        ))
        .ok_or(TextLayoutError::Unavailable)?;
        CfOwned::new(CTFontCreateWithFontDescriptor(
            descriptor.as_ptr(),
            style.size,
            std::ptr::null_mut(),
        ))
        .ok_or(TextLayoutError::Unavailable)
    }
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
        // SAFETY: Every Create result is owned by CfOwned and outlives the
        // borrowed lines, runs, and arrays read from it in this scope.
        let lines = unsafe {
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
unsafe fn shape_paragraph(
    paragraph: &str,
    byte_start: usize,
    grapheme_start: usize,
    font: &CfOwned,
    style: &TextStyle,
    max_width: Option<f64>,
    top: &mut f64,
    lines: &mut Vec<LayoutLine>,
) -> Result<(), TextLayoutError> {
    let unavailable = TextLayoutError::Unavailable;
    let rtl = style.direction.is_rtl_for(paragraph);
    unsafe {
        if paragraph.is_empty() {
            let (ascent, descent, leading) = (
                CTFontGetAscent(font.as_ptr()),
                CTFontGetDescent(font.as_ptr()),
                CTFontGetLeading(font.as_ptr()),
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
        let string = cf_utf8(paragraph).ok_or(unavailable)?;
        let direction: i8 = if rtl { 1 } else { 0 };
        let setting = CTParagraphStyleSetting {
            spec: PARAGRAPH_BASE_WRITING_DIRECTION,
            value_size: 1,
            value: (&raw const direction).cast(),
        };
        let paragraph_style =
            CfOwned::new(CTParagraphStyleCreate(&setting, 1)).ok_or(unavailable)?;
        let mut keys = vec![
            kCTFontAttributeName,
            kCTForegroundColorFromContextAttributeName,
            kCTParagraphStyleAttributeName,
        ];
        let mut values = vec![
            font.as_ptr(),
            kCFBooleanTrue as Id,
            paragraph_style.as_ptr(),
        ];
        let language = match &style.locale {
            Some(tag) => Some(cf_utf8(tag).ok_or(unavailable)?),
            None => None,
        };
        if let Some(language) = &language {
            keys.push(kCTLanguageAttributeName);
            values.push(language.as_ptr());
        }
        let kern = fonts::kern_attribute(style.tracking);
        if let Some((key, number)) = &kern {
            keys.push(*key);
            values.push(number.as_ptr());
        }
        let attributes = cf_dictionary(&keys, &values).ok_or(unavailable)?;
        let attributed = CfOwned::new(CFAttributedStringCreate(
            std::ptr::null_mut(),
            string.as_ptr(),
            attributes.as_ptr(),
        ))
        .ok_or(unavailable)?;
        let typesetter = CfOwned::new(CTTypesetterCreateWithAttributedString(attributed.as_ptr()))
            .ok_or(unavailable)?;
        let local = Boundaries::new(paragraph);
        let total = local.count();
        let mut start = 0;
        while start < total {
            let start_utf16 = local.utf16[start];
            let end = match max_width {
                None => total,
                Some(width) => {
                    let count = CTTypesetterSuggestLineBreak(
                        typesetter.as_ptr(),
                        start_utf16 as isize,
                        width,
                    );
                    let limit = start_utf16 + usize::try_from(count).unwrap_or(0);
                    let mut end = start;
                    while end < total && local.utf16[end + 1] <= limit {
                        end += 1;
                    }
                    end.max(start + 1)
                }
            };
            let end_utf16 = local.utf16[end];
            let line = CfOwned::new(CTTypesetterCreateLine(
                typesetter.as_ptr(),
                CFRange {
                    location: start_utf16 as isize,
                    length: (end_utf16 - start_utf16) as isize,
                },
            ))
            .ok_or(unavailable)?;
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
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn build_line(
    line: &CfOwned,
    local: &Boundaries,
    graphemes: Range<usize>,
    byte_start: usize,
    grapheme_start: usize,
    rtl: bool,
    max_width: Option<f64>,
    top: f64,
) -> Result<LayoutLine, TextLayoutError> {
    let unavailable = TextLayoutError::Unavailable;
    unsafe {
        let (mut ascent, mut descent, mut leading) = (0.0, 0.0, 0.0);
        let advance =
            CTLineGetTypographicBounds(line.as_ptr(), &mut ascent, &mut descent, &mut leading);
        let width = (advance - CTLineGetTrailingWhitespaceWidth(line.as_ptr())).max(0.0);
        let offset_x = match max_width {
            Some(w) if rtl => CTLineGetPenOffsetForFlush(line.as_ptr(), 1.0, w),
            _ => 0.0,
        };
        let mut carets = Vec::with_capacity(graphemes.len() + 1);
        for g in graphemes.start..=graphemes.end {
            let mut secondary = 0.0;
            let primary = CTLineGetOffsetForStringIndex(
                line.as_ptr(),
                local.utf16[g] as isize,
                &mut secondary,
            );
            carets.push(CaretStop { primary, secondary });
        }
        let mut extents: Vec<Option<(f64, f64)>> = vec![None; graphemes.len()];
        let mut runs = Vec::new();
        let array = CTLineGetGlyphRuns(line.as_ptr());
        for index in 0..CFArrayGetCount(array) {
            let run = CFArrayGetValueAtIndex(array, index);
            let count = usize::try_from(CTRunGetGlyphCount(run)).unwrap_or(0);
            if count == 0 {
                continue;
            }
            let all = CFRange {
                location: 0,
                length: 0,
            };
            let mut glyphs = vec![0u16; count];
            let mut points = vec![NSPoint { x: 0.0, y: 0.0 }; count];
            let mut advances = vec![
                NSSize {
                    width: 0.0,
                    height: 0.0
                };
                count
            ];
            let mut indices = vec![0isize; count];
            CTRunGetGlyphs(run, all, glyphs.as_mut_ptr());
            CTRunGetPositions(run, all, points.as_mut_ptr());
            CTRunGetAdvances(run, all, advances.as_mut_ptr());
            CTRunGetStringIndices(run, all, indices.as_mut_ptr());
            let run_font = CFDictionaryGetValue(CTRunGetAttributes(run), kCTFontAttributeName);
            if run_font.is_null() {
                return Err(unavailable);
            }
            let name = CfOwned::new(CTFontCopyPostScriptName(run_font)).ok_or(unavailable)?;
            let font = rust_string(name.as_ptr()).ok_or(unavailable)?;
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
                right_to_left: CTRunGetStatus(run) & RUN_STATUS_RIGHT_TO_LEFT != 0,
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
            bytes: byte_start + local.bytes[graphemes.start]
                ..byte_start + local.bytes[graphemes.end],
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
}

thread_local! {
    /// CTFont objects keyed by PostScript name and size bits, bounded.
    static FONTS: RefCell<HashMap<(String, u64), CfOwned>> = RefCell::new(HashMap::new());
}
const FONT_CACHE_LIMIT: usize = 64;

/// Draws a pre-shaped run with its baseline origin at `origin`.
pub(crate) unsafe fn draw_glyph_run(
    context: Id,
    origin: Point,
    run: &GlyphRun,
    color: choscorui_core::Color,
) -> bool {
    if run.glyphs.is_empty() {
        return true;
    }
    FONTS.with(|fonts| {
        let mut fonts = fonts.borrow_mut();
        let key = (run.font.clone(), run.size.to_bits());
        if !fonts.contains_key(&key) {
            if fonts.len() >= FONT_CACHE_LIMIT {
                fonts.clear();
            }
            // SAFETY: Creates a +1 font owned by the cache.
            let Some(font) = (unsafe { cf_utf8(&run.font) }).and_then(|name| {
                CfOwned::new(unsafe {
                    CTFontCreateWithName(name.as_ptr(), run.size, std::ptr::null_mut())
                })
            }) else {
                return false;
            };
            fonts.insert(key.clone(), font);
        }
        let font = &fonts[&key];
        // Positions are stored y-down; the context is flipped around the
        // baseline so Core Text draws upright glyphs.
        let points: Vec<NSPoint> = run
            .positions
            .iter()
            .map(|p| NSPoint { x: p.x, y: -p.y })
            .collect();
        // SAFETY: The context is live for this paint pass; buffers have
        // equal lengths checked by DisplayList::draw_glyphs.
        unsafe {
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
            CTFontDrawGlyphs(
                font.as_ptr(),
                run.glyphs.as_ptr(),
                points.as_ptr(),
                run.glyphs.len(),
                context,
            );
            CGContextRestoreGState(context);
            CGContextSetTextMatrix(context, previous);
        }
        true
    })
}
