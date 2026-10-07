//! Reference software renderer for display-list fixtures and offscreen output.
//!
//! It defines the expected pixels for the renderer-neutral display list and
//! is compared against the platform painters. Coverage is estimated with
//! 4×4 supersampling per pixel. Opacity groups composite as a whole layer.
//! Text is rendered only by platform painters and is rejected here.

use choscorui_core::{
    Color, DisplayCommand, DisplayList, FillRule, Image, ImageSampling, LineCap, LineJoin, Paint,
    Path, Point, Rect, Shadow, StrokeStyle, Transform,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    InvalidScale,
    TooLarge,
    UnsupportedText,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScale => f.write_str("the scale must be positive and finite"),
            Self::TooLarge => f.write_str("the raster is too large"),
            Self::UnsupportedText => f.write_str("the software renderer does not draw text"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Straight-alpha RGBA8 pixels in row-major order.
pub struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Premultiplied floating-point working buffer.
#[derive(Clone)]
struct Layer {
    data: Vec<[f32; 4]>,
}

impl Layer {
    fn new(len: usize) -> Self {
        Self {
            data: vec![[0.0; 4]; len],
        }
    }
}

enum Clip {
    Rect(Rect),
    Path(Path),
}

struct State {
    transform: Transform,
    clips: Vec<(Clip, Transform)>,
}

const SAMPLES: usize = 4;

impl Raster {
    pub fn render(
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::InvalidScale);
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .ok_or(RenderError::TooLarge)?;
        if len.checked_mul(16).is_none() || len > 64 * 1024 * 1024 {
            return Err(RenderError::TooLarge);
        }
        let mut renderer = Renderer {
            width,
            height,
            scale,
            layers: vec![(Layer::new(len), 1.0)],
            transforms: vec![Transform::IDENTITY],
            state: State {
                transform: Transform::IDENTITY,
                clips: Vec::new(),
            },
            clip_marks: Vec::new(),
        };
        if width > 0 && height > 0 {
            for command in list.commands() {
                renderer.execute(command)?;
            }
            while renderer.layers.len() > 1 {
                renderer.pop_layer();
            }
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len * 4)
            .map_err(|_| RenderError::TooLarge)?;
        for [r, g, b, a] in &renderer.layers[0].0.data {
            if *a <= 0.0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let un = |c: f32| ((c / a).clamp(0.0, 1.0) * 255.0).round() as u8;
                pixels.extend_from_slice(&[
                    un(*r),
                    un(*g),
                    un(*b),
                    (a.clamp(0.0, 1.0) * 255.0).round() as u8,
                ]);
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
        let index = ((y as usize) * (self.width as usize) + x as usize) * 4;
        Some(self.pixels[index..index + 4].try_into().unwrap())
    }

    /// Converts to an image for display or comparison.
    pub fn to_image(&self) -> Option<Image> {
        Image::from_rgba8(self.width, self.height, self.pixels.clone()).ok()
    }
}

/// Largest per-channel difference between two equally sized RGBA buffers,
/// ignoring color where both pixels are nearly transparent, and the share
/// of pixels that differ by more than `tolerance`.
pub fn compare(a: &[u8], b: &[u8], tolerance: u8) -> Option<(u8, f64)> {
    if a.len() != b.len() || !a.len().is_multiple_of(4) {
        return None;
    }
    let mut worst = 0u8;
    let mut differing = 0usize;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let both_clear = pa[3] < 8 && pb[3] < 8;
        let channels = if both_clear { 3..4 } else { 0..4 };
        let diff = channels.map(|i| pa[i].abs_diff(pb[i])).max().unwrap_or(0);
        worst = worst.max(diff);
        if diff > tolerance {
            differing += 1;
        }
    }
    Some((worst, differing as f64 / (a.len() / 4).max(1) as f64))
}

struct Renderer {
    width: u32,
    height: u32,
    scale: f64,
    layers: Vec<(Layer, f64)>,
    transforms: Vec<Transform>,
    state: State,
    clip_marks: Vec<usize>,
}

impl Renderer {
    fn execute(&mut self, command: &DisplayCommand) -> Result<(), RenderError> {
        match command {
            DisplayCommand::FillRect { rect, color } => {
                self.fill(&Path::rect(*rect), &Paint::Solid(*color), FillRule::NonZero)
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                let mut style = StrokeStyle::new(*width);
                style.join = LineJoin::Miter;
                self.stroke(&Path::rect(*rect), &Paint::Solid(*color), &style);
            }
            DisplayCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => self.fill(
                &Path::rounded_rect(*rect, *radius),
                &Paint::Solid(*color),
                FillRule::NonZero,
            ),
            DisplayCommand::StrokeRoundedRect {
                rect,
                radius,
                color,
                width,
            } => self.stroke(
                &Path::rounded_rect(*rect, *radius),
                &Paint::Solid(*color),
                &StrokeStyle::new(*width),
            ),
            DisplayCommand::FillPath { path, paint, rule } => self.fill(path, paint, *rule),
            DisplayCommand::StrokePath { path, paint, style } => self.stroke(path, paint, style),
            DisplayCommand::DrawImage {
                image,
                rect,
                sampling,
            } => self.image(image, *rect, *sampling),
            DisplayCommand::DrawShadow {
                rect,
                radius,
                shadow,
            } => self.shadow(*rect, *radius, *shadow),
            DisplayCommand::DrawText { .. } | DisplayCommand::DrawGlyphs { .. } => {
                return Err(RenderError::UnsupportedText);
            }
            DisplayCommand::PushClip(rect) => {
                self.clip_marks.push(self.state.clips.len());
                self.state
                    .clips
                    .push((Clip::Rect(*rect), self.state.transform));
            }
            DisplayCommand::PushClipPath(path) => {
                self.clip_marks.push(self.state.clips.len());
                self.state
                    .clips
                    .push((Clip::Path(path.clone()), self.state.transform));
            }
            DisplayCommand::PopClip => {
                if let Some(mark) = self.clip_marks.pop() {
                    self.state.clips.truncate(mark);
                }
            }
            DisplayCommand::PushOpacity(opacity) => {
                let len = self.layers[0].0.data.len();
                self.layers.push((Layer::new(len), *opacity));
            }
            DisplayCommand::PopOpacity => {
                if self.layers.len() > 1 {
                    self.pop_layer();
                }
            }
            DisplayCommand::PushTransform(transform) => {
                self.transforms.push(self.state.transform);
                self.state.transform = transform.then(self.state.transform);
            }
            DisplayCommand::PopTransform => {
                if let Some(previous) = self.transforms.pop() {
                    if self.transforms.is_empty() {
                        self.transforms.push(Transform::IDENTITY);
                    }
                    self.state.transform = previous;
                }
            }
        }
        Ok(())
    }

    fn pop_layer(&mut self) {
        let (layer, opacity) = self.layers.pop().expect("layer");
        let target = &mut self.layers.last_mut().expect("base layer").0;
        let o = opacity as f32;
        for (dst, src) in target.data.iter_mut().zip(layer.data) {
            let sa = src[3] * o;
            for i in 0..3 {
                dst[i] = src[i] * o + dst[i] * (1.0 - sa);
            }
            dst[3] = sa + dst[3] * (1.0 - sa);
        }
    }

    fn clip_contains(&self, point: Point) -> bool {
        self.state.clips.iter().all(|(clip, transform)| {
            let Some(inverse) = transform.invert() else {
                return false;
            };
            let local = inverse.apply(point);
            match clip {
                Clip::Rect(rect) => rect.contains(local),
                Clip::Path(path) => path.contains(local, FillRule::NonZero),
            }
        })
    }

    /// Paints device pixels covering `bounds` (logical, device space). For
    /// each subsample, `sample` returns the straight color to deposit.
    fn paint(&mut self, bounds: Rect, sample: impl Fn(Point) -> Option<Color>) {
        let scale = self.scale;
        let x0 = ((bounds.min_x() * scale).floor().max(0.0)) as u32;
        let y0 = ((bounds.min_y() * scale).floor().max(0.0)) as u32;
        let x1 = ((bounds.max_x() * scale).ceil().min(f64::from(self.width))).max(0.0) as u32;
        let y1 = ((bounds.max_y() * scale).ceil().min(f64::from(self.height))).max(0.0) as u32;
        let width = self.width as usize;
        let mut writes = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let mut acc = [0.0f32; 4];
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let point = Point::new(
                            (f64::from(x) + (sx as f64 + 0.5) / SAMPLES as f64) / scale,
                            (f64::from(y) + (sy as f64 + 0.5) / SAMPLES as f64) / scale,
                        );
                        if !self.clip_contains(point) {
                            continue;
                        }
                        if let Some(color) = sample(point) {
                            let a = f32::from(color.alpha) / 255.0;
                            acc[0] += f32::from(color.red) / 255.0 * a;
                            acc[1] += f32::from(color.green) / 255.0 * a;
                            acc[2] += f32::from(color.blue) / 255.0 * a;
                            acc[3] += a;
                        }
                    }
                }
                if acc[3] > 0.0 {
                    let n = (SAMPLES * SAMPLES) as f32;
                    writes.push((y as usize * width + x as usize, acc.map(|c| c / n)));
                }
            }
        }
        let layer = &mut self.layers.last_mut().expect("layer").0;
        for (index, src) in writes {
            let dst = &mut layer.data[index];
            let sa = src[3];
            for i in 0..3 {
                dst[i] = src[i] + dst[i] * (1.0 - sa);
            }
            dst[3] = sa + dst[3] * (1.0 - sa);
        }
    }

    fn fill(&mut self, path: &Path, paint: &Paint, rule: FillRule) {
        let transform = self.state.transform;
        let device = path.transformed(transform);
        let polys = device.flatten(0.05 / self.scale);
        let Some(inverse) = transform.invert() else {
            return;
        };
        let bounds = device.bounds();
        let paint = paint.clone();
        self.paint(bounds, |point| {
            winding_contains(&polys, point, rule).then(|| paint.color_at(inverse.apply(point)))
        });
    }

    fn stroke(&mut self, path: &Path, paint: &Paint, style: &StrokeStyle) {
        let transform = self.state.transform;
        let Some(inverse) = transform.invert() else {
            return;
        };
        // Stroke in local space so width follows the transform's scale.
        let polys = dash(path.flatten(0.05 / self.scale), &style.dash);
        let half = style.width / 2.0;
        let device_bounds = transform.apply_rect(path.bounds().inset(-style.width * 2.0));
        let paint = paint.clone();
        let cap = style.cap;
        let join = style.join;
        self.paint(device_bounds, |point| {
            let local = inverse.apply(point);
            stroke_contains(&polys, local, half, cap, join).then(|| paint.color_at(local))
        });
    }

    fn image(&mut self, image: &Image, rect: Rect, sampling: ImageSampling) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let transform = self.state.transform;
        let Some(inverse) = transform.invert() else {
            return;
        };
        let bounds = transform.apply_rect(rect);
        let image = image.clone();
        self.paint(bounds, |point| {
            let local = inverse.apply(point);
            if !rect.contains(local) {
                return None;
            }
            let u = (local.x - rect.min_x()) / rect.size.width * f64::from(image.width());
            let v = (local.y - rect.min_y()) / rect.size.height * f64::from(image.height());
            Some(sample_image(&image, u, v, sampling))
        });
    }

    fn shadow(&mut self, rect: Rect, radius: f64, shadow: Shadow) {
        // The mask is generated at the raster scale times the transform's
        // scale so it stays sharp, then drawn like an image.
        let scale = self.scale * self.state.transform.determinant().abs().sqrt().max(1e-6);
        if let Some(mask) = shadow.mask(rect, radius, scale) {
            self.image(&mask, shadow.bounds(rect), ImageSampling::Linear);
        }
    }
}

fn winding_contains(polys: &[(Vec<Point>, bool)], point: Point, rule: FillRule) -> bool {
    let mut winding = 0i32;
    for (poly, _) in polys {
        let n = poly.len();
        for i in 0..n {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            let cross = (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y);
            if a.y <= point.y {
                if b.y > point.y && cross > 0.0 {
                    winding += 1;
                }
            } else if b.y <= point.y && cross < 0.0 {
                winding -= 1;
            }
        }
    }
    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding % 2 != 0,
    }
}

/// Splits polylines into dashes. Closed paths become open dash pieces.
fn dash(polys: Vec<(Vec<Point>, bool)>, pattern: &[f64]) -> Vec<(Vec<Point>, bool)> {
    if pattern.is_empty() || pattern.iter().sum::<f64>() <= 0.0 {
        return polys;
    }
    let mut out = Vec::new();
    for (mut poly, closed) in polys {
        if closed {
            poly.push(poly[0]);
        }
        let mut index = 0;
        let mut remaining = pattern[0];
        let mut on = true;
        let mut piece = vec![poly[0]];
        for pair in poly.windows(2) {
            let (mut a, b) = (pair[0], pair[1]);
            let mut length = a.distance(b);
            while length > remaining {
                let t = remaining / length;
                let split = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                if on {
                    piece.push(split);
                    out.push((std::mem::take(&mut piece), false));
                } else {
                    piece = vec![split];
                }
                length -= remaining;
                a = split;
                on = !on;
                index = (index + 1) % pattern.len();
                remaining = pattern[index];
            }
            remaining -= length;
            if on {
                piece.push(b);
            }
        }
        if on && piece.len() > 1 {
            out.push((piece, false));
        }
    }
    out
}

fn stroke_contains(
    polys: &[(Vec<Point>, bool)],
    p: Point,
    half: f64,
    cap: LineCap,
    join: LineJoin,
) -> bool {
    for (poly, closed) in polys {
        let n = poly.len();
        let segments = if *closed { n } else { n - 1 };
        for i in 0..segments {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            if len2 == 0.0 {
                continue;
            }
            let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
            let len = len2.sqrt();
            let first = !closed && i == 0;
            let last = !closed && i == segments - 1;
            // Extend open ends for square caps.
            let ext = if cap == LineCap::Square {
                half / len
            } else {
                0.0
            };
            let lo = if first { -ext } else { 0.0 };
            let hi = if last { 1.0 + ext } else { 1.0 };
            if t >= lo && t <= hi {
                let proj = Point::new(a.x + dx * t, a.y + dy * t);
                if proj.distance(p) <= half {
                    return true;
                }
            }
        }
        // Joins and round caps.
        for (index, vertex) in poly.iter().enumerate() {
            let is_end = !closed && (index == 0 || index == n - 1);
            let round = if is_end {
                cap == LineCap::Round
            } else {
                true // interior vertices: round joins fill the gaps
            };
            if round && vertex.distance(p) <= half {
                return true;
            }
            if !is_end && join == LineJoin::Miter {
                // Miter: point lies in the wedge between the two offset
                // lines' extensions, bounded by a miter limit of 10.
                let prev = poly[(index + n - 1) % n];
                let next = poly[(index + 1) % n];
                if miter_contains(prev, *vertex, next, p, half) {
                    return true;
                }
            }
        }
    }
    false
}

fn miter_contains(prev: Point, vertex: Point, next: Point, p: Point, half: f64) -> bool {
    let d1 = normalize(Point::new(vertex.x - prev.x, vertex.y - prev.y));
    let d2 = normalize(Point::new(next.x - vertex.x, next.y - vertex.y));
    let (Some(d1), Some(d2)) = (d1, d2) else {
        return false;
    };
    let turn = d1.x * d2.y - d1.y * d2.x;
    if turn.abs() < 1e-9 {
        return false;
    }
    // Outer side normal.
    let side = if turn > 0.0 { -1.0 } else { 1.0 };
    let n1 = Point::new(-d1.y * side, d1.x * side);
    let n2 = Point::new(-d2.y * side, d2.x * side);
    let bis = normalize(Point::new(n1.x + n2.x, n1.y + n2.y));
    let Some(bis) = bis else {
        return false;
    };
    let cos_half = bis.x * n1.x + bis.y * n1.y;
    if cos_half <= 0.1 {
        return false; // beyond miter limit 10: bevel
    }
    let tip = half / cos_half;
    let rel = Point::new(p.x - vertex.x, p.y - vertex.y);
    let along = rel.x * bis.x + rel.y * bis.y;
    if along < 0.0 || along > tip {
        return false;
    }
    // Inside both offset half-planes' outer extents.
    let o1 = rel.x * n1.x + rel.y * n1.y;
    let o2 = rel.x * n2.x + rel.y * n2.y;
    o1 <= half + 1e-9 && o2 <= half + 1e-9 && o1 >= 0.0 && o2 >= 0.0
}

fn normalize(p: Point) -> Option<Point> {
    let len = (p.x * p.x + p.y * p.y).sqrt();
    (len > 0.0).then(|| Point::new(p.x / len, p.y / len))
}

fn sample_image(image: &Image, u: f64, v: f64, sampling: ImageSampling) -> Color {
    let fetch = |x: i64, y: i64| {
        let x = x.clamp(0, i64::from(image.width()) - 1) as u32;
        let y = y.clamp(0, i64::from(image.height()) - 1) as u32;
        image.pixel(x, y).expect("clamped")
    };
    match sampling {
        ImageSampling::Nearest => {
            let [r, g, b, a] = fetch(u.floor() as i64, v.floor() as i64);
            Color::rgba8(r, g, b, a)
        }
        ImageSampling::Linear => {
            let (x, y) = (u - 0.5, v - 0.5);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            let mut acc = [0.0f64; 4];
            for (dx, dy, w) in [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let p = fetch(x0 as i64 + dx, y0 as i64 + dy);
                let a = f64::from(p[3]) / 255.0;
                for i in 0..3 {
                    acc[i] += f64::from(p[i]) * a * w;
                }
                acc[3] += a * w;
            }
            if acc[3] <= 0.0 {
                return Color::TRANSPARENT;
            }
            Color::rgba8(
                (acc[0] / acc[3]).round() as u8,
                (acc[1] / acc[3]).round() as u8,
                (acc[2] / acc[3]).round() as u8,
                (acc[3] * 255.0).round() as u8,
            )
        }
    }
}
