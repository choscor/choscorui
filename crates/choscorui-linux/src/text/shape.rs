//! Paragraph shaping: fonts per grapheme, items, line breaks, and the
//! `LayoutLine` of each line.
//!
//! 1. Each grapheme gets a face from the font chain and a bidi level.
//! 2. Graphemes with the same face and level form an item, which
//!    HarfBuzz shapes with the paragraph or the line as context.
//! 3. Grapheme advances of the paragraph choose the line breaks.
//! 4. Each line orders its items visually and places the glyphs. A
//!    cluster that covers several graphemes splits its width evenly, so
//!    each grapheme gets an extent and caret stops.

use super::bidi;
use super::breaks::{self, is_space};
use super::fonts::{Face, FontChain, SizedFont};
use crate::ffi::harfbuzz::{HbBuffer, ShapeRequest, ShapedGlyph};
use choscorui_core::{
    CaretStop, GlyphRun, LayoutLine, Point, Rect, Size, TextLayout, TextLayoutError, TextStyle,
    grapheme_boundaries,
};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

const UNAVAILABLE: TextLayoutError = TextLayoutError::Unavailable;

/// Shapes `text` into a validated layout.
pub(crate) fn shape_text(
    text: &str,
    style: &TextStyle,
    max_width: Option<f64>,
) -> Result<TextLayout, TextLayoutError> {
    style.validate()?;
    if max_width.is_some_and(|w| !w.is_finite() || w <= 0.0) {
        return Err(TextLayoutError::InvalidWidth);
    }
    let chain = super::fonts::chain(style).ok_or(UNAVAILABLE)?;
    let mut buffer = HbBuffer::new().ok_or(UNAVAILABLE)?;
    let source = grapheme_boundaries(text);
    let mut lines = Vec::new();
    let mut top = 0.0;
    let mut paragraph_start = 0usize;
    for raw in text.split('\n') {
        let start = paragraph_start;
        paragraph_start += raw.len() + 1;
        let paragraph = raw.strip_suffix('\r').unwrap_or(raw);
        let first = source.binary_search(&start).map_err(|_| UNAVAILABLE)?;
        let mut shaper = Paragraph {
            text: paragraph,
            byte_start: start,
            grapheme_start: first,
            chain: &chain,
            style,
            buffer: &mut buffer,
        };
        shaper.shape(max_width, &mut top, &mut lines)?;
    }
    TextLayout::from_lines(text, style.clone(), max_width, lines)
}

/// One shaped item: graphemes with one face and one level.
struct Item {
    font: Rc<SizedFont>,
    face: Rc<Face>,
    level: u8,
    /// Local grapheme range in the paragraph.
    graphemes: Range<usize>,
    /// Glyphs in visual order.
    glyphs: Vec<ShapedGlyph>,
}

impl Item {
    fn right_to_left(&self) -> bool {
        self.level % 2 == 1
    }
}

/// A cluster: contiguous glyphs that cover a range of graphemes.
struct Cluster {
    glyphs: Range<usize>,
    graphemes: Range<usize>,
}

/// The state of one paragraph.
struct Paragraph<'a> {
    text: &'a str,
    byte_start: usize,
    grapheme_start: usize,
    chain: &'a FontChain,
    style: &'a TextStyle,
    buffer: &'a mut HbBuffer,
}

impl Paragraph<'_> {
    fn shape(
        &mut self,
        max_width: Option<f64>,
        top: &mut f64,
        lines: &mut Vec<LayoutLine>,
    ) -> Result<(), TextLayoutError> {
        let rtl = self.style.direction.is_rtl_for(self.text);
        let primary = self
            .chain
            .primary
            .sized(self.style.size)
            .ok_or(UNAVAILABLE)?;
        if self.text.is_empty() {
            let line = self.empty_line(&primary, rtl, max_width, *top);
            *top += line.height() * self.style.line_height;
            lines.push(line);
            return Ok(());
        }
        let bounds = grapheme_boundaries(self.text);
        let graphemes: Vec<&str> = bounds
            .windows(2)
            .map(|pair| &self.text[pair[0]..pair[1]])
            .collect();
        let faces = self.faces(&graphemes);
        let levels = bidi::levels(&graphemes, rtl);
        let items = self.items(&bounds, 0..graphemes.len(), &faces, &levels, None)?;
        let ends = match max_width {
            None => vec![graphemes.len()],
            Some(width) => {
                let advances = self.advances(&items, graphemes.len(), &bounds);
                breaks::line_ends(&graphemes, &advances, width)
            }
        };
        let mut start = 0;
        for end in ends {
            let mut line_levels = levels[start..end].to_vec();
            bidi::reset_trailing_whitespace(&mut line_levels, &graphemes[start..end], rtl);
            let whole = start == 0 && end == graphemes.len() && line_levels == levels;
            let line_items = if whole {
                None
            } else {
                let mut all_levels = levels.clone();
                all_levels[start..end].copy_from_slice(&line_levels);
                let context = bounds[start]..bounds[end];
                Some(self.items(&bounds, start..end, &faces, &all_levels, Some(context))?)
            };
            let line = self.build_line(
                line_items.as_deref().unwrap_or(&items),
                &bounds,
                &graphemes,
                start..end,
                &primary,
                rtl,
                max_width,
                *top,
            );
            *top += line.height() * self.style.line_height;
            lines.push(line);
            start = end;
        }
        Ok(())
    }

    /// The line of an empty paragraph, with the primary face's metrics.
    fn empty_line(
        &self,
        primary: &SizedFont,
        rtl: bool,
        max_width: Option<f64>,
        top: f64,
    ) -> LayoutLine {
        let start = self.byte_start;
        LayoutLine {
            bytes: start..start,
            graphemes: self.grapheme_start..self.grapheme_start,
            baseline: top + primary.ascent,
            ascent: primary.ascent,
            descent: primary.descent,
            leading: primary.leading,
            width: 0.0,
            offset_x: if rtl { max_width.unwrap_or(0.0) } else { 0.0 },
            right_to_left: rtl,
            runs: Vec::new(),
            carets: vec![CaretStop {
                primary: 0.0,
                secondary: 0.0,
            }],
            extents: Vec::new(),
        }
    }

    /// The face of each grapheme. A neutral grapheme stays in the face of
    /// the grapheme before it when that face has its characters.
    fn faces(&self, graphemes: &[&str]) -> Vec<Rc<Face>> {
        let mut faces: Vec<Rc<Face>> = Vec::with_capacity(graphemes.len());
        for grapheme in graphemes {
            let neutral = choscorui_core::first_strong_is_rtl(grapheme).is_none();
            let face = match faces.last() {
                Some(previous) if neutral && previous.covers(grapheme) => Rc::clone(previous),
                _ => self.chain.face_for(grapheme),
            };
            faces.push(face);
        }
        faces
    }

    /// Shapes the graphemes `range` as items. `context` limits the text
    /// that HarfBuzz sees around each item, such as to one line.
    fn items(
        &mut self,
        bounds: &[usize],
        range: Range<usize>,
        faces: &[Rc<Face>],
        levels: &[u8],
        context: Option<Range<usize>>,
    ) -> Result<Vec<Item>, TextLayoutError> {
        let context = context.unwrap_or(0..self.text.len());
        let mut items = Vec::new();
        let mut start = range.start;
        while start < range.end {
            let mut end = start + 1;
            while end < range.end
                && levels[end] == levels[start]
                && Rc::ptr_eq(&faces[end], &faces[start])
            {
                end += 1;
            }
            let face = Rc::clone(&faces[start]);
            let font = face.sized(self.style.size).ok_or(UNAVAILABLE)?;
            let request = ShapeRequest {
                text: &self.text[context.clone()],
                item: bounds[start] - context.start..bounds[end] - context.start,
                right_to_left: levels[start] % 2 == 1,
                language: self.style.locale.as_deref(),
            };
            let mut glyphs = self.buffer.shape(&font.hb, &request);
            for glyph in &mut glyphs {
                glyph.cluster += context.start;
            }
            items.push(Item {
                font,
                face,
                level: levels[start],
                graphemes: start..end,
                glyphs,
            });
            start = end;
        }
        Ok(items)
    }

    /// The width of each grapheme of the paragraph, with tracking.
    fn advances(&self, items: &[Item], count: usize, bounds: &[usize]) -> Vec<f64> {
        let mut advances = vec![0.0; count];
        for item in items {
            for cluster in clusters(item, bounds) {
                let width: f64 = item.glyphs[cluster.glyphs.clone()]
                    .iter()
                    .map(|glyph| glyph.x_advance)
                    .sum();
                let share = width / cluster.graphemes.len() as f64;
                for grapheme in cluster.graphemes {
                    advances[grapheme] = share + self.style.tracking;
                }
            }
        }
        advances
    }

    /// Places the items of one line and builds its `LayoutLine`.
    #[allow(clippy::too_many_arguments, reason = "the parts of one line, all read")]
    fn build_line(
        &self,
        items: &[Item],
        bounds: &[usize],
        graphemes: &[&str],
        range: Range<usize>,
        primary: &SizedFont,
        rtl: bool,
        max_width: Option<f64>,
        top: f64,
    ) -> LayoutLine {
        let (mut ascent, mut descent, mut leading) =
            (primary.ascent, primary.descent, primary.leading);
        let line_items: Vec<&Item> = items
            .iter()
            .filter(|item| item.graphemes.start < range.end && item.graphemes.end > range.start)
            .collect();
        let levels: Vec<u8> = line_items.iter().map(|item| item.level).collect();
        let mut extents: Vec<Option<(f64, f64)>> = vec![None; range.len()];
        let mut directions = vec![false; range.len()];
        let mut runs = Vec::new();
        let mut pen = 0.0;
        for index in bidi::visual_order(&levels) {
            let item = line_items[index];
            ascent = f64::max(ascent, item.font.ascent);
            descent = f64::max(descent, item.font.descent);
            leading = f64::max(leading, item.font.leading);
            for grapheme in item.graphemes.clone() {
                if range.contains(&grapheme) {
                    directions[grapheme - range.start] = item.right_to_left();
                }
            }
            let placed = self.place(item, bounds, &range, &mut pen, &mut extents);
            if let Some(run) = placed {
                runs.push(Arc::new(run));
            }
        }
        // A grapheme without glyphs keeps the pen position.
        let extents: Vec<(f64, f64)> = extents
            .into_iter()
            .map(|extent| extent.unwrap_or((pen, pen)))
            .collect();
        let carets = carets(&extents, &directions);
        let trailing: f64 = graphemes[range.clone()]
            .iter()
            .zip(&extents)
            .rev()
            .take_while(|(grapheme, _)| is_space(grapheme))
            .map(|(_, (left, right))| right - left)
            .sum();
        let width = (pen - trailing).max(0.0);
        let offset_x = match max_width {
            Some(w) if rtl => {
                let right = graphemes[range.clone()]
                    .iter()
                    .zip(&extents)
                    .filter(|(grapheme, _)| !is_space(grapheme))
                    .map(|(_, extent)| extent.1)
                    .fold(0.0, f64::max);
                w - right
            }
            _ => 0.0,
        };
        LayoutLine {
            bytes: self.byte_start + bounds[range.start]..self.byte_start + bounds[range.end],
            graphemes: self.grapheme_start + range.start..self.grapheme_start + range.end,
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
        }
    }

    /// Places the glyphs of `item` that belong to `range` from `pen`, and
    /// records the extent of each of its graphemes.
    fn place(
        &self,
        item: &Item,
        bounds: &[usize],
        range: &Range<usize>,
        pen: &mut f64,
        extents: &mut [Option<(f64, f64)>],
    ) -> Option<GlyphRun> {
        let left = *pen;
        let mut glyphs = Vec::new();
        let mut positions = Vec::new();
        let (mut first, mut last) = (usize::MAX, 0);
        for cluster in clusters(item, bounds) {
            if !range.contains(&cluster.graphemes.start) {
                continue;
            }
            let start = *pen;
            for glyph in &item.glyphs[cluster.glyphs.clone()] {
                glyphs.push(u16::try_from(glyph.glyph).unwrap_or(0));
                positions.push(Point::new(*pen + glyph.x_offset, -glyph.y_offset));
                *pen += glyph.x_advance;
            }
            let span = cluster.graphemes.len();
            *pen += self.style.tracking * span as f64;
            let share = (*pen - start) / span as f64;
            for (offset, grapheme) in cluster.graphemes.clone().enumerate() {
                let Some(slot) = grapheme
                    .checked_sub(range.start)
                    .and_then(|local| extents.get_mut(local))
                else {
                    continue;
                };
                let step = offset as f64 * share;
                *slot = Some(if item.right_to_left() {
                    (*pen - step - share, *pen - step)
                } else {
                    (start + step, start + step + share)
                });
            }
            first = first.min(cluster.graphemes.start);
            last = last.max(cluster.graphemes.end);
        }
        if glyphs.is_empty() {
            return None;
        }
        let last = last.min(range.end);
        Some(GlyphRun {
            font: item.face.name.clone(),
            size: self.style.size,
            glyphs,
            positions,
            text: self.byte_start + bounds[first]..self.byte_start + bounds[last],
            right_to_left: item.right_to_left(),
            bounds: Rect::new(
                Point::new(left, -item.font.ascent),
                Size::new(*pen - left, item.font.ascent + item.font.descent),
            ),
        })
    }
}

/// The local grapheme that contains byte `byte` of the paragraph.
fn grapheme_at(bounds: &[usize], byte: usize) -> usize {
    let count = bounds.len().saturating_sub(1);
    match bounds.binary_search(&byte) {
        Ok(index) => index.min(count.saturating_sub(1)),
        Err(index) => index.saturating_sub(1),
    }
}

/// The clusters of an item in visual order. The first grapheme of a
/// cluster is the one of its byte offset; it reaches up to the next
/// cluster in logical order.
fn clusters(item: &Item, bounds: &[usize]) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    for (index, glyph) in item.glyphs.iter().enumerate() {
        let grapheme = grapheme_at(bounds, glyph.cluster);
        match clusters.last_mut() {
            Some(cluster) if cluster.graphemes.start == grapheme => cluster.glyphs.end = index + 1,
            _ => clusters.push(Cluster {
                glyphs: index..index + 1,
                graphemes: grapheme..grapheme + 1,
            }),
        }
    }
    let mut starts: Vec<usize> = clusters.iter().map(|c| c.graphemes.start).collect();
    starts.sort_unstable();
    starts.dedup();
    for cluster in &mut clusters {
        let next = starts
            .iter()
            .copied()
            .find(|start| *start > cluster.graphemes.start)
            .unwrap_or(item.graphemes.end);
        cluster.graphemes.end = next.max(cluster.graphemes.start + 1);
    }
    clusters
}

/// The caret stops of a line: at each boundary, the leading edge of the
/// grapheme after it; at the end, the trailing edge of the last one. The
/// secondary stop is the trailing edge of the grapheme before.
fn carets(extents: &[(f64, f64)], right_to_left: &[bool]) -> Vec<CaretStop> {
    let leading = |index: usize| {
        let (left, right) = extents[index];
        if right_to_left[index] { right } else { left }
    };
    let trailing = |index: usize| {
        let (left, right) = extents[index];
        if right_to_left[index] { left } else { right }
    };
    (0..=extents.len())
        .map(|boundary| {
            let primary = if boundary < extents.len() {
                leading(boundary)
            } else if boundary > 0 {
                trailing(boundary - 1)
            } else {
                0.0
            };
            let secondary = if boundary > 0 {
                trailing(boundary - 1)
            } else {
                primary
            };
            CaretStop { primary, secondary }
        })
        .collect()
}
