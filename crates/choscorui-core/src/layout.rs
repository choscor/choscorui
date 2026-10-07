use crate::{NodeId, Point, Rect, Size, Tree};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutDirection {
    Ltr,
    Rtl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossAlign {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Insets {
    pub const fn all(value: f64) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    fn valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    InvalidGeometry,
    InsufficientSpace,
    MissingNode,
    DuplicateNode,
    InvalidPlacement,
}

#[derive(Clone, Copy)]
enum MainSize {
    Fixed(f64),
    Flex { weight: f64, min: f64, max: f64 },
}

#[derive(Clone, Copy)]
struct Child {
    node: NodeId,
    main: MainSize,
    cross: f64,
}

/// Experimental linear measure/arrange primitive in logical points. Fixed
/// children keep their specified main size. Flex children receive remaining
/// space by weight, capped at max. Insufficient minimum space returns an error
/// before mutating the retained tree.
pub struct LinearLayout {
    axis: Axis,
    direction: LayoutDirection,
    padding: Insets,
    spacing: f64,
    align: CrossAlign,
    children: Vec<Child>,
    identities: HashSet<NodeId>,
}

impl LinearLayout {
    pub fn new(
        axis: Axis,
        direction: LayoutDirection,
        padding: Insets,
        spacing: f64,
        align: CrossAlign,
    ) -> Result<Self, LayoutError> {
        if !padding.valid() || !spacing.is_finite() || spacing < 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            axis,
            direction,
            padding,
            spacing,
            align,
            children: Vec::new(),
            identities: HashSet::new(),
        })
    }

    pub fn add_fixed(&mut self, node: NodeId, main: f64, cross: f64) -> Result<(), LayoutError> {
        if !valid_extent(main) || !valid_extent(cross) {
            return Err(LayoutError::InvalidGeometry);
        }
        self.add(Child {
            node,
            main: MainSize::Fixed(main),
            cross,
        })
    }

    pub fn add_flex(
        &mut self,
        node: NodeId,
        weight: f64,
        min: f64,
        max: f64,
        cross: f64,
    ) -> Result<(), LayoutError> {
        if !weight.is_finite()
            || weight <= 0.0
            || !valid_extent(min)
            || !(valid_extent(max) || max == f64::INFINITY)
            || max < min
            || !valid_extent(cross)
        {
            return Err(LayoutError::InvalidGeometry);
        }
        self.add(Child {
            node,
            main: MainSize::Flex { weight, min, max },
            cross,
        })
    }

    fn add(&mut self, child: Child) -> Result<(), LayoutError> {
        if !self.identities.insert(child.node) {
            return Err(LayoutError::DuplicateNode);
        }
        self.children.push(child);
        Ok(())
    }

    /// Returns the minimum intrinsic size needed by all children and spacing.
    pub fn measure(&self) -> Size {
        let main = self
            .children
            .iter()
            .map(|child| match child.main {
                MainSize::Fixed(value) => value,
                MainSize::Flex { min, .. } => min,
            })
            .sum::<f64>()
            + self.spacing * self.children.len().saturating_sub(1) as f64;
        let cross = self
            .children
            .iter()
            .map(|child| child.cross)
            .fold(0.0, f64::max);
        match self.axis {
            Axis::Horizontal => Size::new(
                main + self.padding.left + self.padding.right,
                cross + self.padding.top + self.padding.bottom,
            ),
            Axis::Vertical => Size::new(
                cross + self.padding.left + self.padding.right,
                main + self.padding.top + self.padding.bottom,
            ),
        }
    }

    pub fn arrange(&self, bounds: Rect) -> Result<Vec<(NodeId, Rect)>, LayoutError> {
        if !valid_extent(bounds.size.width)
            || !valid_extent(bounds.size.height)
            || !bounds.origin.x.is_finite()
            || !bounds.origin.y.is_finite()
            || !(bounds.origin.x + bounds.size.width).is_finite()
            || !(bounds.origin.y + bounds.size.height).is_finite()
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let (
            main_size,
            cross_size,
            start_main,
            start_cross,
            pad_start,
            pad_end,
            cross_start_pad,
            cross_end_pad,
        ) = match self.axis {
            Axis::Horizontal => (
                bounds.size.width,
                bounds.size.height,
                bounds.origin.x,
                bounds.origin.y,
                self.padding.left,
                self.padding.right,
                self.padding.top,
                self.padding.bottom,
            ),
            Axis::Vertical => (
                bounds.size.height,
                bounds.size.width,
                bounds.origin.y,
                bounds.origin.x,
                self.padding.top,
                self.padding.bottom,
                self.padding.left,
                self.padding.right,
            ),
        };
        let gaps = self.spacing * self.children.len().saturating_sub(1) as f64;
        let available_main = main_size - pad_start - pad_end - gaps;
        let available_cross = cross_size - cross_start_pad - cross_end_pad;
        let minimum_main = self
            .children
            .iter()
            .map(|child| match child.main {
                MainSize::Fixed(value) => value,
                MainSize::Flex { min, .. } => min,
            })
            .sum::<f64>();
        if !available_main.is_finite()
            || !available_cross.is_finite()
            || available_main < minimum_main
            || available_cross < 0.0
        {
            return Err(LayoutError::InsufficientSpace);
        }
        if self.align != CrossAlign::Stretch
            && self
                .children
                .iter()
                .any(|child| child.cross > available_cross)
        {
            return Err(LayoutError::InsufficientSpace);
        }

        let tracks: Vec<MainSize> = self.children.iter().map(|child| child.main).collect();
        let lengths = distribute_tracks(&tracks, available_main)?;

        let reverse = self.axis == Axis::Horizontal && self.direction == LayoutDirection::Rtl;
        let mut cursor = if reverse {
            start_main + main_size - pad_end
        } else {
            start_main + pad_start
        };
        let mut result = Vec::with_capacity(self.children.len());
        for (index, child) in self.children.iter().enumerate() {
            let main = lengths[index];
            let main_origin = if reverse { cursor - main } else { cursor };
            let cross = if self.align == CrossAlign::Stretch {
                available_cross
            } else {
                child.cross
            };
            let cross_offset = match self.align {
                CrossAlign::Start => {
                    if self.axis == Axis::Vertical && self.direction == LayoutDirection::Rtl {
                        available_cross - cross
                    } else {
                        0.0
                    }
                }
                CrossAlign::Center => (available_cross - cross) / 2.0,
                CrossAlign::End => {
                    if self.axis == Axis::Vertical && self.direction == LayoutDirection::Rtl {
                        0.0
                    } else {
                        available_cross - cross
                    }
                }
                CrossAlign::Stretch => 0.0,
            };
            let cross_origin = start_cross + cross_start_pad + cross_offset;
            let rect = match self.axis {
                Axis::Horizontal => Rect::new(
                    Point::new(main_origin, cross_origin),
                    Size::new(main, cross),
                ),
                Axis::Vertical => Rect::new(
                    Point::new(cross_origin, main_origin),
                    Size::new(cross, main),
                ),
            };
            if !rect.origin.x.is_finite()
                || !rect.origin.y.is_finite()
                || !valid_extent(rect.size.width)
                || !valid_extent(rect.size.height)
            {
                return Err(LayoutError::InvalidGeometry);
            }
            result.push((child.node, rect));
            if reverse {
                cursor -= main + self.spacing;
            } else {
                cursor += main + self.spacing;
            }
        }
        Ok(result)
    }

    pub fn apply(&self, tree: &mut Tree, bounds: Rect) -> Result<(), LayoutError> {
        let arranged = self.arrange(bounds)?;
        if arranged.iter().any(|(node, _)| !tree.contains(*node)) {
            return Err(LayoutError::MissingNode);
        }
        for (node, rect) in arranged {
            tree.set_bounds(node, rect)
                .map_err(|_| LayoutError::MissingNode)?;
        }
        Ok(())
    }
}

pub(crate) fn valid_extent(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

#[derive(Clone, Copy)]
struct GridItem {
    node: NodeId,
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
}

/// Experimental grid primitive with fixed/flexible tracks and spanning cells.
/// Tracks have explicit minimums; intrinsic content-driven sizing and form
/// label alignment are separate follow-up contracts.
pub struct GridLayout {
    direction: LayoutDirection,
    padding: Insets,
    row_gap: f64,
    column_gap: f64,
    rows: Vec<MainSize>,
    columns: Vec<MainSize>,
    items: Vec<GridItem>,
    identities: HashSet<NodeId>,
    occupied: HashSet<(usize, usize)>,
}

impl GridLayout {
    pub fn new(
        direction: LayoutDirection,
        padding: Insets,
        row_gap: f64,
        column_gap: f64,
    ) -> Result<Self, LayoutError> {
        if !padding.valid() || !valid_extent(row_gap) || !valid_extent(column_gap) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            direction,
            padding,
            row_gap,
            column_gap,
            rows: Vec::new(),
            columns: Vec::new(),
            items: Vec::new(),
            identities: HashSet::new(),
            occupied: HashSet::new(),
        })
    }

    pub fn add_column_fixed(&mut self, size: f64) -> Result<(), LayoutError> {
        self.columns.push(fixed_track(size)?);
        Ok(())
    }

    pub fn add_column_flex(&mut self, weight: f64, min: f64, max: f64) -> Result<(), LayoutError> {
        self.columns.push(flex_track(weight, min, max)?);
        Ok(())
    }

    pub fn add_row_fixed(&mut self, size: f64) -> Result<(), LayoutError> {
        self.rows.push(fixed_track(size)?);
        Ok(())
    }

    pub fn add_row_flex(&mut self, weight: f64, min: f64, max: f64) -> Result<(), LayoutError> {
        self.rows.push(flex_track(weight, min, max)?);
        Ok(())
    }

    pub fn place(
        &mut self,
        node: NodeId,
        row: usize,
        column: usize,
        row_span: usize,
        column_span: usize,
    ) -> Result<(), LayoutError> {
        let row_end = row
            .checked_add(row_span)
            .ok_or(LayoutError::InvalidPlacement)?;
        let column_end = column
            .checked_add(column_span)
            .ok_or(LayoutError::InvalidPlacement)?;
        if row_span == 0
            || column_span == 0
            || row_end > self.rows.len()
            || column_end > self.columns.len()
        {
            return Err(LayoutError::InvalidPlacement);
        }
        if self.identities.contains(&node) {
            return Err(LayoutError::DuplicateNode);
        }
        if (row..row_end).any(|r| (column..column_end).any(|c| self.occupied.contains(&(r, c)))) {
            return Err(LayoutError::InvalidPlacement);
        }
        self.identities.insert(node);
        for r in row..row_end {
            for c in column..column_end {
                self.occupied.insert((r, c));
            }
        }
        self.items.push(GridItem {
            node,
            row,
            column,
            row_span,
            column_span,
        });
        Ok(())
    }

    /// Minimum size from explicit track minimums, gaps, and padding.
    pub fn measure(&self) -> Size {
        Size::new(
            self.columns
                .iter()
                .map(|track| track_min(*track))
                .sum::<f64>()
                + self.column_gap * self.columns.len().saturating_sub(1) as f64
                + self.padding.left
                + self.padding.right,
            self.rows.iter().map(|track| track_min(*track)).sum::<f64>()
                + self.row_gap * self.rows.len().saturating_sub(1) as f64
                + self.padding.top
                + self.padding.bottom,
        )
    }

    pub fn arrange(&self, bounds: Rect) -> Result<Vec<(NodeId, Rect)>, LayoutError> {
        validate_bounds(bounds)?;
        let column_space = bounds.size.width
            - self.padding.left
            - self.padding.right
            - self.column_gap * self.columns.len().saturating_sub(1) as f64;
        let row_space = bounds.size.height
            - self.padding.top
            - self.padding.bottom
            - self.row_gap * self.rows.len().saturating_sub(1) as f64;
        let widths = distribute_tracks(&self.columns, column_space)?;
        let heights = distribute_tracks(&self.rows, row_space)?;
        let mut column_origins = Vec::with_capacity(widths.len());
        let mut column_cursor = if self.direction == LayoutDirection::Rtl {
            bounds.origin.x + bounds.size.width - self.padding.right
        } else {
            bounds.origin.x + self.padding.left
        };
        for width in &widths {
            if self.direction == LayoutDirection::Rtl {
                column_origins.push(column_cursor - width);
                column_cursor -= width + self.column_gap;
            } else {
                column_origins.push(column_cursor);
                column_cursor += width + self.column_gap;
            }
        }
        let mut row_origins = Vec::with_capacity(heights.len());
        let mut row_cursor = bounds.origin.y + self.padding.top;
        for height in &heights {
            row_origins.push(row_cursor);
            row_cursor += height + self.row_gap;
        }
        let mut result = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let x = if self.direction == LayoutDirection::Rtl {
                column_origins[item.column + item.column_span - 1]
            } else {
                column_origins[item.column]
            };
            let y = row_origins[item.row];
            let width = widths[item.column..item.column + item.column_span]
                .iter()
                .sum::<f64>()
                + self.column_gap * (item.column_span - 1) as f64;
            let height = heights[item.row..item.row + item.row_span]
                .iter()
                .sum::<f64>()
                + self.row_gap * (item.row_span - 1) as f64;
            let rect = Rect::new(Point::new(x, y), Size::new(width, height));
            validate_bounds(rect)?;
            result.push((item.node, rect));
        }
        Ok(result)
    }

    pub fn apply(&self, tree: &mut Tree, bounds: Rect) -> Result<(), LayoutError> {
        let arranged = self.arrange(bounds)?;
        if arranged.iter().any(|(node, _)| !tree.contains(*node)) {
            return Err(LayoutError::MissingNode);
        }
        for (node, rect) in arranged {
            tree.set_bounds(node, rect)
                .map_err(|_| LayoutError::MissingNode)?;
        }
        Ok(())
    }
}

fn fixed_track(size: f64) -> Result<MainSize, LayoutError> {
    if valid_extent(size) {
        Ok(MainSize::Fixed(size))
    } else {
        Err(LayoutError::InvalidGeometry)
    }
}

fn flex_track(weight: f64, min: f64, max: f64) -> Result<MainSize, LayoutError> {
    if !weight.is_finite()
        || weight <= 0.0
        || !valid_extent(min)
        || !(valid_extent(max) || max == f64::INFINITY)
        || max < min
    {
        Err(LayoutError::InvalidGeometry)
    } else {
        Ok(MainSize::Flex { weight, min, max })
    }
}

fn track_min(track: MainSize) -> f64 {
    match track {
        MainSize::Fixed(value) => value,
        MainSize::Flex { min, .. } => min,
    }
}

pub(crate) fn validate_bounds(bounds: Rect) -> Result<(), LayoutError> {
    if valid_extent(bounds.size.width)
        && valid_extent(bounds.size.height)
        && bounds.origin.x.is_finite()
        && bounds.origin.y.is_finite()
        && (bounds.origin.x + bounds.size.width).is_finite()
        && (bounds.origin.y + bounds.size.height).is_finite()
    {
        Ok(())
    } else {
        Err(LayoutError::InvalidGeometry)
    }
}

fn distribute_tracks(tracks: &[MainSize], available: f64) -> Result<Vec<f64>, LayoutError> {
    let minimum: f64 = tracks.iter().map(|track| track_min(*track)).sum();
    if !available.is_finite() || available < minimum {
        return Err(LayoutError::InsufficientSpace);
    }
    let mut lengths: Vec<f64> = tracks.iter().map(|track| track_min(*track)).collect();
    let mut remaining = available - minimum;
    let mut active: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(index, track)| match track {
            MainSize::Flex { min, max, .. } if max > min => Some(index),
            _ => None,
        })
        .collect();
    while remaining > 0.0 && !active.is_empty() {
        let weight_scale = active
            .iter()
            .map(|index| match tracks[*index] {
                MainSize::Flex { weight, .. } => weight,
                MainSize::Fixed(_) => unreachable!(),
            })
            .fold(0.0, f64::max);
        let total_weight: f64 = active
            .iter()
            .map(|index| match tracks[*index] {
                MainSize::Flex { weight, .. } => weight / weight_scale,
                MainSize::Fixed(_) => unreachable!(),
            })
            .sum();
        let mut consumed = 0.0;
        for index in &active {
            if let MainSize::Flex { weight, max, .. } = tracks[*index] {
                let share = remaining * (weight / weight_scale) / total_weight;
                let added = share.min(max - lengths[*index]);
                lengths[*index] += added;
                consumed += added;
            }
        }
        if consumed <= 0.0 {
            break;
        }
        remaining -= consumed;
        active.retain(|index| match tracks[*index] {
            MainSize::Flex { max, .. } => max - lengths[*index] > 0.0,
            MainSize::Fixed(_) => false,
        });
    }
    Ok(lengths)
}

#[derive(Clone, Copy)]
struct FormRow {
    label: NodeId,
    field: NodeId,
    label_width: f64,
    field_min_width: f64,
    height: f64,
}

/// A two-column form with labels aligned against the field column. Label
/// widths and row heights come from the caller's text/control measurement.
pub struct FormLayout {
    direction: LayoutDirection,
    padding: Insets,
    row_gap: f64,
    column_gap: f64,
    rows: Vec<FormRow>,
    identities: HashSet<NodeId>,
}

impl FormLayout {
    pub fn new(
        direction: LayoutDirection,
        padding: Insets,
        row_gap: f64,
        column_gap: f64,
    ) -> Result<Self, LayoutError> {
        if !padding.valid() || !valid_extent(row_gap) || !valid_extent(column_gap) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            direction,
            padding,
            row_gap,
            column_gap,
            rows: Vec::new(),
            identities: HashSet::new(),
        })
    }

    pub fn add_row(
        &mut self,
        label: NodeId,
        field: NodeId,
        label_width: f64,
        field_min_width: f64,
        height: f64,
    ) -> Result<(), LayoutError> {
        if !valid_extent(label_width) || !valid_extent(field_min_width) || !valid_extent(height) {
            return Err(LayoutError::InvalidGeometry);
        }
        if label == field || self.identities.contains(&label) || self.identities.contains(&field) {
            return Err(LayoutError::DuplicateNode);
        }
        self.identities.insert(label);
        self.identities.insert(field);
        self.rows.push(FormRow {
            label,
            field,
            label_width,
            field_min_width,
            height,
        });
        Ok(())
    }

    pub fn measure(&self) -> Size {
        if self.rows.is_empty() {
            return Size::new(
                self.padding.left + self.padding.right,
                self.padding.top + self.padding.bottom,
            );
        }
        let label = self
            .rows
            .iter()
            .map(|row| row.label_width)
            .fold(0.0, f64::max);
        let field = self
            .rows
            .iter()
            .map(|row| row.field_min_width)
            .fold(0.0, f64::max);
        let heights: f64 = self.rows.iter().map(|row| row.height).sum();
        Size::new(
            self.padding.left + label + self.column_gap + field + self.padding.right,
            self.padding.top
                + heights
                + self.row_gap * (self.rows.len() - 1) as f64
                + self.padding.bottom,
        )
    }

    pub fn arrange(&self, bounds: Rect) -> Result<Vec<(NodeId, Rect)>, LayoutError> {
        validate_bounds(bounds)?;
        if self.rows.is_empty() {
            if bounds.size.width < self.padding.left + self.padding.right
                || bounds.size.height < self.padding.top + self.padding.bottom
            {
                return Err(LayoutError::InsufficientSpace);
            }
            return Ok(Vec::new());
        }
        let label_track = self
            .rows
            .iter()
            .map(|row| row.label_width)
            .fold(0.0, f64::max);
        let field_min = self
            .rows
            .iter()
            .map(|row| row.field_min_width)
            .fold(0.0, f64::max);
        let mut grid =
            GridLayout::new(self.direction, self.padding, self.row_gap, self.column_gap)?;
        grid.add_column_fixed(label_track)?;
        grid.add_column_flex(1.0, field_min, f64::INFINITY)?;
        for (index, row) in self.rows.iter().enumerate() {
            grid.add_row_fixed(row.height)?;
            grid.place(row.label, index, 0, 1, 1)?;
            grid.place(row.field, index, 1, 1, 1)?;
        }
        let mut arranged = grid.arrange(bounds)?;
        for (index, row) in self.rows.iter().enumerate() {
            let label_rect = &mut arranged[index * 2].1;
            if self.direction == LayoutDirection::Ltr {
                label_rect.origin.x += label_track - row.label_width;
            }
            label_rect.size.width = row.label_width;
        }
        Ok(arranged)
    }

    pub fn apply(&self, tree: &mut Tree, bounds: Rect) -> Result<(), LayoutError> {
        let arranged = self.arrange(bounds)?;
        if arranged.iter().any(|(node, _)| !tree.contains(*node)) {
            return Err(LayoutError::MissingNode);
        }
        for (node, rect) in arranged {
            tree.set_bounds(node, rect)
                .map_err(|_| LayoutError::MissingNode)?;
        }
        Ok(())
    }
}
