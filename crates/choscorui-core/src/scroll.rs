use crate::layout::{valid_extent, validate_bounds};
use crate::{LayoutError, NodeId, Point, Rect, Size, Tree, TreeError, WheelUnit};

/// A clipped viewport with one direct content child. Scrolling translates the
/// content subtree in the tree's absolute logical coordinate space.
pub struct ScrollViewport {
    viewport: NodeId,
    content: NodeId,
    content_size: Size,
    line_step: Size,
    offset: Point,
}

impl ScrollViewport {
    pub fn new(
        viewport: NodeId,
        content: NodeId,
        content_size: Size,
        line_step: Size,
    ) -> Result<Self, LayoutError> {
        if !valid_extent(content_size.width)
            || !valid_extent(content_size.height)
            || !line_step.width.is_finite()
            || line_step.width <= 0.0
            || !line_step.height.is_finite()
            || line_step.height <= 0.0
        {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            viewport,
            content,
            content_size,
            line_step,
            offset: Point::new(0.0, 0.0),
        })
    }

    pub fn offset(&self) -> Point {
        self.offset
    }

    pub fn arrange(&mut self, tree: &mut Tree, bounds: Rect) -> Result<(), LayoutError> {
        let offset = self.clamp_offset(self.offset, bounds.size);
        self.reposition(tree, bounds, offset, self.content_size)
    }

    pub fn scroll_by(
        &mut self,
        tree: &mut Tree,
        delta: Point,
        unit: WheelUnit,
    ) -> Result<(), LayoutError> {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return Err(LayoutError::InvalidGeometry);
        }
        let bounds = tree.bounds(self.viewport).ok_or(LayoutError::MissingNode)?;
        let displacement = match unit {
            WheelUnit::Points => delta,
            WheelUnit::Lines => Point::new(
                delta.x * self.line_step.width,
                delta.y * self.line_step.height,
            ),
        };
        let requested = Point::new(
            self.offset.x + displacement.x,
            self.offset.y + displacement.y,
        );
        if !requested.x.is_finite() || !requested.y.is_finite() {
            return Err(LayoutError::InvalidGeometry);
        }
        let offset = self.clamp_offset(requested, bounds.size);
        self.reposition(tree, bounds, offset, self.content_size)
    }

    pub fn set_content_size(
        &mut self,
        tree: &mut Tree,
        content_size: Size,
    ) -> Result<(), LayoutError> {
        if !valid_extent(content_size.width) || !valid_extent(content_size.height) {
            return Err(LayoutError::InvalidGeometry);
        }
        let bounds = tree.bounds(self.viewport).ok_or(LayoutError::MissingNode)?;
        let offset = Self::clamp_for(content_size, self.offset, bounds.size);
        self.reposition(tree, bounds, offset, content_size)
    }

    /// Minimally scrolls a rectangle expressed in unscrolled content
    /// coordinates into view. Targets outside content are clamped normally.
    pub fn reveal(&mut self, tree: &mut Tree, target: Rect) -> Result<(), LayoutError> {
        validate_bounds(target)?;
        let bounds = tree.bounds(self.viewport).ok_or(LayoutError::MissingNode)?;
        let requested = Point::new(
            reveal_axis(
                self.offset.x,
                bounds.size.width,
                target.origin.x,
                target.size.width,
            ),
            reveal_axis(
                self.offset.y,
                bounds.size.height,
                target.origin.y,
                target.size.height,
            ),
        );
        let offset = self.clamp_offset(requested, bounds.size);
        self.reposition(tree, bounds, offset, self.content_size)
    }

    fn clamp_offset(&self, requested: Point, viewport_size: Size) -> Point {
        Self::clamp_for(self.content_size, requested, viewport_size)
    }

    fn clamp_for(content_size: Size, requested: Point, viewport_size: Size) -> Point {
        let max_x = (content_size.width - viewport_size.width).max(0.0);
        let max_y = (content_size.height - viewport_size.height).max(0.0);
        Point::new(requested.x.clamp(0.0, max_x), requested.y.clamp(0.0, max_y))
    }

    fn reposition(
        &mut self,
        tree: &mut Tree,
        bounds: Rect,
        offset: Point,
        content_size: Size,
    ) -> Result<(), LayoutError> {
        validate_bounds(bounds)?;
        if !tree.contains(self.viewport) || !tree.contains(self.content) {
            return Err(LayoutError::MissingNode);
        }
        if tree.parent(self.content) != Some(self.viewport) {
            return Err(LayoutError::InvalidPlacement);
        }
        let old = tree.bounds(self.content).ok_or(LayoutError::MissingNode)?;
        let next = Rect::new(
            Point::new(bounds.origin.x - offset.x, bounds.origin.y - offset.y),
            content_size,
        );
        validate_bounds(next)?;
        let delta = Point::new(next.origin.x - old.origin.x, next.origin.y - old.origin.y);
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return Err(LayoutError::InvalidGeometry);
        }
        tree.translate_subtree(self.content, delta)
            .map_err(|error| match error {
                TreeError::MissingNode | TreeError::MissingParent => LayoutError::MissingNode,
                _ => LayoutError::InvalidGeometry,
            })?;
        if old.size != content_size {
            tree.set_bounds(self.content, next)
                .map_err(|_| LayoutError::MissingNode)?;
        }
        tree.set_bounds(self.viewport, bounds)
            .map_err(|_| LayoutError::MissingNode)?;
        self.offset = offset;
        self.content_size = content_size;
        Ok(())
    }
}

fn reveal_axis(offset: f64, viewport: f64, start: f64, extent: f64) -> f64 {
    if extent >= viewport || start < offset {
        start
    } else if start + extent > offset + viewport {
        start + extent - viewport
    } else {
        offset
    }
}
