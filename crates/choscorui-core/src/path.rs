//! Vector paths, paints, stroke styles, and shared image handles used by
//! the display list.

use crate::{Color, Point, Rect, Transform};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathElement {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A sequence of subpaths in logical points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    elements: Vec<PathElement>,
}

/// Kappa for approximating a quarter circle with one cubic Bézier.
const KAPPA: f64 = 0.552_284_749_830_793_4;

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    /// Avoids regrowth while building fixed-size shapes.
    fn with_capacity(elements: usize) -> Self {
        Self {
            elements: Vec::with_capacity(elements),
        }
    }

    pub fn elements(&self) -> &[PathElement] {
        &self.elements
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn move_to(&mut self, point: Point) -> &mut Self {
        self.elements.push(PathElement::MoveTo(point));
        self
    }

    pub fn line_to(&mut self, point: Point) -> &mut Self {
        self.elements.push(PathElement::LineTo(point));
        self
    }

    pub fn quad_to(&mut self, control: Point, end: Point) -> &mut Self {
        self.elements.push(PathElement::QuadTo(control, end));
        self
    }

    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> &mut Self {
        self.elements.push(PathElement::CubicTo(c1, c2, end));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.elements.push(PathElement::Close);
        self
    }

    pub fn rect(rect: Rect) -> Self {
        let mut path = Self::with_capacity(5);
        path.move_to(rect.origin)
            .line_to(Point::new(rect.max_x(), rect.min_y()))
            .line_to(Point::new(rect.max_x(), rect.max_y()))
            .line_to(Point::new(rect.min_x(), rect.max_y()))
            .close();
        path
    }

    pub fn rounded_rect(rect: Rect, radius: f64) -> Self {
        let r = radius
            .min(rect.size.width / 2.0)
            .min(rect.size.height / 2.0)
            .max(0.0);
        if r == 0.0 {
            return Self::rect(rect);
        }
        let (l, t, rr, b) = (rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y());
        let k = r * KAPPA;
        let mut path = Self::new();
        path.move_to(Point::new(l + r, t))
            .line_to(Point::new(rr - r, t))
            .cubic_to(
                Point::new(rr - r + k, t),
                Point::new(rr, t + r - k),
                Point::new(rr, t + r),
            )
            .line_to(Point::new(rr, b - r))
            .cubic_to(
                Point::new(rr, b - r + k),
                Point::new(rr - r + k, b),
                Point::new(rr - r, b),
            )
            .line_to(Point::new(l + r, b))
            .cubic_to(
                Point::new(l + r - k, b),
                Point::new(l, b - r + k),
                Point::new(l, b - r),
            )
            .line_to(Point::new(l, t + r))
            .cubic_to(
                Point::new(l, t + r - k),
                Point::new(l + r - k, t),
                Point::new(l + r, t),
            )
            .close();
        path
    }

    pub fn ellipse(rect: Rect) -> Self {
        let c = rect.center();
        let (rx, ry) = (rect.size.width / 2.0, rect.size.height / 2.0);
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        let mut path = Self::with_capacity(6);
        path.move_to(Point::new(c.x + rx, c.y))
            .cubic_to(
                Point::new(c.x + rx, c.y + ky),
                Point::new(c.x + kx, c.y + ry),
                Point::new(c.x, c.y + ry),
            )
            .cubic_to(
                Point::new(c.x - kx, c.y + ry),
                Point::new(c.x - rx, c.y + ky),
                Point::new(c.x - rx, c.y),
            )
            .cubic_to(
                Point::new(c.x - rx, c.y - ky),
                Point::new(c.x - kx, c.y - ry),
                Point::new(c.x, c.y - ry),
            )
            .cubic_to(
                Point::new(c.x + kx, c.y - ry),
                Point::new(c.x + rx, c.y - ky),
                Point::new(c.x + rx, c.y),
            )
            .close();
        path
    }

    pub fn polygon(points: &[Point]) -> Self {
        let mut path = Self::new();
        if let Some((first, rest)) = points.split_first() {
            path.move_to(*first);
            for point in rest {
                path.line_to(*point);
            }
            path.close();
        }
        path
    }

    pub fn line(from: Point, to: Point) -> Self {
        let mut path = Self::new();
        path.move_to(from).line_to(to);
        path
    }

    /// Appends another path's subpaths.
    pub fn extend(&mut self, other: &Path) {
        self.elements.extend_from_slice(&other.elements);
    }

    pub fn transformed(&self, transform: Transform) -> Self {
        let map = |p: Point| transform.apply(p);
        Self {
            elements: self
                .elements
                .iter()
                .map(|element| match *element {
                    PathElement::MoveTo(p) => PathElement::MoveTo(map(p)),
                    PathElement::LineTo(p) => PathElement::LineTo(map(p)),
                    PathElement::QuadTo(c, p) => PathElement::QuadTo(map(c), map(p)),
                    PathElement::CubicTo(a, b, p) => PathElement::CubicTo(map(a), map(b), map(p)),
                    PathElement::Close => PathElement::Close,
                })
                .collect(),
        }
    }

    pub fn is_finite(&self) -> bool {
        self.elements.iter().all(|element| match *element {
            PathElement::MoveTo(p) | PathElement::LineTo(p) => p.is_finite(),
            PathElement::QuadTo(c, p) => c.is_finite() && p.is_finite(),
            PathElement::CubicTo(a, b, p) => a.is_finite() && b.is_finite() && p.is_finite(),
            PathElement::Close => true,
        })
    }

    /// Control-point bounds; contains the curve.
    pub fn bounds(&self) -> Rect {
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        let mut add = |p: Point| {
            bounds = Some(match bounds {
                None => (p.x, p.y, p.x, p.y),
                Some((x0, y0, x1, y1)) => (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
            });
        };
        for element in &self.elements {
            match *element {
                PathElement::MoveTo(p) | PathElement::LineTo(p) => add(p),
                PathElement::QuadTo(c, p) => {
                    add(c);
                    add(p);
                }
                PathElement::CubicTo(a, b, p) => {
                    add(a);
                    add(b);
                    add(p);
                }
                PathElement::Close => {}
            }
        }
        bounds.map_or(Rect::default(), |(x0, y0, x1, y1)| {
            Rect::from_xywh(x0, y0, x1 - x0, y1 - y0)
        })
    }

    /// Flattens curves into polylines within `tolerance` logical points.
    /// Each returned subpath carries whether it was explicitly closed.
    pub fn flatten(&self, tolerance: f64) -> Vec<(Vec<Point>, bool)> {
        let tolerance = tolerance.max(1e-3);
        let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
        let mut current: Vec<Point> = Vec::new();
        let mut start = Point::default();
        let finish = |current: &mut Vec<Point>, out: &mut Vec<(Vec<Point>, bool)>, closed| {
            if current.len() > 1 {
                out.push((std::mem::take(current), closed));
            } else {
                current.clear();
            }
        };
        for element in &self.elements {
            match *element {
                PathElement::MoveTo(p) => {
                    finish(&mut current, &mut out, false);
                    current.push(p);
                    start = p;
                }
                PathElement::LineTo(p) => {
                    if current.is_empty() {
                        current.push(start);
                    }
                    current.push(p);
                }
                PathElement::QuadTo(c, p) => {
                    let p0 = *current.last().unwrap_or(&start);
                    if current.is_empty() {
                        current.push(p0);
                    }
                    let steps = curve_steps(p0.distance(c) + c.distance(p), tolerance);
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let mt = 1.0 - t;
                        current.push(Point::new(
                            mt * mt * p0.x + 2.0 * mt * t * c.x + t * t * p.x,
                            mt * mt * p0.y + 2.0 * mt * t * c.y + t * t * p.y,
                        ));
                    }
                }
                PathElement::CubicTo(a, b, p) => {
                    let p0 = *current.last().unwrap_or(&start);
                    if current.is_empty() {
                        current.push(p0);
                    }
                    let steps =
                        curve_steps(p0.distance(a) + a.distance(b) + b.distance(p), tolerance);
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let mt = 1.0 - t;
                        let (w0, w1, w2, w3) =
                            (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
                        current.push(Point::new(
                            w0 * p0.x + w1 * a.x + w2 * b.x + w3 * p.x,
                            w0 * p0.y + w1 * a.y + w2 * b.y + w3 * p.y,
                        ));
                    }
                }
                PathElement::Close => {
                    let next = current.first().copied().unwrap_or(start);
                    finish(&mut current, &mut out, true);
                    start = next;
                }
            }
        }
        finish(&mut current, &mut out, false);
        out
    }

    /// Point-in-fill test on the flattened path (open subpaths are closed
    /// implicitly, as when filling).
    pub fn contains(&self, point: Point, rule: FillRule) -> bool {
        let mut winding = 0i32;
        for (polyline, _) in self.flatten(0.1) {
            let n = polyline.len();
            for i in 0..n {
                let a = polyline[i];
                let b = polyline[(i + 1) % n];
                if a.y <= point.y {
                    if b.y > point.y && cross(a, b, point) > 0.0 {
                        winding += 1;
                    }
                } else if b.y <= point.y && cross(a, b, point) < 0.0 {
                    winding -= 1;
                }
            }
        }
        match rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    }
}

fn curve_steps(length: f64, tolerance: f64) -> usize {
    ((length / tolerance).sqrt().ceil() as usize).clamp(2, 256)
}

fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Alternating dash and gap lengths; empty for a solid line.
    pub dash: Vec<f64>,
}

impl StrokeStyle {
    pub fn new(width: f64) -> Self {
        Self {
            width,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            dash: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width.is_finite()
            && self.width > 0.0
            && self.dash.iter().all(|d| d.is_finite() && *d >= 0.0)
            && (self.dash.is_empty() || self.dash.iter().sum::<f64>() > 0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient, 0..=1.
    pub offset: f64,
    pub color: Color,
}

/// How a shape is filled or stroked.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    /// Colors vary along `start → end`, extended with the end colors.
    LinearGradient {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    /// Colors vary from `center` (offset 0) to `radius` (offset 1).
    RadialGradient {
        center: Point,
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

impl Paint {
    pub fn is_valid(&self) -> bool {
        let stops_ok = |stops: &[GradientStop]| {
            stops.len() >= 2
                && stops
                    .iter()
                    .all(|stop| stop.offset.is_finite() && (0.0..=1.0).contains(&stop.offset))
                && stops
                    .windows(2)
                    .all(|pair| pair[0].offset <= pair[1].offset)
        };
        match self {
            Self::Solid(_) => true,
            Self::LinearGradient { start, end, stops } => {
                start.is_finite() && end.is_finite() && stops_ok(stops)
            }
            Self::RadialGradient {
                center,
                radius,
                stops,
            } => center.is_finite() && radius.is_finite() && *radius > 0.0 && stops_ok(stops),
        }
    }

    /// Straight-alpha color at `point`, interpolated in sRGB.
    pub fn color_at(&self, point: Point) -> Color {
        let sample = |stops: &[GradientStop], t: f64| {
            let t = t.clamp(0.0, 1.0);
            let first = stops[0];
            if t <= first.offset {
                return first.color;
            }
            for pair in stops.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if t <= b.offset {
                    let span = (b.offset - a.offset).max(1e-12);
                    return lerp_color(a.color, b.color, (t - a.offset) / span);
                }
            }
            stops[stops.len() - 1].color
        };
        match self {
            Self::Solid(color) => *color,
            Self::LinearGradient { start, end, stops } => {
                let (dx, dy) = (end.x - start.x, end.y - start.y);
                let length = dx * dx + dy * dy;
                let t = if length == 0.0 {
                    0.0
                } else {
                    ((point.x - start.x) * dx + (point.y - start.y) * dy) / length
                };
                sample(stops, t)
            }
            Self::RadialGradient {
                center,
                radius,
                stops,
            } => sample(stops, point.distance(*center) / radius),
        }
    }
}

pub fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    let mix = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Color::rgba8(
        mix(a.red, b.red),
        mix(a.green, b.green),
        mix(a.blue, b.blue),
        mix(a.alpha, b.alpha),
    )
}

static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageError {
    InvalidDimensions,
    LengthMismatch,
    /// The bytes are not a supported encoded image.
    UnsupportedFormat,
    DecodeFailed,
    EncodeFailed,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("image dimensions must be positive"),
            Self::LengthMismatch => {
                f.write_str("the pixel buffer length does not match the dimensions")
            }
            Self::UnsupportedFormat => f.write_str("the image format is not supported"),
            Self::DecodeFailed => f.write_str("the image could not be decoded"),
            Self::EncodeFailed => f.write_str("the image could not be encoded"),
        }
    }
}

impl std::error::Error for ImageError {}

/// Decoded straight-alpha sRGB RGBA8 pixels, shared cheaply between display
/// lists. Identity is per decoded image, so clones compare equal and two
/// separately decoded copies do not.
#[derive(Clone, Debug)]
pub struct Image {
    id: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Image {
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions);
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ImageError::InvalidDimensions)?;
        if pixels.len() != expected {
            return Err(ImageError::LengthMismatch);
        }
        Ok(Self {
            id: NEXT_IMAGE.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            pixels: pixels.into(),
        })
    }

    pub fn id(&self) -> u64 {
        self.id
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
        let index = (y as usize * self.width as usize + x as usize) * 4;
        Some(
            self.pixels[index..index + 4]
                .try_into()
                .expect("four channels"),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ImageSampling {
    /// Smooth scaling for photos and icons.
    #[default]
    Linear,
    /// Crisp pixel art and exact-size icons.
    Nearest,
}
