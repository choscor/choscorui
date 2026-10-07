//! Lays out a `RichDocument` with any `TextShaper`: each run is shaped in
//! word-sized pieces with its own style, pieces wrap greedily at spaces,
//! and lines align per block. Painting emits glyph runs (or text commands
//! when a shaper supplies no glyphs), underlines, and strikethroughs.
//!
//! Limits: a word wider than the width overflows instead of breaking
//! mid-word, and bidirectional reordering happens within pieces only; a
//! right-to-left block places its pieces from the right.

use crate::{
    BlockAlignment, BlockKind, Color, DisplayList, DisplayListError, FontSlant, FontWeight, Point,
    Rect, RichDocument, RichPosition, Size, SpanFormat, TextLayout, TextLayoutError, TextShaper,
    TextStyle, first_strong_is_rtl,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct RichTheme {
    pub base: TextStyle,
    pub code_family: String,
    pub text: Color,
    pub link: Color,
    pub quote: Color,
    pub code_background: Color,
    /// Space after each block, as a multiple of the base size.
    pub block_spacing: f64,
    pub indent: f64,
}

impl RichTheme {
    pub fn new(base: TextStyle) -> Self {
        Self {
            base,
            code_family: "Menlo".into(),
            text: Color::rgba8(29, 29, 31, 255),
            link: Color::rgba8(0, 102, 204, 255),
            quote: Color::rgba8(110, 110, 115, 255),
            code_background: Color::rgba8(0, 0, 0, 18),
            block_spacing: 0.5,
            indent: 20.0,
        }
    }

    fn style(&self, kind: BlockKind, format: &SpanFormat) -> TextStyle {
        let mut style = self.base.clone();
        if let BlockKind::Heading(level) = kind {
            style.size *= [2.0, 1.6, 1.3, 1.15, 1.0, 0.9][(level.clamp(1, 6) - 1) as usize];
            style.weight = FontWeight(700);
        }
        if format.bold {
            style.weight = FontWeight(700);
        }
        if format.italic {
            style.slant = FontSlant::Italic;
        }
        if format.code || kind == BlockKind::Code {
            style.family = self.code_family.clone();
            style.size *= 0.92;
        }
        style
    }

    fn color(&self, kind: BlockKind, format: &SpanFormat) -> Color {
        if format.link.is_some() {
            self.link
        } else if let Some(color) = format.color {
            color
        } else if kind == BlockKind::Quote {
            self.quote
        } else {
            self.text
        }
    }
}

/// One shaped piece of a run on a line.
#[derive(Clone, Debug, PartialEq)]
pub struct RichPiece {
    pub block: usize,
    /// Byte range within the block text.
    pub range: std::ops::Range<usize>,
    /// Top-left of the piece's line box.
    pub origin: Point,
    pub layout: Arc<TextLayout>,
    pub color: Color,
    pub format: SpanFormat,
    /// List markers are decoration, not document text.
    pub marker: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichLine {
    pub block: usize,
    pub top: f64,
    pub baseline: f64,
    pub height: f64,
    /// Indexes into `pieces`.
    pub pieces: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichLayout {
    pub pieces: Vec<RichPiece>,
    pub lines: Vec<RichLine>,
    /// Background rects for code blocks and quote bars.
    pub decorations: Vec<(Rect, Color)>,
    pub size: Size,
}

impl RichLayout {
    pub fn build(
        document: &RichDocument,
        theme: &RichTheme,
        shaper: &dyn TextShaper,
        width: f64,
    ) -> Result<Self, TextLayoutError> {
        let mut cache: HashMap<(String, TextStyle), Arc<TextLayout>> = HashMap::new();
        let mut shape =
            |text: &str, style: &TextStyle| -> Result<Arc<TextLayout>, TextLayoutError> {
                let key = (text.to_owned(), style.clone());
                if let Some(layout) = cache.get(&key) {
                    return Ok(Arc::clone(layout));
                }
                let layout = Arc::new(shaper.shape(text, style, None)?);
                cache.insert(key, Arc::clone(&layout));
                Ok(layout)
            };
        let mut layout = Self {
            pieces: Vec::new(),
            lines: Vec::new(),
            decorations: Vec::new(),
            size: Size::new(width, 0.0),
        };
        let mut top = 0.0;
        let mut number = 0;
        for (block_index, block) in document.blocks().iter().enumerate() {
            number = if block.kind == BlockKind::Numbered {
                number + 1
            } else {
                0
            };
            let indent = match block.kind {
                BlockKind::Bullet | BlockKind::Numbered | BlockKind::Quote => theme.indent,
                BlockKind::Code => theme.indent / 2.0,
                _ => 0.0,
            };
            let rtl = first_strong_is_rtl(&block.text()).unwrap_or(false);
            let block_top = top;
            let available = (width - indent).max(1.0);
            // Collect pieces: word plus trailing spaces, per run.
            let mut words: Vec<(std::ops::Range<usize>, &SpanFormat, &str)> = Vec::new();
            for (span, run) in block.spans() {
                let mut start = 0;
                let bytes = run.text.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    if bytes[i] == b' ' {
                        while i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        words.push((
                            span.start + start..span.start + i,
                            &run.format,
                            &run.text[start..i],
                        ));
                        start = i;
                    } else {
                        i += 1;
                    }
                }
                if start < bytes.len() {
                    words.push((
                        span.start + start..span.end,
                        &run.format,
                        &run.text[start..],
                    ));
                }
            }
            let empty_style = theme.style(block.kind, &SpanFormat::default());
            let mut line_pieces: Vec<RichPiece> = Vec::new();
            let mut x = 0.0;
            let empty_metrics = {
                let probe = shape(" ", &empty_style)?;
                let line = &probe.lines()[0];
                (line.ascent, line.descent + line.leading)
            };
            let flush = |pieces: &mut Vec<RichPiece>,
                         top: &mut f64,
                         layout: &mut RichLayout|
             -> Result<(), TextLayoutError> {
                let (ascent, descent) = if pieces.is_empty() {
                    empty_metrics
                } else {
                    pieces.iter().fold((0.0f64, 0.0f64), |(a, d), piece| {
                        let line = &piece.layout.lines()[0];
                        (a.max(line.ascent), d.max(line.descent + line.leading))
                    })
                };
                let line_width: f64 = pieces
                    .iter()
                    .map(|piece| piece.layout.size().width)
                    .sum::<f64>()
                    - trailing_space(pieces.last());
                let shift = match (block.alignment, rtl) {
                    (BlockAlignment::Center, _) => (available - line_width) / 2.0,
                    (BlockAlignment::End, false) | (BlockAlignment::Start, true) => {
                        available - line_width
                    }
                    _ => 0.0,
                }
                .max(0.0);
                let baseline = *top + ascent;
                let first = layout.pieces.len();
                // RTL blocks place the first piece at the right.
                let mut cursor = 0.0;
                for mut piece in pieces.drain(..) {
                    let piece_width = piece.layout.size().width;
                    let line = &piece.layout.lines()[0];
                    let x = if rtl {
                        indent + shift + line_width - cursor - piece_width
                    } else {
                        indent + shift + cursor
                    };
                    piece.origin = Point::new(x, baseline - line.ascent);
                    cursor += piece_width;
                    layout.pieces.push(piece);
                }
                layout.lines.push(RichLine {
                    block: block_index,
                    top: *top,
                    baseline,
                    height: ascent + descent,
                    pieces: first..layout.pieces.len(),
                });
                *top += ascent + descent;
                Ok(())
            };
            if let BlockKind::Bullet | BlockKind::Numbered = block.kind {
                let marker = if block.kind == BlockKind::Bullet {
                    "•".to_owned()
                } else {
                    format!("{number}.")
                };
                let marker_layout = shape(&marker, &empty_style)?;
                layout.pieces.push(RichPiece {
                    block: block_index,
                    range: 0..0,
                    origin: Point::new(
                        if rtl {
                            width - indent + 4.0
                        } else {
                            indent - marker_layout.size().width - 6.0
                        },
                        top,
                    ),
                    layout: marker_layout,
                    color: theme.text,
                    format: SpanFormat::default(),
                    marker: true,
                });
            }
            let marker_index = layout
                .pieces
                .len()
                .checked_sub(1)
                .filter(|_| matches!(block.kind, BlockKind::Bullet | BlockKind::Numbered));
            let first_line = layout.lines.len();
            for (range, format, text) in words {
                let style = theme.style(block.kind, format);
                let piece_layout = shape(text, &style)?;
                let piece_width = piece_layout.size().width;
                let visible_width = piece_width - space_width(text, &piece_layout);
                if x > 0.0 && x + visible_width > available {
                    flush(&mut line_pieces, &mut top, &mut layout)?;
                    x = 0.0;
                }
                x += piece_width;
                line_pieces.push(RichPiece {
                    block: block_index,
                    range,
                    origin: Point::new(0.0, 0.0),
                    layout: piece_layout,
                    color: theme.color(block.kind, format),
                    format: format.clone(),
                    marker: false,
                });
            }
            flush(&mut line_pieces, &mut top, &mut layout)?;
            // Align the list marker with the first line's baseline.
            if let Some(index) = marker_index {
                let baseline = layout.lines[first_line].baseline;
                let marker = &mut layout.pieces[index];
                marker.origin.y = baseline - marker.layout.lines()[0].ascent;
            }
            match block.kind {
                BlockKind::Code => layout.decorations.push((
                    Rect::new(
                        Point::new(0.0, block_top),
                        Size::new(width, top - block_top),
                    ),
                    theme.code_background,
                )),
                BlockKind::Quote => layout.decorations.push((
                    Rect::new(
                        Point::new(if rtl { width - 3.0 } else { 0.0 }, block_top),
                        Size::new(3.0, top - block_top),
                    ),
                    theme.quote,
                )),
                _ => {}
            }
            let next_is_same_list = document
                .blocks()
                .get(block_index + 1)
                .is_some_and(|next| next.kind == block.kind && block.kind != BlockKind::Paragraph);
            if !next_is_same_list {
                top += theme.base.size * theme.block_spacing;
            }
        }
        layout.size.height = top;
        Ok(layout)
    }

    /// The document position nearest `point` (layout coordinates).
    pub fn hit_test(&self, point: Point) -> RichPosition {
        let Some(line) = self
            .lines
            .iter()
            .find(|line| point.y < line.top + line.height)
            .or(self.lines.last())
        else {
            return RichPosition::new(0, 0);
        };
        let pieces = &self.pieces[line.pieces.clone()];
        let mut best = (f64::INFINITY, RichPosition::new(line.block, 0));
        for piece in pieces {
            let local = Point::new(point.x - piece.origin.x, 0.0);
            let caret = piece.layout.hit_test(local);
            let offset = byte_for_grapheme(piece.layout.text(), caret);
            let x = piece
                .layout
                .caret_rect(caret)
                .map_or(0.0, |rect| rect.origin.x)
                + piece.origin.x;
            let distance = (x - point.x).abs();
            if distance < best.0 {
                best = (
                    distance,
                    RichPosition::new(piece.block, piece.range.start + offset),
                );
            }
        }
        best.1
    }

    /// Caret rectangle for a document position.
    pub fn caret_rect(&self, position: RichPosition) -> Option<Rect> {
        let mut fallback = None;
        for line in self
            .lines
            .iter()
            .filter(|line| line.block == position.block)
        {
            if line.pieces.is_empty() {
                fallback = Some(Rect::new(
                    Point::new(0.0, line.top),
                    Size::new(1.0, line.height),
                ));
            }
            for piece in &self.pieces[line.pieces.clone()] {
                if piece.range.start <= position.offset && position.offset <= piece.range.end {
                    let grapheme =
                        grapheme_for_byte(piece.layout.text(), position.offset - piece.range.start);
                    let rect = piece.layout.caret_rect(grapheme)?;
                    let at_end = position.offset == piece.range.end;
                    // Prefer the next piece's start for a boundary between pieces.
                    if at_end && fallback.is_none() {
                        fallback = Some(Rect::new(
                            Point::new(rect.origin.x + piece.origin.x, line.top),
                            Size::new(rect.size.width, line.height),
                        ));
                        continue;
                    }
                    return Some(Rect::new(
                        Point::new(rect.origin.x + piece.origin.x, line.top),
                        Size::new(rect.size.width, line.height),
                    ));
                }
            }
        }
        fallback
    }

    pub fn paint(&self, list: &mut DisplayList, origin: Point) -> Result<(), DisplayListError> {
        for (rect, color) in &self.decorations {
            list.fill_rect(rect.translate(origin.x, origin.y), *color);
        }
        for piece in &self.pieces {
            let top_left = Point::new(origin.x + piece.origin.x, origin.y + piece.origin.y);
            let line = &piece.layout.lines()[0];
            let baseline = Point::new(top_left.x + line.offset_x, top_left.y + line.baseline);
            if line.runs.is_empty() {
                let style = piece.layout.style();
                let text = piece.layout.text().trim_end();
                if !text.is_empty() {
                    list.draw_text(baseline, text, &style.family, style.size, piece.color)?;
                }
            } else {
                for run in &line.runs {
                    list.draw_glyphs(baseline, Arc::clone(run), piece.color)?;
                }
            }
            let width = piece.layout.size().width - space_width(piece.layout.text(), &piece.layout);
            let thickness = (piece.layout.style().size / 14.0).max(1.0);
            if piece.format.underline || piece.format.link.is_some() {
                list.fill_rect(
                    Rect::new(
                        Point::new(baseline.x, baseline.y + line.descent * 0.4),
                        Size::new(width, thickness),
                    ),
                    piece.color,
                );
            }
            if piece.format.strikethrough {
                list.fill_rect(
                    Rect::new(
                        Point::new(baseline.x, baseline.y - line.ascent * 0.35),
                        Size::new(width, thickness),
                    ),
                    piece.color,
                );
            }
        }
        Ok(())
    }

    /// The link under `point`, for pointer activation and cursors.
    pub fn link_at(&self, point: Point) -> Option<&str> {
        self.pieces.iter().find_map(|piece| {
            let rect = Rect::new(piece.origin, piece.layout.size());
            (point.x >= rect.min_x()
                && point.x < rect.max_x()
                && point.y >= rect.min_y()
                && point.y < rect.max_y())
            .then_some(piece.format.link.as_deref())
            .flatten()
        })
    }
}

fn trailing_space(piece: Option<&RichPiece>) -> f64 {
    piece.map_or(0.0, |piece| space_width(piece.layout.text(), &piece.layout))
}

/// Width of trailing spaces, from the layout's caret stops.
fn space_width(text: &str, layout: &TextLayout) -> f64 {
    let trimmed = text.trim_end_matches(' ');
    if trimmed.len() == text.len() {
        return 0.0;
    }
    let graphemes = crate::grapheme_boundaries(text).len() - 1;
    let kept = crate::grapheme_boundaries(trimmed).len() - 1;
    match (layout.caret_rect(kept), layout.caret_rect(graphemes)) {
        (Some(a), Some(b)) => (b.origin.x - a.origin.x).abs(),
        _ => 0.0,
    }
}

fn byte_for_grapheme(text: &str, grapheme: usize) -> usize {
    crate::grapheme_boundaries(text)
        .get(grapheme)
        .copied()
        .unwrap_or(text.len())
}

fn grapheme_for_byte(text: &str, byte: usize) -> usize {
    let boundaries = crate::grapheme_boundaries(text);
    match boundaries.binary_search(&byte) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    }
}
