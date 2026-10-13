//! Rectangle helpers, 2D affine transforms, and logical/physical scaling.

use crate::{Point, Rect, Size};

impl Point {
    /// The point moved by `dx` and `dy`.
    pub fn offset(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// The straight-line distance to `other`.
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    /// True when both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Size {
    /// A size with no width and no height.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// True when the width or the height is not positive.
    pub fn is_empty(self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    /// Clamps each dimension into `[min, max]`.
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self::new(
            self.width.max(min.width).min(max.width.max(min.width)),
            self.height.max(min.height).min(max.height.max(min.height)),
        )
    }
}

impl Rect {
    /// A rectangle from its left edge, top edge, width, and height.
    pub const fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }

    /// The left edge.
    pub fn min_x(self) -> f64 {
        self.origin.x
    }
    /// The top edge.
    pub fn min_y(self) -> f64 {
        self.origin.y
    }
    /// The right edge.
    pub fn max_x(self) -> f64 {
        self.origin.x + self.size.width
    }
    /// The bottom edge.
    pub fn max_y(self) -> f64 {
        self.origin.y + self.size.height
    }
    /// The center point.
    pub fn center(self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    /// True when the size is empty.
    pub fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// The rectangle moved by `dx` and `dy`.
    pub fn translate(self, dx: f64, dy: f64) -> Self {
        Self::new(self.origin.offset(dx, dy), self.size)
    }

    /// Shrinks by `amount` on every side; never produces negative size.
    pub fn inset(self, amount: f64) -> Self {
        Self::from_xywh(
            self.origin.x + amount,
            self.origin.y + amount,
            (self.size.width - 2.0 * amount).max(0.0),
            (self.size.height - 2.0 * amount).max(0.0),
        )
    }

    /// Overlap of two rectangles, or None when they do not overlap.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x0 = self.min_x().max(other.min_x());
        let y0 = self.min_y().max(other.min_y());
        let x1 = self.max_x().min(other.max_x());
        let y1 = self.max_y().min(other.max_y());
        (x1 > x0 && y1 > y0).then(|| Self::from_xywh(x0, y0, x1 - x0, y1 - y0))
    }

    /// True when the two rectangles overlap.
    pub fn intersects(self, other: Self) -> bool {
        self.intersection(other).is_some()
    }

    /// Smallest rectangle containing both. Empty rectangles are ignored.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.min_x().min(other.min_x());
        let y0 = self.min_y().min(other.min_y());
        let x1 = self.max_x().max(other.max_x());
        let y1 = self.max_y().max(other.max_y());
        Self::from_xywh(x0, y0, x1 - x0, y1 - y0)
    }

    /// True when `other` is fully inside this rectangle.
    pub fn contains_rect(self, other: Self) -> bool {
        other.min_x() >= self.min_x()
            && other.min_y() >= self.min_y()
            && other.max_x() <= self.max_x()
            && other.max_y() <= self.max_y()
    }

    /// True when the origin and the size are finite.
    pub fn is_finite(self) -> bool {
        self.origin.is_finite() && self.size.width.is_finite() && self.size.height.is_finite()
    }

    /// Mirrors horizontally inside `container`, for right-to-left layout.
    pub fn mirrored_in(self, container: Self) -> Self {
        let x = container.min_x() + (container.max_x() - self.max_x());
        Self::from_xywh(x, self.origin.y, self.size.width, self.size.height)
    }
}

/// Row-major 2D affine transform mapping `(x, y)` to
/// `(a*x + c*y + tx, b*x + d*y + ty)`, matching Core Graphics conventions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub const fn translate(tx: f64, ty: f64) -> Self {
        Self {
            tx,
            ty,
            ..Self::IDENTITY
        }
    }

    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Counter-clockwise rotation in radians in a y-up space; in the
    /// toolkit's y-down logical space positive angles rotate clockwise.
    pub fn rotate(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Applies `self` first, then `next`.
    pub fn then(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            tx: self.tx * next.a + self.ty * next.c + next.tx,
            ty: self.tx * next.b + self.ty * next.d + next.ty,
        }
    }

    pub fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn is_finite(self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|value| value.is_finite())
    }

    /// Inverse transform, or None for a singular or non-finite matrix.
    pub fn invert(self) -> Option<Self> {
        let det = self.determinant();
        if !self.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            tx: (self.c * self.ty - self.d * self.tx) / det,
            ty: (self.b * self.tx - self.a * self.ty) / det,
        })
    }

    pub fn apply(self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.tx,
            self.b * point.x + self.d * point.y + self.ty,
        )
    }

    /// Axis-aligned bounds of the transformed rectangle.
    pub fn apply_rect(self, rect: Rect) -> Rect {
        let corners = [
            self.apply(rect.origin),
            self.apply(Point::new(rect.max_x(), rect.min_y())),
            self.apply(Point::new(rect.min_x(), rect.max_y())),
            self.apply(Point::new(rect.max_x(), rect.max_y())),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for corner in corners {
            x0 = x0.min(corner.x);
            y0 = y0.min(corner.y);
            x1 = x1.max(corner.x);
            y1 = y1.max(corner.y);
        }
        Rect::from_xywh(x0, y0, x1 - x0, y1 - y0)
    }
}

/// A pixel rectangle on a backing surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelRect {
    pub x: i64,
    pub y: i64,
    pub width: u64,
    pub height: u64,
}

/// Converts between logical points and device pixels for one surface. Only
/// backends and painters use it; layout and hit testing stay logical.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub fn new(scale: f64) -> Option<Self> {
        (scale.is_finite() && scale > 0.0).then_some(Self(scale))
    }

    pub fn get(self) -> f64 {
        self.0
    }

    pub fn to_physical(self, point: Point) -> Point {
        Point::new(point.x * self.0, point.y * self.0)
    }

    pub fn to_logical(self, point: Point) -> Point {
        Point::new(point.x / self.0, point.y / self.0)
    }

    /// Smallest pixel rectangle that covers `rect` (used for damage).
    pub fn cover_pixels(self, rect: Rect) -> PixelRect {
        let x0 = (rect.min_x() * self.0).floor();
        let y0 = (rect.min_y() * self.0).floor();
        let x1 = (rect.max_x() * self.0).ceil();
        let y1 = (rect.max_y() * self.0).ceil();
        PixelRect {
            x: x0 as i64,
            y: y0 as i64,
            width: (x1 - x0).max(0.0) as u64,
            height: (y1 - y0).max(0.0) as u64,
        }
    }

    /// Snaps a logical coordinate to the nearest device pixel boundary,
    /// so one-pixel strokes stay crisp at any scale.
    pub fn snap(self, value: f64) -> f64 {
        (value * self.0).round() / self.0
    }
}
