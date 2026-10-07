//! Renderer-neutral text layout. A platform shaper turns text and a
//! [`TextStyle`] into a [`TextLayout`]: positioned glyph runs plus caret and
//! grapheme geometry. Measurement, hit testing, selection, and drawing all
//! read that one object, so what is measured is exactly what is painted.

use crate::{Color, DisplayList, DisplayListError, Point, Rect, Size};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// CSS-style numeric weight, 1..=1000.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const LIGHT: Self = Self(300);
    pub const REGULAR: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::REGULAR
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FontSlant {
    #[default]
    Upright,
    Italic,
}

/// Base direction of each paragraph. `Auto` uses the first strong character
/// (Unicode rule P2) and falls back to left-to-right.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

impl TextDirection {
    /// Resolves `Auto` against `paragraph`; returns true for right-to-left.
    pub fn is_rtl_for(self, paragraph: &str) -> bool {
        match self {
            Self::LeftToRight => false,
            Self::RightToLeft => true,
            Self::Auto => first_strong_is_rtl(paragraph).unwrap_or(false),
        }
    }
}

/// Direction of the first strong character, if any.
pub fn first_strong_is_rtl(text: &str) -> Option<bool> {
    text.chars().find_map(|ch| {
        let code = ch as u32;
        let rtl = matches!(code,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF);
        if rtl {
            Some(true)
        } else if ch.is_alphabetic() {
            Some(false)
        } else {
            None
        }
    })
}

/// Font and paragraph attributes for one block of uniformly styled text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub size: f64,
    pub weight: FontWeight,
    pub slant: FontSlant,
    /// Families tried in order, before the system cascade, when the primary
    /// font lacks a glyph.
    pub fallback: Vec<String>,
    pub direction: TextDirection,
    /// BCP 47 tag selecting locale-specific glyph forms (for example Han
    /// variants) and line-breaking rules.
    pub locale: Option<String>,
    /// Multiple of the font's natural line height; 1.0 keeps it unchanged.
    pub line_height: f64,
}

impl TextStyle {
    pub fn new(family: impl Into<String>, size: f64) -> Self {
        Self {
            family: family.into(),
            size,
            weight: FontWeight::REGULAR,
            slant: FontSlant::Upright,
            fallback: Vec::new(),
            direction: TextDirection::Auto,
            locale: None,
            line_height: 1.0,
        }
    }
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
    pub fn with_slant(mut self, slant: FontSlant) -> Self {
        self.slant = slant;
        self
    }
    pub fn with_fallback(mut self, families: &[&str]) -> Self {
        self.fallback = families.iter().map(|family| (*family).to_owned()).collect();
        self
    }
    pub fn with_direction(mut self, direction: TextDirection) -> Self {
        self.direction = direction;
        self
    }
    pub fn with_locale(mut self, tag: impl Into<String>) -> Self {
        self.locale = Some(tag.into());
        self
    }
    pub fn with_line_height(mut self, factor: f64) -> Self {
        self.line_height = factor;
        self
    }

    pub fn validate(&self) -> Result<(), TextLayoutError> {
        let valid = !self.family.is_empty()
            && self.size.is_finite()
            && self.size > 0.0
            && self.size <= 4096.0
            && (1..=1000).contains(&self.weight.0)
            && self.line_height.is_finite()
            && self.line_height > 0.0
            && self.fallback.iter().all(|family| !family.is_empty());
        if valid {
            Ok(())
        } else {
            Err(TextLayoutError::InvalidStyle)
        }
    }
}

// Validated styles never hold NaN, so bitwise equality is total.
impl Eq for TextStyle {}
impl Hash for TextStyle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.family.hash(state);
        self.size.to_bits().hash(state);
        self.weight.hash(state);
        self.slant.hash(state);
        self.fallback.hash(state);
        self.direction.hash(state);
        self.locale.hash(state);
        self.line_height.to_bits().hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutError {
    InvalidStyle,
    InvalidWidth,
    /// A shaper produced inconsistent geometry.
    InvalidLayout,
    /// The platform text system failed.
    Unavailable,
}

impl std::fmt::Display for TextLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStyle => {
                f.write_str("the text style has an invalid size, weight, or line height")
            }
            Self::InvalidWidth => f.write_str("the wrap width must be positive and finite"),
            Self::InvalidLayout => f.write_str("the shaper returned inconsistent lines"),
            Self::Unavailable => f.write_str("text shaping is unavailable for this font or text"),
        }
    }
}

impl std::error::Error for TextLayoutError {}

/// Glyphs from one resolved font, in visual order.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    /// Font actually used after fallback (PostScript name).
    pub font: String,
    pub size: f64,
    pub glyphs: Vec<u16>,
    /// Glyph origins relative to the line's baseline origin, y down.
    pub positions: Vec<Point>,
    /// Source byte range this run covers.
    pub text: Range<usize>,
    pub right_to_left: bool,
    /// Typographic box relative to the baseline origin.
    pub bounds: Rect,
}

impl GlyphRun {
    fn is_consistent(&self) -> bool {
        self.glyphs.len() == self.positions.len()
            && !self.font.is_empty()
            && self.size.is_finite()
            && self.size > 0.0
            && self.positions.iter().all(|p| p.is_finite())
            && self.bounds.is_finite()
    }
}

/// Caret x offsets at one grapheme boundary. `secondary` differs from
/// `primary` only where the boundary joins runs of opposite direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretStop {
    pub primary: f64,
    pub secondary: f64,
}

/// One visual line. Horizontal values are relative to `offset_x`, which
/// aligns the line within the layout width (right for RTL paragraphs).
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    /// Source byte range, excluding a terminating line break.
    pub bytes: Range<usize>,
    /// Source grapheme range, excluding a terminating line break.
    pub graphemes: Range<usize>,
    /// Baseline distance from the layout's top edge.
    pub baseline: f64,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
    /// Advance width without trailing whitespace.
    pub width: f64,
    pub offset_x: f64,
    pub right_to_left: bool,
    pub runs: Vec<Arc<GlyphRun>>,
    /// One stop per boundary from `graphemes.start` through `graphemes.end`.
    pub carets: Vec<CaretStop>,
    /// Visual horizontal extent of each grapheme on this line.
    pub extents: Vec<(f64, f64)>,
}

impl LayoutLine {
    pub fn top(&self) -> f64 {
        self.baseline - self.ascent
    }
    pub fn height(&self) -> f64 {
        self.ascent + self.descent + self.leading
    }
    fn caret(&self, grapheme: usize) -> Option<CaretStop> {
        self.carets
            .get(grapheme.checked_sub(self.graphemes.start)?)
            .copied()
    }
}

/// Shaped, positioned text. Construct through a [`TextShaper`].
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    text: String,
    style: TextStyle,
    max_width: Option<f64>,
    lines: Vec<LayoutLine>,
    size: Size,
    graphemes: usize,
}

impl TextLayout {
    /// Validates shaper output. Lines must be in source order, inside the
    /// text, and carry one caret per boundary and one extent per grapheme.
    pub fn from_lines(
        text: &str,
        style: TextStyle,
        max_width: Option<f64>,
        lines: Vec<LayoutLine>,
    ) -> Result<Self, TextLayoutError> {
        style.validate()?;
        if max_width.is_some_and(|w| !w.is_finite() || w <= 0.0) {
            return Err(TextLayoutError::InvalidWidth);
        }
        let graphemes = text.graphemes(true).count();
        let mut previous_end = 0;
        let mut width: f64 = 0.0;
        let mut bottom: f64 = 0.0;
        for line in &lines {
            let span = line.graphemes.len();
            let consistent = line.graphemes.start >= previous_end
                && line.graphemes.start <= line.graphemes.end
                && line.graphemes.end <= graphemes
                && line.bytes.end <= text.len()
                && text.is_char_boundary(line.bytes.start)
                && text.is_char_boundary(line.bytes.end)
                && line.carets.len() == span + 1
                && line.extents.len() == span
                && [
                    line.baseline,
                    line.ascent,
                    line.descent,
                    line.leading,
                    line.width,
                    line.offset_x,
                ]
                .iter()
                .all(|v| v.is_finite())
                && line.runs.iter().all(|run| run.is_consistent());
            if !consistent {
                return Err(TextLayoutError::InvalidLayout);
            }
            previous_end = line.graphemes.end;
            width = width.max(line.offset_x + line.width);
            bottom = bottom.max(line.top() + line.height() * style.line_height);
        }
        Ok(Self {
            text: text.to_owned(),
            style,
            max_width,
            lines,
            size: Size::new(width.max(0.0), bottom.max(0.0)),
            graphemes,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn style(&self) -> &TextStyle {
        &self.style
    }
    pub fn max_width(&self) -> Option<f64> {
        self.max_width
    }
    pub fn lines(&self) -> &[LayoutLine] {
        &self.lines
    }
    /// Bounding size of all lines, for layout measurement.
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn grapheme_count(&self) -> usize {
        self.graphemes
    }
    /// Baseline of the first line, for baseline alignment.
    pub fn first_baseline(&self) -> f64 {
        self.lines.first().map_or(0.0, |line| line.baseline)
    }
    /// Every resolved font, in first-use order (shows fallback decisions).
    pub fn fonts(&self) -> Vec<&str> {
        let mut fonts: Vec<&str> = Vec::new();
        for run in self.lines.iter().flat_map(|line| &line.runs) {
            if !fonts.contains(&run.font.as_str()) {
                fonts.push(&run.font);
            }
        }
        fonts
    }

    /// Line that shows `caret`. A boundary shared by a soft wrap belongs to
    /// the later line, matching where typing would insert text.
    pub fn line_for_caret(&self, caret: usize) -> Option<usize> {
        if caret > self.graphemes || self.lines.is_empty() {
            return None;
        }
        let mut found = None;
        for (index, line) in self.lines.iter().enumerate() {
            if line.graphemes.contains(&caret) {
                return Some(index);
            }
            if line.graphemes.end == caret {
                found = Some(index);
            } else if line.graphemes.start > caret {
                return Some(found.unwrap_or(index));
            }
        }
        found.or(Some(self.lines.len() - 1))
    }

    pub fn caret_rect(&self, caret: usize) -> Option<Rect> {
        let line = &self.lines[self.line_for_caret(caret)?];
        let local = caret.clamp(line.graphemes.start, line.graphemes.end);
        let stop = line.caret(local)?;
        Some(Rect::new(
            Point::new(line.offset_x + stop.primary, line.top()),
            Size::new(1.0, line.height()),
        ))
    }

    /// Nearest caret to a point relative to the layout origin.
    pub fn hit_test(&self, point: Point) -> usize {
        if !point.is_finite() || self.lines.is_empty() {
            return 0;
        }
        let index = self
            .lines
            .iter()
            .position(|line| point.y < line.top() + line.height())
            .unwrap_or(self.lines.len() - 1);
        let line = &self.lines[index];
        let mut best = line.graphemes.start;
        let mut distance = f64::INFINITY;
        for (offset, stop) in line.carets.iter().enumerate() {
            let current = (line.offset_x + stop.primary - point.x).abs();
            if current < distance {
                distance = current;
                best = line.graphemes.start + offset;
            }
        }
        best
    }

    /// Visual rectangles covering `selection` (grapheme indices). Mixed
    /// direction text yields one rectangle per contiguous visual span.
    pub fn selection_rects(&self, selection: Range<usize>) -> Vec<Rect> {
        let mut rects = Vec::new();
        if selection.start >= selection.end {
            return rects;
        }
        for line in &self.lines {
            let start = selection.start.max(line.graphemes.start);
            let end = selection.end.min(line.graphemes.end);
            let mut spans: Vec<(f64, f64)> = (start..end)
                .filter_map(|g| line.extents.get(g - line.graphemes.start).copied())
                .collect();
            // A selection that continues past a line end covers the break.
            if selection.end > line.graphemes.end && selection.start <= line.graphemes.end {
                let edge = if line.right_to_left {
                    -line.offset_x
                } else {
                    line.width
                        .max(line.carets.last().map_or(0.0, |c| c.primary))
                };
                spans.push((edge, edge + 4.0));
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f64, f64)> = Vec::new();
            for (left, right) in spans {
                match merged.last_mut() {
                    Some(last) if left <= last.1 + 0.5 => last.1 = last.1.max(right),
                    _ => merged.push((left, right)),
                }
            }
            for (left, right) in merged {
                rects.push(Rect::new(
                    Point::new(line.offset_x + left, line.top()),
                    Size::new(right - left, line.height()),
                ));
            }
        }
        rects
    }

    /// Moves one line up (`-1`) or down (`1`), keeping a preferred x.
    pub fn move_vertical(
        &self,
        caret: usize,
        direction: i32,
        preferred_x: Option<f64>,
    ) -> Option<(usize, f64)> {
        let current = self.line_for_caret(caret)?;
        let x = match preferred_x {
            Some(x) if x.is_finite() => x,
            _ => self.caret_rect(caret)?.origin.x,
        };
        let target = match direction.signum() {
            -1 if current == 0 => return Some((0, x)),
            1 if current + 1 == self.lines.len() => return Some((self.graphemes, x)),
            sign => (current as i64 + i64::from(sign)) as usize,
        };
        let line = &self.lines[target];
        Some((self.hit_test(Point::new(x, line.baseline)), x))
    }

    /// Appends one glyph command per run, with `origin` at the layout's
    /// top-left corner.
    pub fn draw(
        &self,
        list: &mut DisplayList,
        origin: Point,
        color: Color,
    ) -> Result<(), DisplayListError> {
        for line in &self.lines {
            let baseline = Point::new(origin.x + line.offset_x, origin.y + line.baseline);
            for run in &line.runs {
                list.draw_glyphs(baseline, Arc::clone(run), color)?;
            }
        }
        Ok(())
    }

    /// Approximate heap footprint, used by the cache budget.
    pub fn estimated_bytes(&self) -> usize {
        let runs: usize = self
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|run| run.glyphs.len() * 18 + run.font.len() + 96)
            .sum();
        let lines: usize = self
            .lines
            .iter()
            .map(|line| line.carets.len() * 16 + line.extents.len() * 16 + 160)
            .sum();
        self.text.len() + runs + lines + 128
    }
}

/// Platform text system entry point.
pub trait TextShaper {
    /// Shapes `text`. With `max_width`, lines wrap at break opportunities;
    /// a single grapheme wider than the width gets its own line.
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    pub bytes: usize,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    text: String,
    style: TextStyle,
    width: Option<u64>,
}

struct CacheEntry {
    layout: Arc<TextLayout>,
    tick: u64,
    bytes: usize,
}

/// Least-recently-used layout cache bounded by entry count and bytes.
/// Invalidate it when installed fonts, display scale, or the theme's font
/// metrics change; style and locale are part of each key.
pub struct TextLayoutCache {
    max_entries: usize,
    max_bytes: usize,
    entries: HashMap<CacheKey, CacheEntry>,
    order: BTreeMap<u64, CacheKey>,
    tick: u64,
    scale: f64,
    stats: TextCacheStats,
}

impl TextLayoutCache {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1),
            entries: HashMap::new(),
            order: BTreeMap::new(),
            tick: 0,
            scale: 1.0,
            stats: TextCacheStats::default(),
        }
    }

    pub fn layout(
        &mut self,
        shaper: &dyn TextShaper,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<Arc<TextLayout>, TextLayoutError> {
        style.validate()?;
        if max_width.is_some_and(|w| !w.is_finite() || w <= 0.0) {
            return Err(TextLayoutError::InvalidWidth);
        }
        let key = CacheKey {
            text: text.to_owned(),
            style: style.clone(),
            width: max_width.map(f64::to_bits),
        };
        self.tick += 1;
        if let Some(entry) = self.entries.get_mut(&key) {
            self.order.remove(&entry.tick);
            entry.tick = self.tick;
            self.order.insert(self.tick, key);
            self.stats.hits += 1;
            return Ok(Arc::clone(&entry.layout));
        }
        self.stats.misses += 1;
        let layout = Arc::new(shaper.shape(text, style, max_width)?);
        let bytes = layout.estimated_bytes() + text.len();
        self.stats.bytes += bytes;
        self.order.insert(self.tick, key.clone());
        self.entries.insert(
            key,
            CacheEntry {
                layout: Arc::clone(&layout),
                tick: self.tick,
                bytes,
            },
        );
        while self.entries.len() > 1
            && (self.entries.len() > self.max_entries || self.stats.bytes > self.max_bytes)
        {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.stats.bytes -= entry.bytes;
                self.stats.evictions += 1;
            }
        }
        self.stats.entries = self.entries.len();
        Ok(layout)
    }

    /// Lays out `text` one `\n`-separated paragraph at a time, reusing the
    /// cached layout of every unchanged paragraph, and joins them into one
    /// layout. Editing a long document then reshapes only the edited
    /// paragraph.
    ///
    /// The result equals `shaper.shape(text, ..)` for shapers that lay out
    /// paragraphs independently and stack lines by `height × line_height`,
    /// as [`TextShaper`] implementations in this workspace do. Size the cache
    /// for the document's paragraph count, or paragraphs are reshaped as
    /// they are evicted. The joined layout itself is not cached.
    pub fn layout_paragraphs(
        &mut self,
        shaper: &dyn TextShaper,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<Arc<TextLayout>, TextLayoutError> {
        if !text.contains('\n') {
            return self.layout(shaper, text, style, max_width);
        }
        let mut lines = Vec::new();
        let (mut byte_offset, mut grapheme_offset, mut top) = (0usize, 0usize, 0.0f64);
        for raw in text.split('\n') {
            let paragraph = self.layout(shaper, raw, style, max_width)?;
            for line in paragraph.lines() {
                let mut line = line.clone();
                if byte_offset > 0 {
                    line.bytes = line.bytes.start + byte_offset..line.bytes.end + byte_offset;
                    line.runs = line
                        .runs
                        .iter()
                        .map(|run| {
                            let mut run = GlyphRun::clone(run);
                            run.text = run.text.start + byte_offset..run.text.end + byte_offset;
                            Arc::new(run)
                        })
                        .collect();
                }
                line.graphemes =
                    line.graphemes.start + grapheme_offset..line.graphemes.end + grapheme_offset;
                line.baseline += top;
                lines.push(line);
            }
            // A trailing `\r` joins the `\n` into one grapheme.
            let own = raw.strip_suffix('\r').unwrap_or(raw);
            grapheme_offset += own.graphemes(true).count() + 1;
            byte_offset += raw.len() + 1;
            top += paragraph
                .lines()
                .iter()
                .map(|line| line.height() * style.line_height)
                .sum::<f64>();
        }
        TextLayout::from_lines(text, style.clone(), max_width, lines).map(Arc::new)
    }

    /// Drops every layout using `family` as primary or fallback font.
    pub fn invalidate_family(&mut self, family: &str) {
        self.retain(|key| {
            key.style.family != family && !key.style.fallback.iter().any(|f| f == family)
        });
    }

    pub fn invalidate_all(&mut self) {
        self.retain(|_| false);
    }

    /// Records the display scale; a change drops every layout because
    /// hinting and optical sizes may differ.
    pub fn set_scale(&mut self, scale: f64) {
        if scale.is_finite() && scale > 0.0 && scale != self.scale {
            self.scale = scale;
            self.invalidate_all();
        }
    }

    pub fn stats(&self) -> TextCacheStats {
        self.stats
    }

    fn retain(&mut self, keep: impl Fn(&CacheKey) -> bool) {
        let mut freed = 0;
        self.entries.retain(|key, entry| {
            let kept = keep(key);
            if !kept {
                freed += entry.bytes;
            }
            kept
        });
        self.order.retain(|_, key| keep(key));
        self.stats.bytes -= freed;
        self.stats.entries = self.entries.len();
    }
}
