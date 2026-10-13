//! Stroke geometry that every painter shares: dashes, and the outline of
//! a stroke as polygons that a painter fills.

use crate::{LineCap, LineJoin, Point, StrokeStyle};

/// The miter limit of every painter: a miter join longer than this many
/// half widths becomes a bevel join.
pub const MITER_LIMIT: f64 = 10.0;

/// Splits flattened subpaths into dashes of the alternating dash and gap
/// lengths in `pattern`. A closed subpath becomes open dash pieces. An
/// empty pattern, or one without length, returns the subpaths unchanged.
pub fn dash_polylines(polys: Vec<(Vec<Point>, bool)>, pattern: &[f64]) -> Vec<(Vec<Point>, bool)> {
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

/// The outline of a stroke of `style` along flattened, already dashed
/// subpaths, as polygons with a positive area in a y-down space. Their
/// union under the non-zero rule is the stroke: one quadrilateral for
/// each segment, one polygon for each join, and one for each round or
/// square cap. Arcs deviate from the circle by at most `tolerance`.
pub fn stroke_polygons(
    polys: &[(Vec<Point>, bool)],
    style: &StrokeStyle,
    tolerance: f64,
) -> Vec<Vec<Point>> {
    let half = style.width / 2.0;
    let mut out = Vec::new();
    if !(half > 0.0 && half.is_finite()) {
        return out;
    }
    let tolerance = tolerance.max(1e-3);
    for (poly, closed) in polys {
        let points = without_repeats(poly, *closed);
        if points.len() < 2 {
            if style.cap == LineCap::Round
                && let Some(point) = points.first()
            {
                out.push(disc(*point, half, tolerance));
            }
            continue;
        }
        let n = points.len();
        let segments = if *closed { n } else { n - 1 };
        for index in 0..segments {
            let a = points[index];
            let b = points[(index + 1) % n];
            let Some(d) = direction(a, b) else {
                continue;
            };
            let square = !closed && style.cap == LineCap::Square;
            let start = if square && index == 0 {
                a.offset(-d.x * half, -d.y * half)
            } else {
                a
            };
            let end = if square && index == segments - 1 {
                b.offset(d.x * half, d.y * half)
            } else {
                b
            };
            let normal = Point::new(-d.y * half, d.x * half);
            out.push(positive(vec![
                start.offset(normal.x, normal.y),
                end.offset(normal.x, normal.y),
                end.offset(-normal.x, -normal.y),
                start.offset(-normal.x, -normal.y),
            ]));
        }
        if !closed && style.cap == LineCap::Round {
            out.push(disc(points[0], half, tolerance));
            out.push(disc(points[n - 1], half, tolerance));
        }
        let joints = if *closed { 0..n } else { 1..n - 1 };
        for index in joints {
            let previous = points[(index + n - 1) % n];
            let vertex = points[index];
            let next = points[(index + 1) % n];
            if let Some(join) = join(previous, vertex, next, half, style.join, tolerance) {
                out.push(join);
            }
        }
    }
    out
}

/// The points of a subpath without consecutive repeats, and for a closed
/// subpath without a last point equal to the first.
fn without_repeats(poly: &[Point], closed: bool) -> Vec<Point> {
    let mut points: Vec<Point> = Vec::with_capacity(poly.len());
    for point in poly {
        if points.last() != Some(point) {
            points.push(*point);
        }
    }
    if closed && points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

fn direction(a: Point, b: Point) -> Option<Point> {
    let length = a.distance(b);
    (length > 0.0 && length.is_finite())
        .then(|| Point::new((b.x - a.x) / length, (b.y - a.y) / length))
}

/// The join polygon at `vertex`, or `None` where the subpath goes straight
/// on.
fn join(
    previous: Point,
    vertex: Point,
    next: Point,
    half: f64,
    style: LineJoin,
    tolerance: f64,
) -> Option<Vec<Point>> {
    let d1 = direction(previous, vertex)?;
    let d2 = direction(vertex, next)?;
    let turn = d1.x * d2.y - d1.y * d2.x;
    if turn.abs() < 1e-9 && d1.x * d2.x + d1.y * d2.y > 0.0 {
        return None;
    }
    if style == LineJoin::Round {
        return Some(disc(vertex, half, tolerance));
    }
    // The normals on the outer side of the turn.
    let side = if turn > 0.0 { -1.0 } else { 1.0 };
    let n1 = Point::new(-d1.y * side, d1.x * side);
    let n2 = Point::new(-d2.y * side, d2.x * side);
    let outer1 = vertex.offset(n1.x * half, n1.y * half);
    let outer2 = vertex.offset(n2.x * half, n2.y * half);
    let bisector = Point::new(n1.x + n2.x, n1.y + n2.y);
    let length = (bisector.x * bisector.x + bisector.y * bisector.y).sqrt();
    if style == LineJoin::Miter && length > 0.0 {
        let cos_half = length / 2.0;
        if cos_half > 1.0 / MITER_LIMIT {
            let reach = half / cos_half / length;
            let tip = vertex.offset(bisector.x * reach, bisector.y * reach);
            return Some(positive(vec![vertex, outer1, tip, outer2]));
        }
    }
    Some(positive(vec![vertex, outer1, outer2]))
}

/// A polygon that stays within `tolerance` of the circle around `center`.
fn disc(center: Point, radius: f64, tolerance: f64) -> Vec<Point> {
    let ratio = (1.0 - tolerance / radius).clamp(-1.0, 1.0);
    let step = ratio.acos().max(1e-3);
    let count = ((std::f64::consts::PI / step).ceil() as usize).clamp(8, 256);
    (0..count)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / count as f64;
            center.offset(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

/// `polygon` with its points in the order that gives a positive area.
fn positive(mut polygon: Vec<Point>) -> Vec<Point> {
    let area: f64 = (0..polygon.len())
        .map(|index| {
            let a = polygon[index];
            let b = polygon[(index + 1) % polygon.len()];
            a.x * b.y - b.x * a.y
        })
        .sum();
    if area < 0.0 {
        polygon.reverse();
    }
    polygon
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The non-zero winding number of `point` in the union of `polygons`.
    fn winding(polygons: &[Vec<Point>], point: Point) -> i32 {
        let mut winding = 0;
        for polygon in polygons {
            for index in 0..polygon.len() {
                let a = polygon[index];
                let b = polygon[(index + 1) % polygon.len()];
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
        winding
    }

    fn line(points: &[(f64, f64)], closed: bool) -> Vec<(Vec<Point>, bool)> {
        vec![(
            points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            closed,
        )]
    }

    #[test]
    fn a_dash_pattern_splits_a_line() {
        let dashed = dash_polylines(line(&[(0.0, 0.0), (10.0, 0.0)], false), &[2.0, 3.0]);
        let starts: Vec<f64> = dashed.iter().map(|(piece, _)| piece[0].x).collect();
        assert_eq!(starts, vec![0.0, 5.0]);
        assert!(dashed.iter().all(|(_, closed)| !closed));
        let solid = dash_polylines(line(&[(0.0, 0.0), (10.0, 0.0)], false), &[]);
        assert_eq!(solid.len(), 1);
    }

    #[test]
    fn overlapping_pieces_never_cancel() {
        let mut style = StrokeStyle::new(4.0);
        style.join = LineJoin::Round;
        style.cap = LineCap::Round;
        let polygons = stroke_polygons(
            &line(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)], false),
            &style,
            0.05,
        );
        // Inside the corner, the two segments and the join overlap.
        assert!(winding(&polygons, Point::new(20.5, 0.5)) > 0);
        assert!(winding(&polygons, Point::new(-1.5, 0.0)) > 0, "round cap");
        assert_eq!(winding(&polygons, Point::new(10.0, 3.0)), 0);
    }

    #[test]
    fn joins_follow_the_style() {
        let corner = line(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)], false);
        let mut style = StrokeStyle::new(4.0);
        let covers_tip = |style: &StrokeStyle| {
            winding(
                &stroke_polygons(&corner, style, 0.05),
                Point::new(21.8, -1.8),
            ) > 0
        };
        assert!(covers_tip(&style), "miter");
        style.join = LineJoin::Bevel;
        assert!(!covers_tip(&style), "bevel");
        style.join = LineJoin::Round;
        assert!(!covers_tip(&style), "round");
        // A sharp turn exceeds the miter limit and becomes a bevel.
        let sharp = line(&[(0.0, 0.0), (20.0, 0.0), (0.0, 0.5)], false);
        let polygons = stroke_polygons(&sharp, &StrokeStyle::new(4.0), 0.05);
        assert_eq!(winding(&polygons, Point::new(40.0, 0.0)), 0);
    }

    #[test]
    fn square_caps_extend_open_ends() {
        let mut style = StrokeStyle::new(4.0);
        style.cap = LineCap::Square;
        let open = line(&[(0.0, 0.0), (10.0, 0.0)], false);
        let polygons = stroke_polygons(&open, &style, 0.05);
        assert!(winding(&polygons, Point::new(-1.5, 1.5)) > 0);
        assert!(winding(&polygons, Point::new(11.5, -1.5)) > 0);
        style.cap = LineCap::Butt;
        let polygons = stroke_polygons(&open, &style, 0.05);
        assert_eq!(winding(&polygons, Point::new(-1.5, 0.0)), 0);
    }

    #[test]
    fn closed_subpaths_join_at_the_start() {
        let square = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true);
        let polygons = stroke_polygons(&square, &StrokeStyle::new(2.0), 0.05);
        assert!(
            winding(&polygons, Point::new(-0.9, -0.9)) > 0,
            "miter at start"
        );
        assert_eq!(winding(&polygons, Point::new(5.0, 5.0)), 0);
    }
}
