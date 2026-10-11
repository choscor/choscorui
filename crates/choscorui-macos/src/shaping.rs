use super::*;
use choscorui_core::{FontSlant, FontWeight};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapError {
    InvalidScale,
    TooLarge,
    ContextUnavailable,
    PaintFailed,
}

impl std::fmt::Display for BitmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScale => f.write_str("the scale must be positive and finite"),
            Self::TooLarge => f.write_str("the bitmap is too large"),
            Self::ContextUnavailable => f.write_str("a bitmap context could not be created"),
            Self::PaintFailed => f.write_str("painting the display list failed"),
        }
    }
}

impl std::error::Error for BitmapError {}

/// Premultiplied RGBA8 pixels drawn with the same Core Graphics/Core Text
/// command path as a window. Rows are contiguous in bitmap storage order.
pub struct MacBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl MacBitmap {
    pub fn render(
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self, BitmapError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(BitmapError::InvalidScale);
        }
        let row_len = (width as usize)
            .checked_mul(4)
            .ok_or(BitmapError::TooLarge)?;
        let len = row_len
            .checked_mul(height as usize)
            .ok_or(BitmapError::TooLarge)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(|_| BitmapError::TooLarge)?;
        if width == 0 || height == 0 {
            return Ok(Self {
                width,
                height,
                pixels,
            });
        }
        // SAFETY: Core Graphics owns the automatically allocated context data.
        // It stays alive until every row has been copied into `pixels`.
        unsafe {
            let context = CfOwned::new(CGBitmapContextCreate(
                std::ptr::null_mut(),
                width as usize,
                height as usize,
                8,
                0,
                paint::srgb(),
                1 | (4 << 12),
            ))
            .ok_or(BitmapError::ContextUnavailable)?;
            CGContextTranslateCTM(context.as_ptr(), 0.0, f64::from(height));
            CGContextScaleCTM(context.as_ptr(), scale, -scale);
            let dirty = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: f64::from(width) / scale,
                    height: f64::from(height) / scale,
                },
            };
            if !paint::paint_commands(context.as_ptr(), dirty, list) {
                return Err(BitmapError::PaintFailed);
            }
            let data = NonNull::new(CGBitmapContextGetData(context.as_ptr()))
                .ok_or(BitmapError::ContextUnavailable)?;
            let stride = CGBitmapContextGetBytesPerRow(context.as_ptr());
            if stride < row_len || stride.checked_mul(height as usize).is_none() {
                return Err(BitmapError::TooLarge);
            }
            pixels.resize(len, 0);
            for y in 0..height as usize {
                let row = std::slice::from_raw_parts(
                    (data.as_ptr() as *const u8).add(y * stride),
                    row_len,
                );
                pixels[y * row_len..(y + 1) * row_len].copy_from_slice(row);
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = (y as usize * self.width as usize + x as usize) * 4;
        Some(
            self.pixels[offset..offset + 4]
                .try_into()
                .expect("pixel has four channels"),
        )
    }
}

pub(crate) unsafe fn cf_utf8(text: &str) -> Option<CfOwned> {
    let length = isize::try_from(text.len()).ok()?;
    CfOwned::new(unsafe {
        CFStringCreateWithBytes(std::ptr::null_mut(), text.as_ptr(), length, 0x0800_0100, 0)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    InvalidFont,
    InvalidWidth,
    InvalidSelection,
    Unavailable,
}

impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFont => f.write_str("the font could not be created"),
            Self::InvalidWidth => f.write_str("the wrap width must be positive and finite"),
            Self::InvalidSelection => f.write_str("the selection is outside the text"),
            Self::Unavailable => f.write_str("Core Text could not shape the text"),
        }
    }
}

impl std::error::Error for ShapeError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLineMetrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretOffsets {
    pub primary: f64,
    pub secondary: f64,
}

/// An immutable Core Text line. Measurement, caret offsets, and drawing use
/// this same shaped object, with grapheme indices at the safe API boundary.
pub struct ShapedLine {
    text: TextBuffer,
    line: CfOwned,
}

impl ShapedLine {
    pub fn new(text: &str, family: &str, size: f64) -> Result<Self, ShapeError> {
        Self::styled(text, family, size, FontWeight::REGULAR, 0.0)
    }

    /// A line in `family` at `weight`, with `tracking` points after each
    /// character.
    pub fn styled(
        text: &str,
        family: &str,
        size: f64,
        weight: FontWeight,
        tracking: f64,
    ) -> Result<Self, ShapeError> {
        if family.is_empty() || !size.is_finite() || size <= 0.0 {
            return Err(ShapeError::InvalidFont);
        }
        // SAFETY: The attributed string, font, and kern number stay alive
        // through line creation. Core Text returns a separately owned
        // immutable CTLine.
        unsafe {
            let string = cf_utf8(text).ok_or(ShapeError::Unavailable)?;
            let font = fonts::create_font(family, size, weight, FontSlant::Upright)
                .ok_or(ShapeError::Unavailable)?;
            let kern = fonts::kern_attribute(tracking);
            let mut keys = vec![
                kCTFontAttributeName,
                kCTForegroundColorFromContextAttributeName,
            ];
            let mut values = vec![font.as_ptr(), kCFBooleanTrue as Id];
            if let Some((key, number)) = &kern {
                keys.push(*key);
                values.push(number.as_ptr());
            }
            let attributes = CfOwned::new(CFDictionaryCreate(
                std::ptr::null_mut(),
                keys.as_ptr(),
                values.as_ptr(),
                keys.len() as isize,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ))
            .ok_or(ShapeError::Unavailable)?;
            let attributed = CfOwned::new(CFAttributedStringCreate(
                std::ptr::null_mut(),
                string.as_ptr(),
                attributes.as_ptr(),
            ))
            .ok_or(ShapeError::Unavailable)?;
            let line = CfOwned::new(CTLineCreateWithAttributedString(attributed.as_ptr()))
                .ok_or(ShapeError::Unavailable)?;
            Ok(Self {
                text: TextBuffer::new(text),
                line,
            })
        }
    }

    pub fn metrics(&self) -> TextLineMetrics {
        let mut ascent = 0.0;
        let mut descent = 0.0;
        let mut leading = 0.0;
        // SAFETY: `line` is an owned live CTLine; output pointers are valid.
        let advance = unsafe {
            CTLineGetTypographicBounds(self.line.as_ptr(), &mut ascent, &mut descent, &mut leading)
        };
        TextLineMetrics {
            advance,
            ascent,
            descent,
            leading,
        }
    }

    pub fn caret_offsets(&self, grapheme_index: usize) -> Option<CaretOffsets> {
        let byte = self.text.grapheme_to_byte(grapheme_index)?;
        let utf16 = self.text.byte_to_utf16(byte)?;
        let index = isize::try_from(utf16).ok()?;
        let mut secondary = 0.0;
        // SAFETY: UTF-16 offset is a valid grapheme boundary in this CTLine.
        let primary =
            unsafe { CTLineGetOffsetForStringIndex(self.line.as_ptr(), index, &mut secondary) };
        Some(CaretOffsets { primary, secondary })
    }

    /// Returns the nearest grapheme caret for a horizontal point on this line.
    /// Uses Core Text's primary caret positions; mixed-direction affinity is
    /// exposed separately through `caret_offsets`.
    pub fn hit_test_x(&self, x: f64) -> usize {
        if !x.is_finite() {
            return 0;
        }
        let mut nearest = 0;
        let mut distance = f64::INFINITY;
        for index in 0..=self.text.grapheme_count() {
            let Some(offset) = self.caret_offsets(index) else {
                continue;
            };
            let current = (offset.primary - x).abs();
            if current < distance {
                nearest = index;
                distance = current;
            }
        }
        nearest
    }

    unsafe fn draw(&self, context: Id, baseline: Point, color: choscorui_core::Color) {
        unsafe {
            let previous_text_matrix = CGContextGetTextMatrix(context);
            CGContextSaveGState(context);
            CGContextSetRGBFillColor(
                context,
                f64::from(color.red) / 255.0,
                f64::from(color.green) / 255.0,
                f64::from(color.blue) / 255.0,
                f64::from(color.alpha) / 255.0,
            );
            CGContextTranslateCTM(context, baseline.x, baseline.y);
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
            CGContextSetTextPosition(context, 0.0, 0.0);
            CTLineDraw(self.line.as_ptr(), context);
            CGContextRestoreGState(context);
            CGContextSetTextMatrix(context, previous_text_matrix);
        }
    }
}

pub struct WrappedLine {
    text: String,
    shaped: ShapedLine,
    baseline_y: f64,
    grapheme_range: Range<usize>,
}

impl WrappedLine {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn metrics(&self) -> TextLineMetrics {
        self.shaped.metrics()
    }
    pub fn baseline_y(&self) -> f64 {
        self.baseline_y
    }
    pub fn shaped(&self) -> &ShapedLine {
        &self.shaped
    }
    pub fn grapheme_range(&self) -> Range<usize> {
        self.grapheme_range.clone()
    }
}

/// Width-limited lines from Core Text's contextual break suggestions.
/// Each line owns the same ShapedLine type used for metrics and painting.
pub struct WrappedText {
    lines: Vec<WrappedLine>,
    family: String,
    size: f64,
    source_graphemes: usize,
}

impl WrappedText {
    pub fn new(text: &str, family: &str, size: f64, width: f64) -> Result<Self, ShapeError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(ShapeError::InvalidWidth);
        }
        if family.is_empty() || !size.is_finite() || size <= 0.0 {
            return Err(ShapeError::InvalidFont);
        }
        let mut lines = Vec::new();
        let mut top = 0.0;
        let source = TextBuffer::new(text);
        let mut paragraph_byte = 0usize;
        for raw_paragraph in text.split('\n') {
            let paragraph_start = paragraph_byte;
            let paragraph_end = paragraph_start + raw_paragraph.len();
            paragraph_byte = paragraph_end + usize::from(paragraph_end < text.len());
            let paragraph = raw_paragraph.strip_suffix('\r').unwrap_or(raw_paragraph);
            if paragraph.is_empty() {
                let grapheme = source
                    .byte_to_grapheme(paragraph_start)
                    .ok_or(ShapeError::Unavailable)?;
                append_wrapped_line(&mut lines, "", family, size, &mut top, grapheme..grapheme)?;
                continue;
            }
            // SAFETY: The temporary attributed text, font and dictionary stay
            // alive for every typesetter query in this paragraph.
            unsafe {
                let string = cf_utf8(paragraph).ok_or(ShapeError::Unavailable)?;
                let font_name = cf_utf8(family).ok_or(ShapeError::Unavailable)?;
                let font = CfOwned::new(CTFontCreateWithName(
                    font_name.as_ptr(),
                    size,
                    std::ptr::null_mut(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let keys = [
                    kCTFontAttributeName,
                    kCTForegroundColorFromContextAttributeName,
                ];
                let values = [font.as_ptr(), kCFBooleanTrue as Id];
                let attributes = CfOwned::new(CFDictionaryCreate(
                    std::ptr::null_mut(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    2,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let attributed = CfOwned::new(CFAttributedStringCreate(
                    std::ptr::null_mut(),
                    string.as_ptr(),
                    attributes.as_ptr(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let typesetter =
                    CfOwned::new(CTTypesetterCreateWithAttributedString(attributed.as_ptr()))
                        .ok_or(ShapeError::Unavailable)?;
                let buffer = TextBuffer::new(paragraph);
                let total = buffer.grapheme_count();
                let mut start = 0;
                while start < total {
                    let start_byte = buffer
                        .grapheme_to_byte(start)
                        .ok_or(ShapeError::Unavailable)?;
                    let start_utf16 = buffer
                        .byte_to_utf16(start_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    let start_index =
                        isize::try_from(start_utf16).map_err(|_| ShapeError::Unavailable)?;
                    let count =
                        CTTypesetterSuggestLineBreak(typesetter.as_ptr(), start_index, width);
                    let suggested_end =
                        start_utf16.saturating_add(usize::try_from(count).unwrap_or(0));
                    let mut end = start;
                    for candidate in start + 1..=total {
                        let byte = buffer
                            .grapheme_to_byte(candidate)
                            .ok_or(ShapeError::Unavailable)?;
                        let offset = buffer.byte_to_utf16(byte).ok_or(ShapeError::Unavailable)?;
                        if offset > suggested_end {
                            break;
                        }
                        end = candidate;
                    }
                    if end == start {
                        end += 1;
                    }
                    let end_byte = buffer
                        .grapheme_to_byte(end)
                        .ok_or(ShapeError::Unavailable)?;
                    let source_start = source
                        .byte_to_grapheme(paragraph_start + start_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    let source_end = source
                        .byte_to_grapheme(paragraph_start + end_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    append_wrapped_line(
                        &mut lines,
                        &paragraph[start_byte..end_byte],
                        family,
                        size,
                        &mut top,
                        source_start..source_end,
                    )?;
                    start = end;
                }
            }
        }
        Ok(Self {
            lines,
            family: family.to_owned(),
            size,
            source_graphemes: source.grapheme_count(),
        })
    }

    pub fn lines(&self) -> &[WrappedLine] {
        &self.lines
    }

    /// Maps a point relative to the paragraph origin to the nearest source
    /// grapheme caret, including across explicit newline gaps.
    pub fn hit_test(&self, point: Point) -> usize {
        if !point.x.is_finite() || !point.y.is_finite() {
            return 0;
        }
        let Some(mut nearest) = self.lines.first() else {
            return 0;
        };
        let mut distance = (point.y - nearest.baseline_y).abs();
        for line in self.lines.iter().skip(1) {
            let current = (point.y - line.baseline_y).abs();
            if current < distance {
                nearest = line;
                distance = current;
            }
        }
        (nearest.grapheme_range.start + nearest.shaped.hit_test_x(point.x))
            .min(nearest.grapheme_range.end)
    }

    pub fn selection_rects(
        &self,
        selection: Range<usize>,
    ) -> Result<Vec<choscorui_core::Rect>, ShapeError> {
        if selection.start > selection.end || selection.end > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let mut rects = Vec::new();
        for line in &self.lines {
            let start = selection.start.max(line.grapheme_range.start);
            let end = selection.end.min(line.grapheme_range.end);
            if start >= end {
                continue;
            }
            let first = line
                .shaped
                .caret_offsets(start - line.grapheme_range.start)
                .ok_or(ShapeError::Unavailable)?
                .primary;
            let last = line
                .shaped
                .caret_offsets(end - line.grapheme_range.start)
                .ok_or(ShapeError::Unavailable)?
                .primary;
            let metrics = line.shaped.metrics();
            rects.push(choscorui_core::Rect::new(
                Point::new(first.min(last), line.baseline_y - metrics.ascent),
                Size::new(
                    (last - first).abs(),
                    (metrics.ascent + metrics.descent + metrics.leading).max(0.0),
                ),
            ));
        }
        Ok(rects)
    }

    pub fn caret_rect(&self, caret: usize) -> Result<choscorui_core::Rect, ShapeError> {
        if caret > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let line = self
            .lines
            .iter()
            .find(|line| line.grapheme_range.start <= caret && caret <= line.grapheme_range.end)
            .or_else(|| {
                self.lines
                    .iter()
                    .find(|line| line.grapheme_range.start > caret)
            })
            .or_else(|| self.lines.last())
            .ok_or(ShapeError::Unavailable)?;
        let local = caret
            .saturating_sub(line.grapheme_range.start)
            .min(line.grapheme_range.end - line.grapheme_range.start);
        let x = line
            .shaped
            .caret_offsets(local)
            .ok_or(ShapeError::Unavailable)?
            .primary;
        let metrics = line.shaped.metrics();
        Ok(choscorui_core::Rect::new(
            Point::new(x, line.baseline_y - metrics.ascent),
            Size::new(
                1.0,
                (metrics.ascent + metrics.descent + metrics.leading).max(0.0),
            ),
        ))
    }

    /// Moves one visual line up or down while retaining a preferred x value.
    /// The returned caret is a source grapheme index.
    pub fn move_caret_vertical(
        &self,
        caret: usize,
        direction: i32,
        preferred_x: Option<f64>,
    ) -> Result<(usize, f64), ShapeError> {
        if caret > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let current = self
            .lines
            .iter()
            .position(|line| line.grapheme_range.start <= caret && caret <= line.grapheme_range.end)
            .or_else(|| {
                self.lines
                    .iter()
                    .position(|line| line.grapheme_range.start > caret)
            })
            .ok_or(ShapeError::Unavailable)?;
        let x = match preferred_x {
            Some(x) if x.is_finite() => x,
            _ => self.caret_rect(caret)?.origin.x,
        };
        let target = if direction < 0 {
            current.saturating_sub(1)
        } else if direction > 0 {
            (current + 1).min(self.lines.len() - 1)
        } else {
            current
        };
        let line = &self.lines[target];
        let local = line.shaped.hit_test_x(x);
        Ok((
            (line.grapheme_range.start + local).min(line.grapheme_range.end),
            x,
        ))
    }

    pub fn append_to_display_list(
        &self,
        list: &mut DisplayList,
        origin: Point,
        color: choscorui_core::Color,
    ) -> Result<(), choscorui_core::DisplayListError> {
        for line in &self.lines {
            list.draw_text(
                Point::new(origin.x, origin.y + line.baseline_y),
                &line.text,
                &self.family,
                self.size,
                color,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn append_wrapped_line(
    lines: &mut Vec<WrappedLine>,
    text: &str,
    family: &str,
    size: f64,
    top: &mut f64,
    grapheme_range: Range<usize>,
) -> Result<(), ShapeError> {
    let shaped = ShapedLine::new(text, family, size)?;
    let metrics = shaped.metrics();
    let baseline_y = *top + metrics.ascent.max(size * 0.8);
    *top += (metrics.ascent + metrics.descent + metrics.leading).max(size * 1.2);
    lines.push(WrappedLine {
        text: text.to_owned(),
        shaped,
        baseline_y,
        grapheme_range,
    });
    Ok(())
}

pub(crate) unsafe fn draw_text_run(
    context: Id,
    baseline: Point,
    text: &str,
    run: &choscorui_core::TextRun<'_>,
    color: choscorui_core::Color,
) -> bool {
    let Ok(line) = ShapedLine::styled(text, run.family, run.size, run.weight, run.tracking) else {
        return false;
    };
    unsafe { line.draw(context, baseline, color) };
    true
}
