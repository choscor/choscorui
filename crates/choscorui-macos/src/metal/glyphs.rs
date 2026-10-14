//! Text for the Metal painter: shaped lines, the fonts of their runs, and
//! glyph atlases that Core Text rasterizes into.
//!
//! A glyph is keyed by font, glyph, pixel size, offset in the pixel, and
//! dark or light ink. Outline glyphs keep their coverage in
//! an alpha atlas; glyphs of color fonts keep premultiplied colors in a
//! color atlas. A full atlas moves the glyphs that plans used most
//! recently, up to half of the atlas, into a new texture and drops the
//! others. Frames that still read the old texture keep their glyphs.

use super::Gpu;
use super::coverage::Shelves;
use crate::ShapedLine;
use crate::ffi::cf::CfRetained;
use crate::ffi::geometry::{NSPoint, NSRect, NSSize};
use crate::ffi::graphics::{
    CGContextFillRect, CGContextScaleCTM, CGContextSetAllowsFontSmoothing,
    CGContextSetRGBFillColor, CGContextSetShouldSmoothFonts,
    CGContextSetShouldSubpixelQuantizeFonts, PREMULTIPLIED_RGBA, bitmap_context,
};
use crate::ffi::metal::{
    MTLOrigin, MTLRegion, MTLSize, MTLTexture, PIXEL_FORMAT_R8, PIXEL_FORMAT_RGBA8, read_region,
    replace_region,
};
use crate::ffi::retained::Retained;
use crate::ffi::text::{
    CTFont, CTFontCopyPostScriptName, CTFontGetSize, CTFontGetSymbolicTraits,
    FONT_TRAIT_COLOR_GLYPHS,
};
use crate::text_layout::run_font;
use choscorui_core::{FontSlant, Point, TextRun};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

/// The offsets of a glyph in each pixel, across and down. Core Graphics
/// places glyphs at fractions of a pixel. Offsets across change the look
/// of stems most, so they have finer steps.
pub(super) const SUBPIXELS: [u8; 2] = [16, 4];

/// The lines that each generation of the line cache keeps.
const LINE_LIMIT: usize = 4096;
const FONT_LIMIT: usize = 256;
const MONO_ATLAS: u32 = 2048;
const COLOR_ATLAS: u32 = 1024;

/// A font of the registry.
pub(super) type FontId = u32;

/// One run of a shaped line.
pub(super) struct LineRun {
    pub(super) font: FontId,
    pub(super) glyphs: Vec<u16>,
    /// Glyph origins relative to the baseline origin, y down.
    pub(super) positions: Vec<Point>,
}

/// Where a glyph is in its atlas and how it sits on its pen position.
#[derive(Clone, Copy, Debug)]
pub(super) struct GlyphEntry {
    pub(super) color: bool,
    /// Left, top, right, and bottom texels in the atlas.
    pub(super) texels: [u32; 4],
    /// The offset of the bitmap from the whole pixel of the pen.
    pub(super) left: i32,
    pub(super) top: i32,
}

/// The style and text of a shaped line.
struct LineKey {
    text: String,
    family: String,
    style: LineStyle,
}

/// The size, weight, slant, and tracking of a shaped line.
type LineStyle = (u64, u16, FontSlant, u64);

impl LineKey {
    fn matches(&self, text: &str, family: &str, style: LineStyle) -> bool {
        self.text == text && self.family == family && self.style == style
    }
}

/// The hash of a line, so a lookup needs no owned key.
fn line_hash(text: &str, family: &str, style: LineStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    (text, family, style).hash(&mut hasher);
    hasher.finish()
}

/// Shaped lines by hash, in two generations. A line that the current
/// generation lacks moves up from the previous one. When the current
/// generation is full, it becomes the previous one, so the lines of
/// recent frames stay.
#[derive(Default)]
struct Lines {
    current: HashMap<u64, (LineKey, Rc<Vec<LineRun>>)>,
    previous: HashMap<u64, (LineKey, Rc<Vec<LineRun>>)>,
}

impl Lines {
    fn get(
        &mut self,
        hash: u64,
        text: &str,
        family: &str,
        style: LineStyle,
    ) -> Option<Rc<Vec<LineRun>>> {
        if let Some((key, runs)) = self.current.get(&hash)
            && key.matches(text, family, style)
        {
            return Some(Rc::clone(runs));
        }
        let (key, runs) = self.previous.remove(&hash)?;
        if !key.matches(text, family, style) {
            return None;
        }
        let found = Rc::clone(&runs);
        self.insert(hash, key, runs);
        Some(found)
    }

    fn insert(&mut self, hash: u64, key: LineKey, runs: Rc<Vec<LineRun>>) {
        if self.current.len() >= LINE_LIMIT {
            self.previous = std::mem::take(&mut self.current);
        }
        self.current.insert(hash, (key, runs));
    }

    fn clear(&mut self) {
        self.current.clear();
        self.previous.clear();
    }
}

struct Atlas {
    texture: Option<Retained<MTLTexture>>,
    shelves: Shelves,
    size: u32,
    format: usize,
    bytes_per_pixel: usize,
}

impl Atlas {
    fn new(size: u32, format: usize, bytes_per_pixel: usize) -> Self {
        Self {
            texture: None,
            shelves: Shelves::new(size),
            size,
            format,
            bytes_per_pixel,
        }
    }

    /// Starts over in a new texture.
    fn reset(&mut self) {
        self.texture = None;
        self.shelves.clear();
    }

    /// Copies a `width × height` bitmap into free texels. Returns their
    /// top-left corner, or `None` when the atlas is full.
    fn insert(&mut self, gpu: &Gpu, width: u32, height: u32, bytes: &[u8]) -> Option<(u32, u32)> {
        let (x, y) = self.shelves.allocate(width, height)?;
        if self.texture.is_none() {
            self.texture = gpu.upload_texture(self.format, self.size as usize, self.size as usize);
        }
        let texture = self.texture.as_ref()?;
        let region = MTLRegion {
            origin: MTLOrigin::new(x as usize, y as usize),
            size: MTLSize::new(width as usize, height as usize),
        };
        let row = width as usize * self.bytes_per_pixel;
        replace_region(texture, region, bytes, row, self.bytes_per_pixel).then_some((x, y))
    }
}

/// How a glyph draws into the atlas. Core Graphics smooths dark text
/// and light text by different amounts, so each has its own entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Ink {
    /// An outline glyph in a color darker than middle gray.
    Dark,
    /// An outline glyph in a lighter color.
    Light,
    /// A glyph of a color font.
    Color,
}

/// A glyph in the atlas: font, glyph, pixel size bits, offset, and ink.
type GlyphKey = (FontId, u16, u64, [u8; 2], Ink);

/// A glyph lookup: the atlas entry, or `None` for a glyph that draws
/// nothing, and the plan that used it last.
#[derive(Clone, Copy)]
struct GlyphSlot {
    entry: Option<GlyphEntry>,
    used: u64,
}

/// The indices of the glyphs to keep when an atlas of `capacity` texels
/// is full: the most recently used first, up to half of the capacity.
/// Each glyph is its texel count and the plan that used it last.
fn glyphs_to_keep(glyphs: &[(u32, u64)], capacity: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..glyphs.len()).collect();
    order.sort_by_key(|&index| Reverse(glyphs[index].1));
    let mut area = 0u64;
    order
        .into_iter()
        .take_while(|&index| {
            area += u64::from(glyphs[index].0);
            area <= capacity / 2
        })
        .collect()
}

/// The text caches of the process. Only the main thread uses them.
pub(super) struct Text {
    fonts: Vec<(CfRetained<CTFont>, bool)>,
    font_ids: HashMap<(String, u64), FontId>,
    /// The fonts of pre-shaped runs, apart from the fonts of shaped lines,
    /// because a PostScript name does not always find the font of a line.
    run_font_ids: HashMap<String, HashMap<u64, FontId>>,
    lines: Lines,
    glyphs: HashMap<GlyphKey, GlyphSlot>,
    /// The number of plans so far, which [`GlyphSlot::used`] counts in.
    plan: u64,
    mono: Atlas,
    color: Atlas,
}

impl Text {
    pub(super) fn new() -> Self {
        Self {
            fonts: Vec::new(),
            font_ids: HashMap::new(),
            run_font_ids: HashMap::new(),
            lines: Lines::default(),
            glyphs: HashMap::new(),
            plan: 0,
            mono: Atlas::new(MONO_ATLAS, PIXEL_FORMAT_R8, 1),
            color: Atlas::new(COLOR_ATLAS, PIXEL_FORMAT_RGBA8, 4),
        }
    }

    /// Starts the glyph lookups of a new plan.
    pub(super) fn start_plan(&mut self) {
        self.plan += 1;
    }

    /// The atlas texture of outline glyphs or of color glyphs.
    pub(super) fn atlas(&self, color: bool) -> Option<&Retained<MTLTexture>> {
        if color {
            self.color.texture.as_ref()
        } else {
            self.mono.texture.as_ref()
        }
    }

    /// The registry id of `font`. The caller keeps the id by name.
    fn register(&mut self, font: CfRetained<CTFont>) -> FontId {
        if self.fonts.len() >= FONT_LIMIT {
            // Lines and glyphs name fonts by id, so they start over too.
            self.fonts.clear();
            self.font_ids.clear();
            self.run_font_ids.clear();
            self.lines.clear();
            self.glyphs.clear();
            self.mono.reset();
            self.color.reset();
        }
        let color = CTFontGetSymbolicTraits(&font) & FONT_TRAIT_COLOR_GLYPHS != 0;
        let id = self.fonts.len() as FontId;
        self.fonts.push((font, color));
        id
    }

    /// The font of a pre-shaped glyph run, by PostScript name. It is the
    /// font that Core Graphics draws the run with.
    pub(super) fn named_font(&mut self, name: &str, size: f64) -> Option<FontId> {
        if let Some(id) = self
            .run_font_ids
            .get(name)
            .and_then(|sizes| sizes.get(&size.to_bits()))
        {
            return Some(*id);
        }
        let id = self.register(run_font(name, size)?);
        self.run_font_ids
            .entry(name.to_owned())
            .or_default()
            .insert(size.to_bits(), id);
        Some(id)
    }

    /// The runs of a line of `text`, shaped as `DrawText` shapes it.
    pub(super) fn line(&mut self, text: &str, run: &TextRun<'_>) -> Option<Rc<Vec<LineRun>>> {
        let family = run.family;
        let style = (
            run.size.to_bits(),
            run.weight.0,
            run.slant,
            run.tracking.to_bits(),
        );
        let hash = line_hash(text, family, style);
        if let Some(runs) = self.lines.get(hash, text, family, style) {
            return Some(runs);
        }
        let shaped = ShapedLine::styled(text, run).ok()?;
        let mut runs = Vec::new();
        for (font, glyphs) in shaped.glyph_runs() {
            let name = CTFontCopyPostScriptName(&font)?.to_rust()?;
            let key = (name, CTFontGetSize(&font).to_bits());
            let id = match self.font_ids.get(&key) {
                Some(id) => *id,
                None => {
                    let id = self.register(font);
                    self.font_ids.insert(key, id);
                    id
                }
            };
            runs.push(LineRun {
                font: id,
                glyphs: glyphs.glyphs,
                positions: glyphs
                    .positions
                    .iter()
                    .map(|p| Point::new(p.x, -p.y))
                    .collect(),
            });
        }
        let runs = Rc::new(runs);
        let key = LineKey {
            text: text.to_owned(),
            family: family.to_owned(),
            style,
        };
        self.lines.insert(hash, key, Rc::clone(&runs));
        Some(runs)
    }

    /// The atlas entry of `glyph` of `font` at `scale` pixels per point,
    /// shifted right and down by `subpixel` steps of [`SUBPIXELS`], for
    /// light or dark text. `None` means that the glyph draws nothing.
    pub(super) fn glyph(
        &mut self,
        gpu: &Gpu,
        font: FontId,
        glyph: u16,
        scale: f64,
        subpixel: [u8; 2],
        light: bool,
    ) -> Option<GlyphEntry> {
        let (ct_font, color) = self.fonts.get(font as usize)?;
        let color = *color;
        let ink = match (color, light) {
            (true, _) => Ink::Color,
            (false, true) => Ink::Light,
            (false, false) => Ink::Dark,
        };
        let key = (font, glyph, scale.to_bits(), subpixel, ink);
        let plan = self.plan;
        if let Some(slot) = self.glyphs.get_mut(&key) {
            slot.used = plan;
            return slot.entry;
        }
        let Some(bitmap) = rasterize(ct_font, glyph, scale, subpixel, ink) else {
            let slot = GlyphSlot {
                entry: None,
                used: plan,
            };
            self.glyphs.insert(key, slot);
            return None;
        };
        let atlas = if color {
            &mut self.color
        } else {
            &mut self.mono
        };
        let corner = match atlas.insert(gpu, bitmap.width, bitmap.height, &bitmap.bytes) {
            Some(corner) => corner,
            None => {
                self.evict(gpu, color);
                let atlas = if color {
                    &mut self.color
                } else {
                    &mut self.mono
                };
                atlas.insert(gpu, bitmap.width, bitmap.height, &bitmap.bytes)?
            }
        };
        let entry = GlyphEntry {
            color,
            texels: [
                corner.0,
                corner.1,
                corner.0 + bitmap.width,
                corner.1 + bitmap.height,
            ],
            left: bitmap.left,
            top: bitmap.top,
        };
        let slot = GlyphSlot {
            entry: Some(entry),
            used: plan,
        };
        self.glyphs.insert(key, slot);
        Some(entry)
    }

    /// Makes room in the full atlas of color or of outline glyphs. The
    /// glyphs that plans used most recently move into a new texture, and
    /// the others go. The old texture stays with the plans that read it.
    fn evict(&mut self, gpu: &Gpu, color: bool) {
        let atlas = if color {
            &mut self.color
        } else {
            &mut self.mono
        };
        let old = atlas.texture.take();
        atlas.shelves.clear();
        let mut entries: Vec<(GlyphKey, GlyphEntry, u64)> = Vec::new();
        self.glyphs.retain(|key, slot| match slot.entry {
            Some(entry) if entry.color == color => {
                entries.push((*key, entry, slot.used));
                false
            }
            _ => true,
        });
        let Some(old) = old else {
            return;
        };
        let sizes: Vec<(u32, u64)> = entries
            .iter()
            .map(|(_, entry, used)| {
                let [left, top, right, bottom] = entry.texels;
                ((right - left) * (bottom - top), *used)
            })
            .collect();
        let capacity = u64::from(atlas.size) * u64::from(atlas.size);
        let mut keep = glyphs_to_keep(&sizes, capacity);
        // Taller glyphs first fill the shelves of the new atlas better.
        keep.sort_by_key(|&index| {
            let [_, top, _, bottom] = entries[index].1.texels;
            Reverse(bottom - top)
        });
        for index in keep {
            let (key, entry, used) = entries[index];
            let [left, top, right, bottom] = entry.texels;
            let (width, height) = (right - left, bottom - top);
            let region = MTLRegion {
                origin: MTLOrigin::new(left as usize, top as usize),
                size: MTLSize::new(width as usize, height as usize),
            };
            let Some(bytes) = read_region(&old, region, atlas.bytes_per_pixel) else {
                continue;
            };
            let Some((x, y)) = atlas.insert(gpu, width, height, &bytes) else {
                continue;
            };
            let entry = GlyphEntry {
                texels: [x, y, x + width, y + height],
                ..entry
            };
            let slot = GlyphSlot {
                entry: Some(entry),
                used,
            };
            self.glyphs.insert(key, slot);
        }
    }
}

struct Bitmap {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    bytes: Vec<u8>,
}

/// The largest side of a glyph bitmap in pixels.
const GLYPH_LIMIT: f64 = 1024.0;

/// Rasterizes one glyph with Core Text: white on transparent, so the
/// alpha of an outline glyph is its coverage.
fn rasterize(font: &CTFont, glyph: u16, scale: f64, subpixel: [u8; 2], ink: Ink) -> Option<Bitmap> {
    let bounds = font.glyph_bounds(glyph);
    if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 || scale.is_nan() || scale <= 0.0 {
        return None;
    }
    let shift = [
        f64::from(subpixel[0]) / f64::from(SUBPIXELS[0]),
        f64::from(subpixel[1]) / f64::from(SUBPIXELS[1]),
    ];
    let left = (bounds.origin.x * scale).floor() - 1.0;
    let right = ((bounds.origin.x + bounds.size.width) * scale + shift[0]).ceil() + 1.0;
    let top = (-(bounds.origin.y + bounds.size.height) * scale).floor() - 1.0;
    let bottom = (-bounds.origin.y * scale + shift[1]).ceil() + 1.0;
    let (width, height) = (right - left, bottom - top);
    if !(width.is_finite() && height.is_finite()) || width > GLYPH_LIMIT || height > GLYPH_LIMIT {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    let context = bitmap_context(width, height, PREMULTIPLIED_RGBA)?;
    // Font smoothing makes Core Graphics text heavier by an amount that
    // depends on the text and background colors. Outline glyphs draw on
    // an opaque background, so the atlas gets the same smoothing.
    CGContextSetAllowsFontSmoothing(&context, true);
    CGContextSetShouldSmoothFonts(&context, true);
    // The glyph goes at its offset, not at the nearest quarter pixel.
    CGContextSetShouldSubpixelQuantizeFonts(&context, false);
    let (paper, gray) = match ink {
        Ink::Dark => (1.0, 0.0),
        Ink::Light | Ink::Color => (0.0, 1.0),
    };
    if ink != Ink::Color {
        CGContextSetRGBFillColor(&context, paper, paper, paper, 1.0);
        let all = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContextFillRect(&context, all);
    }
    CGContextSetRGBFillColor(&context, gray, gray, gray, 1.0);
    CGContextScaleCTM(&context, scale, scale);
    // The bitmap is y up; the pen sits `-top` pixels below its top row.
    let pen = NSPoint {
        x: (-left + shift[0]) / scale,
        y: (height as f64 + top - shift[1]) / scale,
    };
    font.draw_glyphs(&[glyph], &[pen], &context);
    let mut rgba = Vec::new();
    if !context.copy_bitmap_rows(0..height, 0..width * 4, &mut rgba) {
        return None;
    }
    let bytes = match ink {
        Ink::Dark => rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| 255 - pixel[0])
            .collect(),
        Ink::Light => rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[0])
            .collect(),
        Ink::Color => rgba,
    };
    Some(Bitmap {
        width: width as u32,
        height: height as u32,
        left: left as i32,
        top: top as i32,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::glyphs_to_keep;

    #[test]
    fn a_full_atlas_keeps_the_most_recently_used_half() {
        // Texel counts and the plan of last use.
        let glyphs = [(30, 1), (30, 9), (30, 5), (30, 7), (30, 2)];
        // Half of 200 texels holds three glyphs of 30.
        assert_eq!(glyphs_to_keep(&glyphs, 200), vec![1, 3, 2]);
    }

    #[test]
    fn a_glyph_larger_than_half_the_atlas_is_not_kept() {
        assert!(glyphs_to_keep(&[(150, 3)], 200).is_empty());
        assert!(glyphs_to_keep(&[], 200).is_empty());
    }
}
