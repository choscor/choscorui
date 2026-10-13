//! Path coverage: the regions of a winding atlas and the edges that the
//! GPU adds into them.
//!
//! The atlas holds one winding number for each coverage sample. A region
//! covers an integer rectangle of device pixels with `samples × samples`
//! samples in each pixel, at the sample positions of `choscorui-render`.

use choscorui_core::Point;

/// The width and the height of a coverage atlas, in samples.
pub(super) const ATLAS_SIZE: u32 = 2048;

/// The words of one edge in the edge buffer: the line and the quad of
/// samples that it can change, both in atlas samples.
pub(super) const EDGE_WORDS: usize = 8;

/// A rectangle of whole device pixels: left, top, right, bottom.
pub(super) type PixelBox = [i32; 4];

/// Shelf packing of rectangles into a square atlas.
pub(super) struct Shelves {
    size: u32,
    /// The top, the height, and the used width of each shelf.
    shelves: Vec<(u32, u32, u32)>,
    next_top: u32,
}

impl Shelves {
    pub(super) fn new(size: u32) -> Self {
        Self {
            size,
            shelves: Vec::new(),
            next_top: 0,
        }
    }

    /// The top-left corner of a free `width × height` rectangle, or
    /// `None` when the atlas is full.
    pub(super) fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 || width > self.size || height > self.size {
            return None;
        }
        for shelf in &mut self.shelves {
            let (top, shelf_height, used) = *shelf;
            if height <= shelf_height && height * 2 >= shelf_height && used + width <= self.size {
                shelf.2 += width;
                return Some((used, top));
            }
        }
        if self.next_top + height > self.size {
            return None;
        }
        let top = self.next_top;
        self.shelves.push((top, height, width));
        self.next_top += height;
        Some((0, top))
    }

    pub(super) fn clear(&mut self) {
        self.shelves.clear();
        self.next_top = 0;
    }
}

/// The coverage samples per pixel side for a region of `width × height`
/// pixels, so that the region fits in one atlas.
pub(super) fn samples_for(width: u32, height: u32) -> u32 {
    let side = width.max(height);
    if side <= 512 && width * height <= 256 * 512 {
        4
    } else if side <= ATLAS_SIZE / 2 {
        2
    } else {
        1
    }
}

/// The largest side of a region in pixels at `samples` per pixel side.
pub(super) fn max_side(samples: u32) -> u32 {
    ATLAS_SIZE / samples
}

/// A region of an atlas: its pixels, its first sample in the atlas, and
/// its samples per pixel side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Region {
    pub(super) pixels: PixelBox,
    pub(super) offset: (u32, u32),
    pub(super) samples: u32,
}

/// Appends the edges of `polygons`, closed rings in device pixels, that
/// can change the winding numbers of `region` to `out`. A ray from each
/// sample runs to the right, as in `Path::contains`, so edge parts on the
/// left of the region are left out and parts on its right become
/// vertical lines at its right side.
pub(super) fn append_edges(polygons: &[Vec<Point>], region: &Region, out: &mut Vec<f32>) {
    let samples = f64::from(region.samples);
    let [left, top, right, bottom] = region.pixels;
    let (ox, oy) = (f64::from(region.offset.0), f64::from(region.offset.1));
    let x_min = ox;
    let x_max = ox + f64::from(right - left) * samples;
    let y_min = oy;
    let y_max = oy + f64::from(bottom - top) * samples;
    let map = |p: Point| {
        Point::new(
            (p.x - f64::from(left)) * samples + ox,
            (p.y - f64::from(top)) * samples + oy,
        )
    };
    for polygon in polygons {
        let count = polygon.len();
        if count < 2 {
            continue;
        }
        for index in 0..count {
            let a = map(polygon[index]);
            let b = map(polygon[(index + 1) % count]);
            append_edge(a, b, [x_min, y_min, x_max, y_max], out);
        }
    }
}

fn append_edge(a: Point, b: Point, bounds: [f64; 4], out: &mut Vec<f32>) {
    let [x_min, y_min, x_max, y_max] = bounds;
    if a.y == b.y || !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return;
    }
    // Clip to the rows of the region, keeping the direction.
    let at_y = |y: f64| {
        let t = (y - a.y) / (b.y - a.y);
        Point::new(a.x + (b.x - a.x) * t, y)
    };
    let (low, high) = (a.y.min(b.y), a.y.max(b.y));
    if high <= y_min || low >= y_max {
        return;
    }
    let a = if a.y < y_min || a.y > y_max {
        at_y(a.y.clamp(y_min, y_max))
    } else {
        a
    };
    let b = if b.y < y_min || b.y > y_max {
        at_y(b.y.clamp(y_min, y_max))
    } else {
        b
    };
    // Split where the edge crosses the left and the right side.
    let mut cuts = vec![0.0, 1.0];
    for x in [x_min, x_max] {
        if (a.x - x) * (b.x - x) < 0.0 {
            cuts.push((x - a.x) / (b.x - a.x));
        }
    }
    cuts.sort_by(f64::total_cmp);
    let point = |t: f64| Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    for pair in cuts.windows(2) {
        let (start, end) = (point(pair[0]), point(pair[1]));
        if start.y == end.y {
            continue;
        }
        let middle = (start.x + end.x) / 2.0;
        let (start, end) = if middle < x_min {
            continue;
        } else if middle > x_max {
            (Point::new(x_max, start.y), Point::new(x_max, end.y))
        } else {
            (start, end)
        };
        let quad_right = start.x.max(end.x).ceil().min(x_max);
        let quad_top = start.y.min(end.y).floor();
        let quad_bottom = start.y.max(end.y).ceil();
        out.extend([
            start.x as f32,
            start.y as f32,
            end.x as f32,
            end.y as f32,
            x_min as f32,
            quad_top as f32,
            quad_right as f32,
            quad_bottom as f32,
        ]);
    }
}

/// The whole pixels that the points of `polygons` touch, or `None`
/// without points.
pub(super) fn pixel_bounds(polygons: &[Vec<Point>]) -> Option<PixelBox> {
    let mut bounds: Option<[f64; 4]> = None;
    for point in polygons.iter().flatten() {
        let b = bounds.get_or_insert([point.x, point.y, point.x, point.y]);
        b[0] = b[0].min(point.x);
        b[1] = b[1].min(point.y);
        b[2] = b[2].max(point.x);
        b[3] = b[3].max(point.y);
    }
    let [x0, y0, x1, y1] = bounds?;
    let clamp = |value: f64| value.clamp(-1.0e6, 1.0e6) as i32;
    Some([
        clamp(x0.floor()),
        clamp(y0.floor()),
        clamp(x1.ceil()),
        clamp(y1.ceil()),
    ])
}

/// The intersection of two pixel boxes, or `None` when it is empty.
pub(super) fn intersect(a: PixelBox, b: PixelBox) -> Option<PixelBox> {
    let result = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (result[0] < result[2] && result[1] < result[3]).then_some(result)
}

/// `bounds` cut into tiles of at most `side` pixels each way.
pub(super) fn tiles(bounds: PixelBox, side: u32) -> Vec<PixelBox> {
    let side = side.max(1) as i32;
    let mut out = Vec::new();
    let mut top = bounds[1];
    while top < bounds[3] {
        let bottom = (top + side).min(bounds[3]);
        let mut left = bounds[0];
        while left < bounds[2] {
            let right = (left + side).min(bounds[2]);
            out.push([left, top, right, bottom]);
            left = right;
        }
        top = bottom;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The winding number that the edges in `edges` give the sample at
    /// `p`, with the rule of the edge shader.
    fn winding(edges: &[f32], p: Point) -> i32 {
        let mut total = 0;
        for edge in edges.chunks(EDGE_WORDS) {
            let edge: Vec<f64> = edge.iter().map(|value| f64::from(*value)).collect();
            let [ax, ay, bx, by, qx0, qy0, qx1, qy1] = edge[..] else {
                continue;
            };
            if p.x < qx0 || p.x >= qx1 || p.y < qy0 || p.y >= qy1 {
                continue;
            }
            let cross = (bx - ax) * (p.y - ay) - (p.x - ax) * (by - ay);
            if ay <= p.y {
                if by > p.y && cross > 0.0 {
                    total += 1;
                }
            } else if by <= p.y && cross < 0.0 {
                total -= 1;
            }
        }
        total
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }

    #[test]
    fn edges_give_the_winding_of_each_sample() {
        let region = Region {
            pixels: [10, 10, 20, 20],
            offset: (100, 200),
            samples: 2,
        };
        let mut edges = Vec::new();
        // A square that leaves the region on the right and the bottom.
        append_edges(&[square(12.0, 12.0, 40.0, 40.0)], &region, &mut edges);
        let sample = |x: f64, y: f64| {
            let mapped = Point::new((x - 10.0) * 2.0 + 100.0, (y - 10.0) * 2.0 + 200.0);
            winding(&edges, mapped).abs()
        };
        assert_eq!(sample(15.25, 15.25), 1);
        assert_eq!(sample(19.75, 19.75), 1);
        assert_eq!(sample(11.25, 15.25), 0);
        assert_eq!(sample(15.25, 11.75), 0);
    }

    #[test]
    fn shelves_pack_until_full() {
        let mut shelves = Shelves::new(64);
        assert_eq!(shelves.allocate(32, 16), Some((0, 0)));
        assert_eq!(shelves.allocate(32, 16), Some((32, 0)));
        assert_eq!(shelves.allocate(32, 16), Some((0, 16)));
        assert_eq!(shelves.allocate(64, 40), None);
        shelves.clear();
        assert_eq!(shelves.allocate(64, 64), Some((0, 0)));
    }

    #[test]
    fn large_regions_get_fewer_samples_and_tiles() {
        assert_eq!(samples_for(100, 100), 4);
        assert_eq!(samples_for(900, 300), 2);
        assert_eq!(samples_for(3000, 10), 1);
        assert_eq!(tiles([0, 0, 2500, 10], 1024).len(), 3);
        assert_eq!(intersect([0, 0, 4, 4], [4, 0, 8, 4]), None);
    }
}
