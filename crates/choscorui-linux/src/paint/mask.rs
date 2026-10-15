//! Coverage masks for the GLES painter. Cairo rasterizes a fill, a
//! stroke, or glyphs in device pixels, and the GLES painter composites
//! the coverage with its paint. Both painters thus cover the same pixels.

use super::{add_path, glyphs};
use crate::ffi::cairo::{Context, Format, Matrix, Surface};
use choscorui_core::{Color, FillRule, GlyphRun, LineJoin, Path, Point, StrokeStyle};

/// What a mask covers, in user space.
pub(crate) enum Shape<'a> {
    Fill(&'a Path, FillRule),
    Stroke(&'a Path, &'a StrokeStyle),
    /// A glyph run with its baseline origin.
    Run(Point, &'a GlyphRun),
}

/// Why a mask was not made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaskFailure {
    /// Cairo did not load or ran out of memory.
    Cairo,
    /// The face of a glyph run did not open.
    Text,
}

/// The coverage of a shape in device pixels: one byte for each pixel of
/// `width` × `height`, in rows from the top, at `(x, y)`.
pub(crate) struct Mask {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) coverage: Vec<u8>,
}

impl Mask {
    /// The smallest mask with every covered pixel, or `None` when no
    /// pixel is covered.
    pub(crate) fn trimmed(self) -> Option<Self> {
        let (width, height) = (self.width as usize, self.height as usize);
        let covered = |x: usize, y: usize| self.coverage[y * width + x] != 0;
        let rows: Vec<usize> = (0..height)
            .filter(|&y| (0..width).any(|x| covered(x, y)))
            .collect();
        let (&top, &bottom) = (rows.first()?, rows.last()?);
        let columns: Vec<usize> = (0..width)
            .filter(|&x| (top..=bottom).any(|y| covered(x, y)))
            .collect();
        let (&left, &right) = (columns.first()?, columns.last()?);
        let coverage = (top..=bottom)
            .flat_map(|y| {
                self.coverage[y * width + left..=y * width + right]
                    .iter()
                    .copied()
            })
            .collect();
        Some(Self {
            x: self.x + left as i32,
            y: self.y + top as i32,
            width: (right - left + 1) as u32,
            height: (bottom - top + 1) as u32,
            coverage,
        })
    }
}

/// How far a stroke of `style` reaches past its path, in user units.
pub(crate) fn stroke_reach(style: &StrokeStyle) -> f64 {
    let half = style.width.abs() / 2.0;
    // Cairo's default miter limit is 10: a miter reaches at most five
    // widths past the path.
    match style.join {
        LineJoin::Miter => half * 10.0,
        _ => half * std::f64::consts::SQRT_2,
    }
}

/// Rasterizes `shape` with `matrix` from user space to device pixels,
/// in the device pixels `[x0, y0, x1, y1]`.
pub(crate) fn rasterize(
    shape: &Shape<'_>,
    matrix: Matrix,
    [x0, y0, x1, y1]: [i32; 4],
) -> Result<Mask, MaskFailure> {
    let (width, height) = (
        u32::try_from(x1 - x0).unwrap_or(0),
        u32::try_from(y1 - y0).unwrap_or(0),
    );
    if width == 0 || height == 0 {
        return Ok(Mask {
            x: x0,
            y: y0,
            width: 0,
            height: 0,
            coverage: Vec::new(),
        });
    }
    let surface = Surface::new(Format::Argb32, width, height).map_err(|_| MaskFailure::Cairo)?;
    let cx = Context::new(&surface).map_err(|_| MaskFailure::Cairo)?;
    cx.translate(-f64::from(x0), -f64::from(y0));
    cx.transform(matrix);
    cx.set_source_rgba([1.0; 4]);
    match shape {
        Shape::Fill(path, rule) => {
            cx.set_fill_rule(*rule);
            add_path(&cx, path);
            cx.fill();
        }
        Shape::Stroke(path, style) => {
            cx.set_stroke(style.width, style.cap, style.join, &style.dash);
            add_path(&cx, path);
            cx.stroke();
        }
        Shape::Run(origin, run) => {
            if !glyphs::draw_run(&cx, *origin, run, Color::WHITE) {
                return Err(MaskFailure::Text);
            }
        }
    }
    if !cx.ok() {
        return Err(MaskFailure::Cairo);
    }
    drop(cx);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    surface.copy_rows(0..height as usize, 0..width as usize * 4, &mut pixels);
    let coverage = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| (u32::from_ne_bytes(*pixel) >> 24) as u8)
        .collect::<Vec<u8>>();
    if coverage.len() != width as usize * height as usize {
        return Err(MaskFailure::Cairo);
    }
    Ok(Mask {
        x: x0,
        y: y0,
        width,
        height,
        coverage,
    })
}
