//! The Cairo CPU painter. It paints a `DisplayList` into an ARGB32 image
//! surface, command by command as the macOS Core Graphics painter does.
//!
//! - Coordinates are logical points; the base transform scales them to
//!   device pixels.
//! - Each clip and transform push saves the Cairo state, and each opacity
//!   push opens a group that its pop paints with the opacity.
//! - Text draws the glyph runs of `LinuxTextShaper` with Cairo font faces
//!   on the same font files, so both text commands place glyphs alike.
//! - Shadows and backdrop blur use the core mask and blur, as the other
//!   painters do.

mod effects;
mod glyphs;

use crate::ffi::cairo::{CairoFailure, Context, Format, Pattern, Surface};
use choscorui_core::{
    Color, DisplayCommand, DisplayList, FillRule, Image, LineCap, LineJoin, Paint, Path,
    PathElement, Rect, Size, StrokeStyle, Transform,
};
use std::fmt;

/// Why painting failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CairoError {
    /// libcairo did not load.
    Library(crate::LoadError),
    /// The scale is not positive and finite.
    InvalidScale,
    /// The size, the stride, or the buffer length is not valid for Cairo.
    InvalidBuffer,
    /// Cairo reported an error, such as no memory.
    PaintFailed,
    /// A text command could not be drawn, because the font libraries do
    /// not load or no face was found. The other commands were painted.
    TextUnavailable,
}

impl fmt::Display for CairoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(error) => write!(f, "{error}"),
            Self::InvalidScale => f.write_str("the scale must be positive and finite"),
            Self::InvalidBuffer => f.write_str("the pixel buffer does not fit the size and stride"),
            Self::PaintFailed => f.write_str("Cairo could not paint the display list"),
            Self::TextUnavailable => {
                f.write_str("text could not be drawn with the installed fonts")
            }
        }
    }
}

impl std::error::Error for CairoError {}

impl From<CairoFailure> for CairoError {
    fn from(failure: CairoFailure) -> Self {
        match failure {
            CairoFailure::Library(error) => Self::Library(error),
            CairoFailure::BadSize => Self::InvalidBuffer,
            CairoFailure::Status => Self::PaintFailed,
        }
    }
}

/// Paints `list` into a caller's premultiplied ARGB32 buffer: `height`
/// rows of `stride` bytes, each pixel one native-endian word (wl_shm
/// ARGB8888, or a 32-bit X11 visual). `clip` is in logical points; the
/// painter clears the device pixels that cover it and paints only there.
/// `None` paints the whole buffer.
pub(crate) fn paint_into(
    list: &DisplayList,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    stride: usize,
    scale: f64,
    clip: Option<Rect>,
) -> Result<(), CairoError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(CairoError::InvalidScale);
    }
    if width == 0 || height == 0 {
        return Ok(());
    }
    let surface = Surface::for_data(pixels, Format::Argb32, width, height, stride)?;
    paint_surface(&surface, list, scale, clip)
}

/// Renders `list` at `size` points and `scale` pixels per point into a
/// new straight-alpha RGBA8 image, with the painter of the windows.
pub fn render_offscreen(list: &DisplayList, size: Size, scale: f64) -> Result<Image, CairoError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(CairoError::InvalidScale);
    }
    let pixels = |points: f64| {
        let value = (points * scale).ceil();
        (value.is_finite() && value >= 1.0 && value <= f64::from(Image::MAX_SIDE))
            .then_some(value as u32)
    };
    let (Some(width), Some(height)) = (pixels(size.width), pixels(size.height)) else {
        return Err(CairoError::InvalidBuffer);
    };
    let stride = Format::Argb32
        .stride_for_width(width)
        .ok_or(CairoError::InvalidBuffer)?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(stride * height as usize)
        .map_err(|_| CairoError::InvalidBuffer)?;
    buffer.resize(stride * height as usize, 0);
    paint_into(list, &mut buffer, width, height, stride, scale, None)?;
    let row = width as usize * 4;
    let rgba = buffer
        .chunks_exact(stride)
        .flat_map(|line| line[..row].as_chunks::<4>().0)
        .flat_map(|pixel| straight_rgba(u32::from_ne_bytes(*pixel)))
        .collect();
    Image::from_rgba8(width, height, rgba).map_err(|_| CairoError::InvalidBuffer)
}

/// A premultiplied ARGB32 word as straight-alpha RGBA8.
fn straight_rgba(word: u32) -> [u8; 4] {
    let [a, r, g, b] = word.to_be_bytes();
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
    [un(r), un(g), un(b), a]
}

/// Paints `list` into `surface`, inside the device pixels of `clip`.
fn paint_surface(
    surface: &Surface<'_>,
    list: &DisplayList,
    scale: f64,
    clip: Option<Rect>,
) -> Result<(), CairoError> {
    let context = Context::new(surface)?;
    let (width, height) = (surface.width() as f64, surface.height() as f64);
    let (x0, y0, x1, y1) = match clip {
        Some(rect) => (
            (rect.min_x() * scale).floor().clamp(0.0, width),
            (rect.min_y() * scale).floor().clamp(0.0, height),
            (rect.max_x() * scale).ceil().clamp(0.0, width),
            (rect.max_y() * scale).ceil().clamp(0.0, height),
        ),
        None => (0.0, 0.0, width, height),
    };
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    context.rectangle(x0, y0, x1 - x0, y1 - y0);
    context.clip();
    context.set_clear(true);
    context.paint();
    context.set_clear(false);
    context.scale(scale, scale);
    let mut painter = Painter {
        context: &context,
        surface,
        stack: Vec::new(),
        text_failed: false,
    };
    for command in list.commands() {
        painter.execute(command);
    }
    painter.unwind();
    if !context.ok() {
        Err(CairoError::PaintFailed)
    } else if painter.text_failed {
        Err(CairoError::TextUnavailable)
    } else {
        Ok(())
    }
}

/// What a push opened.
enum Pushed {
    /// A saved state, for a clip or a transform.
    State,
    /// An opacity group.
    Group(f64),
}

struct Painter<'a, 's> {
    context: &'a Context<'s>,
    surface: &'a Surface<'s>,
    stack: Vec<Pushed>,
    text_failed: bool,
}

impl Painter<'_, '_> {
    fn execute(&mut self, command: &DisplayCommand) {
        let cx = self.context;
        match command {
            DisplayCommand::FillRect { rect, color } => {
                set_color(cx, *color);
                cx.set_fill_rule(FillRule::NonZero);
                cx.new_path();
                cx.rectangle(
                    rect.min_x(),
                    rect.min_y(),
                    rect.size.width,
                    rect.size.height,
                );
                cx.fill();
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                set_color(cx, *color);
                cx.set_stroke(*width, LineCap::Butt, LineJoin::Miter, &[]);
                cx.new_path();
                cx.rectangle(
                    rect.min_x(),
                    rect.min_y(),
                    rect.size.width,
                    rect.size.height,
                );
                cx.stroke();
            }
            DisplayCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                set_color(cx, *color);
                fill(cx, &Path::rounded_rect(*rect, *radius), FillRule::NonZero);
            }
            DisplayCommand::StrokeRoundedRect {
                rect,
                radius,
                color,
                width,
            } => {
                set_color(cx, *color);
                cx.set_stroke(*width, LineCap::Butt, LineJoin::Miter, &[]);
                add_path(cx, &Path::rounded_rect(*rect, *radius));
                cx.stroke();
            }
            DisplayCommand::FillPath { path, paint, rule } => {
                if set_paint(cx, paint) {
                    fill(cx, path, *rule);
                }
            }
            DisplayCommand::StrokePath { path, paint, style } => {
                if set_paint(cx, paint) {
                    stroke(cx, path, style);
                }
            }
            DisplayCommand::DrawImage {
                image,
                rect,
                sampling,
            } => effects::draw_image(cx, image, *rect, *sampling),
            DisplayCommand::DrawShadow {
                rect,
                radius,
                shadow,
            } => effects::draw_shadow(cx, *rect, [*radius; 4], *shadow),
            DisplayCommand::DrawShadowCorners {
                rect,
                radii,
                shadow,
            } => effects::draw_shadow(cx, *rect, *radii, *shadow),
            DisplayCommand::BackdropBlur { rect, radius, blur } => {
                effects::backdrop_blur(cx, self.surface, *rect, [*radius; 4], *blur);
            }
            DisplayCommand::BackdropBlurCorners { rect, radii, blur } => {
                effects::backdrop_blur(cx, self.surface, *rect, *radii, *blur);
            }
            DisplayCommand::DrawText {
                baseline,
                text,
                font_family,
                font_size,
                font_weight,
                font_slant,
                tracking,
                color,
            } => {
                let run = choscorui_core::TextRun {
                    family: font_family,
                    size: *font_size,
                    weight: *font_weight,
                    slant: *font_slant,
                    tracking: *tracking,
                };
                if !glyphs::draw_text(cx, *baseline, text, &run, *color) {
                    self.text_failed = true;
                }
            }
            DisplayCommand::DrawGlyphs { origin, run, color } => {
                if !glyphs::draw_run(cx, *origin, run, *color) {
                    self.text_failed = true;
                }
            }
            DisplayCommand::PushClip(rect) => {
                cx.save();
                cx.new_path();
                cx.rectangle(
                    rect.min_x(),
                    rect.min_y(),
                    rect.size.width,
                    rect.size.height,
                );
                cx.clip();
                self.stack.push(Pushed::State);
            }
            DisplayCommand::PushClipPath(path) => {
                cx.save();
                add_path(cx, path);
                cx.set_fill_rule(FillRule::NonZero);
                cx.clip();
                self.stack.push(Pushed::State);
            }
            DisplayCommand::PushOpacity(opacity) => {
                cx.push_group();
                self.stack.push(Pushed::Group(*opacity));
            }
            DisplayCommand::PushTransform(transform) => {
                cx.save();
                cx.transform(matrix(*transform));
                self.stack.push(Pushed::State);
            }
            DisplayCommand::PopClip | DisplayCommand::PopOpacity | DisplayCommand::PopTransform => {
                self.pop();
            }
        }
    }

    fn pop(&mut self) {
        match self.stack.pop() {
            Some(Pushed::State) => self.context.restore(),
            Some(Pushed::Group(opacity)) => self.context.pop_group_with_alpha(opacity),
            None => {}
        }
    }

    /// Closes every push that the list left open.
    fn unwind(&mut self) {
        while !self.stack.is_empty() {
            self.pop();
        }
    }
}

/// A core transform as a Cairo matrix.
fn matrix(t: Transform) -> crate::ffi::cairo::Matrix {
    [t.a, t.b, t.c, t.d, t.tx, t.ty]
}

/// The channels of `color` in 0..=1.
fn channels(color: Color) -> [f64; 4] {
    [color.red, color.green, color.blue, color.alpha].map(|c| f64::from(c) / 255.0)
}

fn set_color(cx: &Context<'_>, color: Color) {
    cx.set_source_rgba(channels(color));
}

/// Sets `paint` as the source. Returns false when a gradient pattern
/// could not be made.
fn set_paint(cx: &Context<'_>, paint: &Paint) -> bool {
    let (pattern, stops) = match paint {
        Paint::Solid(color) => {
            set_color(cx, *color);
            return true;
        }
        Paint::LinearGradient { start, end, stops } => {
            (Pattern::linear((start.x, start.y), (end.x, end.y)), stops)
        }
        Paint::RadialGradient {
            center,
            radius,
            stops,
        } => (Pattern::radial((center.x, center.y), *radius), stops),
    };
    let Some(pattern) = pattern else {
        return false;
    };
    for stop in stops {
        pattern.add_stop(stop.offset, channels(stop.color));
    }
    cx.set_source(&pattern);
    true
}

/// Replaces the current path with `path`. A quadratic curve becomes the
/// equal cubic curve.
fn add_path(cx: &Context<'_>, path: &Path) {
    cx.new_path();
    let (mut current, mut start) = ((0.0, 0.0), (0.0, 0.0));
    for element in path.elements() {
        match *element {
            PathElement::MoveTo(p) => {
                cx.move_to(p.x, p.y);
                current = (p.x, p.y);
                start = current;
            }
            PathElement::LineTo(p) => {
                cx.line_to(p.x, p.y);
                current = (p.x, p.y);
            }
            PathElement::QuadTo(c, p) => {
                let a = (
                    current.0 + 2.0 / 3.0 * (c.x - current.0),
                    current.1 + 2.0 / 3.0 * (c.y - current.1),
                );
                let b = (p.x + 2.0 / 3.0 * (c.x - p.x), p.y + 2.0 / 3.0 * (c.y - p.y));
                cx.curve_to(a, b, (p.x, p.y));
                current = (p.x, p.y);
            }
            PathElement::CubicTo(a, b, p) => {
                cx.curve_to((a.x, a.y), (b.x, b.y), (p.x, p.y));
                current = (p.x, p.y);
            }
            PathElement::Close => {
                cx.close_path();
                current = start;
            }
        }
    }
}

fn fill(cx: &Context<'_>, path: &Path, rule: FillRule) {
    cx.set_fill_rule(rule);
    add_path(cx, path);
    cx.fill();
}

fn stroke(cx: &Context<'_>, path: &Path, style: &StrokeStyle) {
    cx.set_stroke(style.width, style.cap, style.join, &style.dash);
    add_path(cx, path);
    cx.stroke();
}

#[cfg(test)]
mod tests;
