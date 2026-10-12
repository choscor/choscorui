//! The visible window of a lazy stack: items of one fixed extent along an
//! axis, with a fixed gap between them. The window costs the same for ten
//! items and for ten million.

use crate::LayoutError;
use crate::layout::valid_extent;
use std::ops::Range;

/// `count` items of `extent` points each along one axis, with `spacing`
/// points between neighbours. Item 0 starts at 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniformStack {
    count: usize,
    extent: f64,
    spacing: f64,
}

impl UniformStack {
    /// The stack, or `InvalidGeometry` when `extent` is not finite and
    /// positive or `spacing` is not finite and non-negative.
    pub fn new(count: usize, extent: f64, spacing: f64) -> Result<Self, LayoutError> {
        if !extent.is_finite() || extent <= 0.0 || !valid_extent(spacing) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            count,
            extent,
            spacing,
        })
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn extent(&self) -> f64 {
        self.extent
    }

    pub fn spacing(&self) -> f64 {
        self.spacing
    }

    /// The distance from the start of one item to the start of the next.
    fn pitch(&self) -> f64 {
        self.extent + self.spacing
    }

    /// The length of all items and the gaps between them.
    pub fn length(&self) -> f64 {
        match self.count {
            0 => 0.0,
            count => count as f64 * self.extent + (count - 1) as f64 * self.spacing,
        }
    }

    /// Where item `index` starts.
    pub fn offset(&self, index: usize) -> f64 {
        index as f64 * self.pitch()
    }

    /// The items that overlap `start..end`, in points from the start of
    /// item 0. Items that only touch the range and gaps do not count. The
    /// range is empty when nothing overlaps.
    pub fn window(&self, start: f64, end: f64) -> Range<usize> {
        let start = start.max(0.0);
        let end = end.min(self.length());
        if start >= end {
            let at = self.first_at(start.min(self.length()));
            return at..at;
        }
        let first = self.first_at(start);
        // An item overlaps when it starts before `end`.
        let last = (end / self.pitch()).ceil();
        let last = if last.is_finite() { last as usize } else { 0 };
        first..last.clamp(first, self.count)
    }

    /// The first item that ends after `position`.
    fn first_at(&self, position: f64) -> usize {
        let pitch = self.pitch();
        let index = (position / pitch).floor();
        let index = if index.is_finite() { index as usize } else { 0 };
        let index = if position - index as f64 * pitch >= self.extent {
            index + 1
        } else {
            index
        };
        index.min(self.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: usize) -> UniformStack {
        UniformStack::new(count, 20.0, 4.0).unwrap()
    }

    #[test]
    fn invalid_geometry_is_an_error() {
        for (extent, spacing) in [(0.0, 0.0), (-1.0, 0.0), (f64::NAN, 0.0), (10.0, -1.0)] {
            assert_eq!(
                UniformStack::new(3, extent, spacing),
                Err(LayoutError::InvalidGeometry)
            );
        }
        assert_eq!(
            UniformStack::new(3, 10.0, f64::INFINITY),
            Err(LayoutError::InvalidGeometry)
        );
    }

    #[test]
    fn length_and_offsets_count_the_gaps_between_items() {
        assert_eq!(rows(0).length(), 0.0);
        assert_eq!(rows(1).length(), 20.0);
        assert_eq!(rows(3).length(), 68.0);
        assert_eq!(rows(3).offset(2), 48.0);
    }

    #[test]
    fn the_window_holds_the_items_that_overlap_the_range() {
        let stack = rows(100);
        assert_eq!(stack.window(0.0, 50.0), 0..3);
        // The range starts in the gap after item 0 and ends where item 3
        // starts, so items 0 and 3 do not overlap it.
        assert_eq!(stack.window(21.0, 72.0), 1..3);
        assert_eq!(stack.window(24.0, 72.5), 1..4);
        assert_eq!(stack.window(10.0, 10.0), 0..0);
    }

    #[test]
    fn the_window_clamps_to_the_items() {
        let stack = rows(10);
        assert_eq!(stack.window(-100.0, 30.0), 0..2);
        assert_eq!(stack.window(200.0, 1_000.0), 8..10);
        assert_eq!(stack.window(1_000.0, 2_000.0), 10..10);
        assert_eq!(rows(0).window(0.0, 100.0), 0..0);
    }

    #[test]
    fn the_window_costs_the_same_for_any_count() {
        let stack = rows(10_000_000);
        let start = stack.offset(9_000_000);
        assert_eq!(stack.window(start, start + 48.0), 9_000_000..9_000_002);
    }
}
