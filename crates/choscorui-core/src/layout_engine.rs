//! Constraint-based measure and arrange over the retained tree.
//!
//! Every laid-out node has a [`LayoutStyle`]. Containers (stack, grid,
//! overlay, scroll) size themselves from their children; leaves ask a
//! [`LeafMeasure`] implementation for intrinsic content size, which is how
//! text reports wrapped height for a given width. Results are cached per
//! node and constraint, and invalidated explicitly when content or style
//! changes, so an unchanged subtree is never remeasured.
//!
//! Layout cannot re-enter itself: a [`LeafMeasure`] receives no access to
//! the engine or tree, so the borrow checker rejects measurement code that
//! would request layout recursively. Tree structure is acyclic by
//! construction ([`Tree::place_child`] rejects cycles).

use crate::{CrossAlign, Insets, LayoutDirection, NodeId, Point, Rect, Size, Tree};
use std::collections::HashMap;
use std::rc::Rc;

/// Minimum and maximum size a parent offers a child. `max` may be infinite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Constraints {
    pub const UNBOUNDED: Self = Self {
        min: Size::new(0.0, 0.0),
        max: Size::new(f64::INFINITY, f64::INFINITY),
    };

    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::new(0.0, 0.0),
            max,
        }
    }

    pub fn constrain(self, size: Size) -> Size {
        Size::new(
            size.width.max(self.min.width).min(self.max.width),
            size.height.max(self.min.height).min(self.max.height),
        )
    }

    fn key(self) -> [u64; 4] {
        [
            self.min.width.to_bits(),
            self.min.height.to_bits(),
            self.max.width.to_bits(),
            self.max.height.to_bits(),
        ]
    }
}

/// Result of measuring one node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    pub size: Size,
    /// Distance from the top edge to the first text baseline, if any.
    pub baseline: Option<f64>,
}

impl Measured {
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            baseline: None,
        }
    }
}

/// Supplies intrinsic sizes for leaves (text, images, custom controls).
pub trait LeafMeasure {
    fn measure(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        direction: LayoutDirection,
    ) -> Measured;
}

impl<F: FnMut(NodeId, Constraints, LayoutDirection) -> Measured> LeafMeasure for F {
    fn measure(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        direction: LayoutDirection,
    ) -> Measured {
        self(node, constraints, direction)
    }
}

/// A preferred dimension.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Size to content.
    #[default]
    Auto,
    Points(f64),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MainAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Cross-axis alignment for a stack child or grid cell, extending
/// [`CrossAlign`] with first-baseline alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

impl From<CrossAlign> for ItemAlign {
    fn from(value: CrossAlign) -> Self {
        match value {
            CrossAlign::Start => Self::Start,
            CrossAlign::Center => Self::Center,
            CrossAlign::End => Self::End,
            CrossAlign::Stretch => Self::Stretch,
        }
    }
}

/// A grid row or column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    Fixed(f64),
    /// Largest content of single-span cells in the track.
    Auto,
    /// Share of remaining space by weight, within `min..=max`.
    Flex {
        weight: f64,
        min: f64,
        max: f64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridPlacement {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

impl GridPlacement {
    pub const fn cell(row: usize, column: usize) -> Self {
        Self {
            row,
            column,
            row_span: 1,
            column_span: 1,
        }
    }

    pub const fn span(row: usize, column: usize, row_span: usize, column_span: usize) -> Self {
        Self {
            row,
            column,
            row_span,
            column_span,
        }
    }
}

/// Measures the children of a [`LayoutKind::Custom`] node for its
/// [`CustomLayout`].
pub trait ChildMeasure {
    /// The number of children that take part in layout.
    fn len(&self) -> usize;

    /// True when no child takes part in layout.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Measures child `index` within `constraints`. An index out of range
    /// measures as zero.
    fn measure(&mut self, index: usize, constraints: Constraints) -> Measured;
}

/// A layout that the built-in containers cannot express, such as a stack
/// that changes its axis with the width it gets.
pub trait CustomLayout {
    /// Measures the children through `children` and places them in a
    /// content box that `space` limits. Returns the content size and one
    /// rectangle for each child, relative to the content box origin.
    /// While a parent measures, `space.max` can be infinite. When the
    /// engine arranges the node, `space` is tight. The layout mirrors
    /// itself in a right-to-left `direction`.
    fn layout(
        &self,
        space: Constraints,
        direction: LayoutDirection,
        children: &mut dyn ChildMeasure,
    ) -> (Size, Vec<Rect>);
}

/// A [`CustomLayout`] that a [`LayoutStyle`] can hold. Two are equal when
/// they share the same layout object.
#[derive(Clone)]
pub struct SharedLayout(Rc<dyn CustomLayout>);

impl SharedLayout {
    pub fn new(layout: impl CustomLayout + 'static) -> Self {
        Self(Rc::new(layout))
    }
}

impl PartialEq for SharedLayout {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.0), Rc::as_ptr(&other.0))
    }
}

impl std::fmt::Debug for SharedLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedLayout")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutKind {
    /// Sized by [`LeafMeasure`]; children, if any, are not laid out.
    Leaf,
    /// Children in a row (`Horizontal`) or column (`Vertical`).
    Stack {
        axis: crate::Axis,
        spacing: f64,
        main_align: MainAlign,
        cross_align: ItemAlign,
    },
    Grid {
        columns: Vec<Track>,
        rows: Vec<Track>,
        column_gap: f64,
        row_gap: f64,
    },
    /// Every child receives the full content box, aligned per child.
    Overlay,
    /// One content child measured unbounded along `axis` and offset by the
    /// node's scroll offset.
    Scroll { axis: crate::Axis },
    /// Children that a [`CustomLayout`] measures and places.
    Custom(SharedLayout),
}

/// Per-node layout input.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    pub kind: LayoutKind,
    pub width: Length,
    pub height: Length,
    pub min: Size,
    pub max: Size,
    pub padding: Insets,
    /// Share of the parent stack's remaining main-axis space. Zero keeps the
    /// measured size.
    pub grow: f64,
    /// Whether the child may shrink below its measured size, down to `min`,
    /// when the parent stack overflows.
    pub shrink: bool,
    pub align_self: Option<ItemAlign>,
    /// Inline-axis alignment inside a grid cell or an overlay: `Start` is
    /// the leading edge (left in LTR, right in RTL). A grid cell defaults
    /// to `Stretch`; an overlay child defaults to its `align_self`.
    pub justify_self: Option<ItemAlign>,
    pub grid: GridPlacement,
    /// `None` inherits the parent's direction.
    pub direction: Option<LayoutDirection>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            kind: LayoutKind::Leaf,
            width: Length::Auto,
            height: Length::Auto,
            min: Size::new(0.0, 0.0),
            max: Size::new(f64::INFINITY, f64::INFINITY),
            padding: Insets::default(),
            grow: 0.0,
            shrink: false,
            align_self: None,
            justify_self: None,
            grid: GridPlacement::cell(0, 0),
            direction: None,
        }
    }
}

impl LayoutStyle {
    pub fn leaf() -> Self {
        Self::default()
    }

    pub fn row(spacing: f64) -> Self {
        Self {
            kind: LayoutKind::Stack {
                axis: crate::Axis::Horizontal,
                spacing,
                main_align: MainAlign::Start,
                cross_align: ItemAlign::Stretch,
            },
            ..Self::default()
        }
    }

    pub fn column(spacing: f64) -> Self {
        Self {
            kind: LayoutKind::Stack {
                axis: crate::Axis::Vertical,
                spacing,
                main_align: MainAlign::Start,
                cross_align: ItemAlign::Stretch,
            },
            ..Self::default()
        }
    }

    pub fn grid(columns: Vec<Track>, rows: Vec<Track>, gap: f64) -> Self {
        Self {
            kind: LayoutKind::Grid {
                columns,
                rows,
                column_gap: gap,
                row_gap: gap,
            },
            ..Self::default()
        }
    }

    /// Two-column form: labels sized to content and trailing-aligned beside
    /// growing fields, one row per pair, with baseline-aligned rows.
    pub fn form(rows: usize, column_gap: f64, row_gap: f64) -> Self {
        Self {
            kind: LayoutKind::Grid {
                columns: vec![
                    Track::Auto,
                    Track::Flex {
                        weight: 1.0,
                        min: 0.0,
                        max: f64::INFINITY,
                    },
                ],
                rows: vec![Track::Auto; rows],
                column_gap,
                row_gap,
            },
            ..Self::default()
        }
    }

    pub fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }
    pub fn with_grow(mut self, grow: f64) -> Self {
        self.grow = grow;
        self
    }
    pub fn with_size(mut self, width: Length, height: Length) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    pub fn with_min(mut self, min: Size) -> Self {
        self.min = min;
        self
    }
    pub fn with_max(mut self, max: Size) -> Self {
        self.max = max;
        self
    }
    pub fn with_align(mut self, align: ItemAlign) -> Self {
        self.align_self = Some(align);
        self
    }
    pub fn with_justify(mut self, justify: ItemAlign) -> Self {
        self.justify_self = Some(justify);
        self
    }

    /// Form label in `row`: trailing-aligned toward its field, baseline
    /// aligned with it.
    pub fn form_label(row: usize) -> Self {
        Self::leaf()
            .with_cell(GridPlacement::cell(row, 0))
            .with_justify(ItemAlign::End)
    }

    /// Form field in `row`, filling the growing second column.
    pub fn form_field(row: usize) -> Self {
        Self::leaf().with_cell(GridPlacement::cell(row, 1))
    }

    pub fn with_cell(mut self, placement: GridPlacement) -> Self {
        self.grid = placement;
        self
    }
    pub fn with_direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = Some(direction);
        self
    }
    pub fn with_shrink(mut self, shrink: bool) -> Self {
        self.shrink = shrink;
        self
    }

    fn validate(&self) -> Result<(), EngineError> {
        let finite_nonneg = |v: f64| v.is_finite() && v >= 0.0;
        let length_ok = |l: Length| match l {
            Length::Auto => true,
            Length::Points(v) => finite_nonneg(v),
        };
        let pad = self.padding;
        let ok = length_ok(self.width)
            && length_ok(self.height)
            && finite_nonneg(self.min.width)
            && finite_nonneg(self.min.height)
            && self.max.width >= self.min.width
            && self.max.height >= self.min.height
            && finite_nonneg(self.grow)
            && [pad.left, pad.top, pad.right, pad.bottom]
                .into_iter()
                .all(finite_nonneg)
            && match &self.kind {
                LayoutKind::Stack { spacing, .. } => finite_nonneg(*spacing),
                LayoutKind::Grid {
                    columns,
                    rows,
                    column_gap,
                    row_gap,
                } => {
                    finite_nonneg(*column_gap)
                        && finite_nonneg(*row_gap)
                        && columns.iter().chain(rows).all(|track| match *track {
                            Track::Fixed(v) => finite_nonneg(v),
                            Track::Auto => true,
                            Track::Flex { weight, min, max } => {
                                weight.is_finite()
                                    && weight > 0.0
                                    && finite_nonneg(min)
                                    && max >= min
                            }
                        })
                }
                _ => true,
            };
        if ok {
            Ok(())
        } else {
            Err(EngineError::InvalidStyle)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineError {
    MissingNode(NodeId),
    InvalidStyle,
    /// A child's minimum size cannot fit; reported only in strict mode.
    Unsatisfiable(NodeId),
    /// A grid child is placed outside the declared tracks.
    InvalidPlacement(NodeId),
    InvalidBounds,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode(node) => write!(f, "node {node:?} is not in the tree"),
            Self::InvalidStyle => {
                f.write_str("the layout style has a non-finite or negative value")
            }
            Self::Unsatisfiable(node) => {
                write!(f, "the constraints of node {node:?} cannot be satisfied")
            }
            Self::InvalidPlacement(node) => {
                write!(f, "node {node:?} has an invalid grid placement")
            }
            Self::InvalidBounds => f.write_str("the root bounds are not finite"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Counters for the most recent [`LayoutEngine::layout`] call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutStats {
    pub leaf_measurements: usize,
    pub cache_hits: usize,
    /// Nodes whose children did not fit; content overflows and is clipped.
    pub overflowed: Vec<NodeId>,
}

#[derive(Default)]
struct NodeCache {
    /// Measurements by constraints and direction. An invalidation
    /// empties them.
    entries: Vec<([u64; 4], u8, Measured)>,
    /// The kept work of a stack, which an invalidation of a child keeps.
    stack: Option<Box<StackMemo>>,
}

/// The inputs of the first measuring pass of a stack, other than its
/// children.
#[derive(Clone, Copy, PartialEq)]
struct StackKey {
    horizontal: bool,
    cross_align: ItemAlign,
    min_cross: u64,
    max_cross: u64,
    direction: LayoutDirection,
    shown: bool,
}

/// The first measuring pass of a stack for one [`StackKey`].
struct StackPass {
    key: StackKey,
    /// The natural size of each laid-out child, in child order. Empty
    /// when the pass must run again for all children.
    items: Vec<StackItem>,
    /// Children to measure again.
    remeasure: Vec<NodeId>,
}

/// What a stack keeps between layouts, so that a change to one child
/// measures and arranges only that child.
struct StackMemo {
    /// The [`Tree::children_revision`] of the stack.
    revision: u64,
    /// The most recent passes. A parent can measure a stack in other
    /// constraints than the stack gets when it is arranged.
    passes: Vec<StackPass>,
    /// The direction and the children with their rectangles from the last
    /// arrangement. Empty before the first arrangement.
    rects: Vec<(NodeId, Rect)>,
    direction: LayoutDirection,
    /// Children to arrange again.
    rearrange: Vec<NodeId>,
}

/// More stale children than this make a stack measure or arrange all of
/// its children again.
const STALE_LIMIT: usize = 32;

/// The number of passes that a stack keeps.
const STACK_PASSES: usize = 2;

impl StackMemo {
    /// Records that `child` lost its measurement (`measure`) or only its
    /// arrangement.
    fn stale(&mut self, child: NodeId, measure: bool) {
        if measure {
            for pass in &mut self.passes {
                if !pass.items.is_empty() && !add_stale(&mut pass.remeasure, child) {
                    pass.items.clear();
                }
            }
        }
        if !self.rects.is_empty() && !add_stale(&mut self.rearrange, child) {
            self.rects.clear();
        }
    }
}

/// Adds `child` to `stale`. Past [`STALE_LIMIT`] children, it empties
/// `stale` and returns false: then the next pass visits every child.
fn add_stale(stale: &mut Vec<NodeId>, child: NodeId) -> bool {
    if stale.contains(&child) {
        return true;
    }
    if stale.len() == STALE_LIMIT {
        stale.clear();
        return false;
    }
    stale.push(child);
    true
}

/// Owns layout styles, caches, and scroll offsets for one retained tree.
pub struct LayoutEngine {
    styles: HashMap<NodeId, LayoutStyle>,
    cache: HashMap<NodeId, NodeCache>,
    /// The bounds and direction that each node's subtree was last arranged
    /// in. A clean node that gets the same again keeps its subtree as is.
    arranged: HashMap<NodeId, (Rect, u8)>,
    /// The nodes that the last layout placed.
    placed: Vec<NodeId>,
    scroll_offsets: HashMap<NodeId, Point>,
    /// Scroll nodes whose offset changed since the last layout. The next
    /// layout arranges the path from the root to each of them again.
    scrolled: Vec<NodeId>,
    content_sizes: HashMap<NodeId, Size>,
    strict: bool,
    stats: LayoutStats,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

const CACHE_SLOTS: usize = 4;

impl LayoutEngine {
    /// Strict mode (unsatisfiable constraints become errors) is the default
    /// in debug builds; release builds overflow and report in stats.
    pub fn new() -> Self {
        Self {
            styles: HashMap::new(),
            cache: HashMap::new(),
            arranged: HashMap::new(),
            placed: Vec::new(),
            scroll_offsets: HashMap::new(),
            scrolled: Vec::new(),
            content_sizes: HashMap::new(),
            strict: cfg!(debug_assertions),
            stats: LayoutStats::default(),
        }
    }

    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    /// Sets a node's style and invalidates it and its ancestors.
    pub fn set_style(
        &mut self,
        tree: &Tree,
        node: NodeId,
        style: LayoutStyle,
    ) -> Result<(), EngineError> {
        if !tree.contains(node) {
            return Err(EngineError::MissingNode(node));
        }
        style.validate()?;
        if self.styles.get(&node) != Some(&style) {
            self.styles.insert(node, style);
            self.invalidate(tree, node);
        }
        Ok(())
    }

    pub fn style(&self, node: NodeId) -> Option<&LayoutStyle> {
        self.styles.get(&node)
    }

    /// Drops state for nodes no longer in the tree.
    pub fn prune(&mut self, tree: &Tree) {
        self.styles.retain(|node, _| tree.contains(*node));
        self.cache.retain(|node, _| tree.contains(*node));
        self.arranged.retain(|node, _| tree.contains(*node));
        self.scroll_offsets.retain(|node, _| tree.contains(*node));
        self.content_sizes.retain(|node, _| tree.contains(*node));
    }

    /// Content of `node` changed (text, font, metric). Clears cached
    /// measurements for the node and every ancestor. A stack ancestor
    /// keeps the measurements of its other children.
    pub fn invalidate(&mut self, tree: &Tree, node: NodeId) {
        self.cache.remove(&node);
        self.arranged.remove(&node);
        let mut child = node;
        while let Some(parent) = tree.parent(child) {
            self.arranged.remove(&parent);
            if let Some(cache) = self.cache.get_mut(&parent) {
                cache.entries.clear();
                if let Some(memo) = &mut cache.stack {
                    memo.stale(child, true);
                }
            }
            child = parent;
        }
    }

    /// Tells the stack that holds `node`, and each stack above it, that
    /// the arrangement on the path to `node` must run again.
    fn unarrange_path(&mut self, tree: &Tree, node: NodeId) {
        let mut child = node;
        while let Some(parent) = tree.parent(child) {
            self.arranged.remove(&parent);
            if let Some(memo) = self.stack_memo(parent) {
                memo.stale(child, false);
            }
            child = parent;
        }
    }

    /// The kept work of the stack `node`, if any.
    fn stack_memo(&mut self, node: NodeId) -> Option<&mut StackMemo> {
        self.cache.get_mut(&node)?.stack.as_deref_mut()
    }

    /// Forgets every arrangement, for example after a failed pass.
    fn unarrange_all(&mut self) {
        self.arranged.clear();
        for cache in self.cache.values_mut() {
            cache.stack = None;
        }
    }

    /// A theme metric, scale, or locale change invalidates everything.
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
        self.unarrange_all();
    }

    pub fn scroll_offset(&self, node: NodeId) -> Point {
        self.scroll_offsets.get(&node).copied().unwrap_or_default()
    }

    /// Scrolling changes placement only; cached measurements stay valid.
    /// The next layout arranges the scroll node and its ancestors again.
    pub fn set_scroll_offset(&mut self, node: NodeId, offset: Point) {
        if self.scroll_offsets.insert(node, offset) != Some(offset) {
            self.arranged.remove(&node);
            self.scrolled.push(node);
        }
    }

    /// Content size of a scroll node from the last layout.
    pub fn content_size(&self, node: NodeId) -> Option<Size> {
        self.content_sizes.get(&node).copied()
    }

    pub fn stats(&self) -> &LayoutStats {
        &self.stats
    }

    /// Measures without arranging.
    pub fn measure(
        &mut self,
        tree: &Tree,
        node: NodeId,
        constraints: Constraints,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<Measured, EngineError> {
        self.stats = LayoutStats::default();
        let direction = self.inherited_direction(tree, node);
        self.measure_node(tree, node, constraints, direction, leaves)
    }

    /// Measures `root` against tight `bounds` and writes absolute bounds to
    /// the tree. Placement uses [`Tree::set_layout_bounds`] so layout does
    /// not mark itself dirty again.
    pub fn layout(
        &mut self,
        tree: &mut Tree,
        root: NodeId,
        bounds: Rect,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<&LayoutStats, EngineError> {
        let placements = self.plan(tree, root, bounds, leaves)?;
        self.place(tree, placements)?;
        Ok(&self.stats)
    }

    /// Measures and arranges `root` in `bounds` without changing the tree,
    /// and returns the bounds to give each node. Only invalidated nodes and
    /// nodes whose bounds change are arranged again, so the work follows
    /// what changed, not the size of the tree. Give the result to
    /// [`LayoutEngine::place`].
    pub fn plan(
        &mut self,
        tree: &Tree,
        root: NodeId,
        bounds: Rect,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<Vec<(NodeId, Rect)>, EngineError> {
        if !bounds.is_finite() || bounds.size.width < 0.0 || bounds.size.height < 0.0 {
            return Err(EngineError::InvalidBounds);
        }
        self.stats = LayoutStats::default();
        for node in std::mem::take(&mut self.scrolled) {
            self.unarrange_path(tree, node);
        }
        // A root below a stack gets an arrangement that the stack did not
        // give it.
        if let Some(memo) = tree.parent(root).and_then(|parent| self.stack_memo(parent)) {
            memo.stale(root, false);
        }
        let direction = self.inherited_direction(tree, root);
        let mut placements = Vec::new();
        let result = self
            .measure_node(
                tree,
                root,
                Constraints::tight(bounds.size),
                direction,
                leaves,
            )
            .and_then(|_| {
                self.arrange_node(tree, root, bounds, direction, leaves, &mut placements)
            });
        if let Err(error) = result {
            // A failed pass leaves some subtrees unarranged.
            self.unarrange_all();
            return Err(error);
        }
        Ok(placements)
    }

    /// Writes the bounds that [`LayoutEngine::plan`] returned to the tree.
    pub fn place(
        &mut self,
        tree: &mut Tree,
        placements: Vec<(NodeId, Rect)>,
    ) -> Result<(), EngineError> {
        self.placed.clear();
        for (node, rect) in placements {
            if tree.set_layout_bounds(node, rect).is_err() {
                self.unarrange_all();
                return Err(EngineError::MissingNode(node));
            }
            self.placed.push(node);
        }
        Ok(())
    }

    /// The nodes that the last layout placed. The other nodes kept their
    /// bounds and the arrangement of their subtrees.
    pub fn placed(&self) -> &[NodeId] {
        &self.placed
    }

    fn inherited_direction(&self, tree: &Tree, node: NodeId) -> LayoutDirection {
        let mut current = Some(node);
        while let Some(id) = current {
            if let Some(direction) = self.styles.get(&id).and_then(|style| style.direction) {
                return direction;
            }
            current = tree.parent(id);
        }
        LayoutDirection::Ltr
    }

    fn style_of(&self, node: NodeId) -> LayoutStyle {
        self.styles.get(&node).cloned().unwrap_or_default()
    }

    /// The visible children of `node`. A child is visible when its own
    /// flag is set and `node` is visible.
    fn laid_out_children(&self, tree: &Tree, node: NodeId) -> Vec<NodeId> {
        if tree.visible(node) != Some(true) {
            return Vec::new();
        }
        tree.children(node)
            .unwrap_or(&[])
            .iter()
            .copied()
            .filter(|child| tree.own_visible(*child) == Some(true))
            .collect()
    }

    fn measure_node(
        &mut self,
        tree: &Tree,
        node: NodeId,
        constraints: Constraints,
        inherited: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<Measured, EngineError> {
        if !tree.contains(node) {
            return Err(EngineError::MissingNode(node));
        }
        let direction = self
            .styles
            .get(&node)
            .and_then(|style| style.direction)
            .unwrap_or(inherited);
        let key = constraints.key();
        let dir_key = direction as u8;
        if let Some(hit) = self.cache.get(&node).and_then(|cache| {
            cache
                .entries
                .iter()
                .find(|(k, d, _)| *k == key && *d == dir_key)
                .map(|(_, _, m)| *m)
        }) {
            self.stats.cache_hits += 1;
            return Ok(hit);
        }
        let style = self.style_of(node);
        let own = own_constraints(&style, constraints);
        let pad = style.padding;
        let pad_w = pad.left + pad.right;
        let pad_h = pad.top + pad.bottom;
        let inner = Constraints {
            min: Size::new(
                (own.min.width - pad_w).max(0.0),
                (own.min.height - pad_h).max(0.0),
            ),
            max: Size::new(
                (own.max.width - pad_w).max(0.0),
                (own.max.height - pad_h).max(0.0),
            ),
        };
        let content = match &style.kind {
            LayoutKind::Leaf => {
                self.stats.leaf_measurements += 1;
                leaves.measure(node, inner, direction)
            }
            LayoutKind::Stack {
                axis,
                spacing,
                cross_align,
                ..
            } => {
                let plan = self.plan_stack(
                    tree,
                    node,
                    *axis,
                    *spacing,
                    *cross_align,
                    inner,
                    direction,
                    leaves,
                )?;
                Measured {
                    size: plan.content,
                    baseline: plan.baseline,
                }
            }
            LayoutKind::Grid { .. } => {
                let plan = self.plan_grid(tree, node, &style, inner, direction, leaves)?;
                Measured {
                    size: plan.content,
                    baseline: plan.baseline,
                }
            }
            LayoutKind::Overlay => {
                let mut size = Size::ZERO;
                let mut baseline = None;
                for child in self.laid_out_children(tree, node) {
                    let m = self.measure_node(
                        tree,
                        child,
                        Constraints::loose(inner.max),
                        direction,
                        leaves,
                    )?;
                    size.width = size.width.max(m.size.width);
                    size.height = size.height.max(m.size.height);
                    baseline = baseline.or(m.baseline);
                }
                Measured { size, baseline }
            }
            LayoutKind::Scroll { axis } => {
                let mut size = Size::ZERO;
                if let Some(child) = self.laid_out_children(tree, node).first().copied() {
                    let child_constraints = scroll_child_constraints(*axis, inner);
                    size = self
                        .measure_node(tree, child, child_constraints, direction, leaves)?
                        .size;
                }
                // A scroll view does not grow with its content along the axis.
                match axis {
                    crate::Axis::Vertical => size.height = inner.min.height,
                    crate::Axis::Horizontal => size.width = inner.min.width,
                }
                Measured::new(size)
            }
            LayoutKind::Custom(layout) => {
                let children = self.laid_out_children(tree, node);
                let (size, _) =
                    self.run_custom(tree, layout, &children, inner, direction, leaves)?;
                Measured::new(size)
            }
        };
        let size = own.constrain(Size::new(
            content.size.width + pad_w,
            content.size.height + pad_h,
        ));
        let measured = Measured {
            size,
            baseline: content.baseline.map(|b| b + pad.top),
        };
        let cache = self.cache.entry(node).or_default();
        if cache.entries.len() == CACHE_SLOTS {
            cache.entries.remove(0);
        }
        cache.entries.push((key, dir_key, measured));
        Ok(measured)
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_stack(
        &mut self,
        tree: &Tree,
        node: NodeId,
        axis: crate::Axis,
        spacing: f64,
        cross_align: ItemAlign,
        inner: Constraints,
        direction: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<StackPlan, EngineError> {
        let horizontal = axis == crate::Axis::Horizontal;
        let main_of = |s: Size| if horizontal { s.width } else { s.height };
        let cross_of = |s: Size| if horizontal { s.height } else { s.width };
        let make = |main: f64, cross: f64| {
            if horizontal {
                Size::new(main, cross)
            } else {
                Size::new(cross, main)
            }
        };
        let max_main = main_of(inner.max);
        let max_cross = cross_of(inner.max);
        let min_cross = cross_of(inner.min);
        // Pass 1: natural sizes with unbounded main axis.
        let key = StackKey {
            horizontal,
            cross_align,
            min_cross: min_cross.to_bits(),
            max_cross: max_cross.to_bits(),
            direction,
            shown: tree.visible(node) == Some(true),
        };
        let mut items = self.stack_items(tree, node, key, leaves)?;
        let gaps = spacing * items.len().saturating_sub(1) as f64;
        let natural: f64 = items.iter().map(|item| item.main).sum::<f64>() + gaps;
        let mut overflow = false;
        if max_main.is_finite() {
            let mut free = max_main - natural;
            if free > 0.0 && (main_of(inner.min) > natural || items.iter().any(|i| i.grow > 0.0)) {
                distribute_grow(&mut items, free);
            } else if free < 0.0 {
                free = shrink_items(&mut items, -free);
                if free > 1e-9 {
                    overflow = true;
                }
            }
        }
        if overflow {
            if self.strict {
                return Err(EngineError::Unsatisfiable(node));
            }
            if !self.stats.overflowed.contains(&node) {
                self.stats.overflowed.push(node);
            }
        }
        // Pass 2: re-measure items whose main size changed, at that size.
        let mut cross = 0.0f64;
        let mut ascent = 0.0f64;
        let mut descent = 0.0f64;
        for item in &mut items {
            if (item.main - main_of(item.measured.size)).abs() > 1e-9 {
                let cross_min = if item.align == ItemAlign::Stretch && max_cross.is_finite() {
                    min_cross
                } else {
                    0.0
                };
                let c = Constraints {
                    min: make(item.main, cross_min),
                    max: make(item.main, max_cross),
                };
                item.measured = self.measure_node(tree, item.node, c, direction, leaves)?;
            }
            let item_cross = cross_of(item.measured.size);
            cross = cross.max(item_cross);
            if horizontal && item.align == ItemAlign::Baseline {
                let baseline = item.measured.baseline.unwrap_or(item_cross);
                ascent = ascent.max(baseline);
                descent = descent.max(item_cross - baseline);
            }
        }
        cross = cross.max(ascent + descent);
        let used_main: f64 = items.iter().map(|item| item.main).sum::<f64>() + gaps;
        let baseline = if horizontal {
            if ascent > 0.0 {
                Some(ascent)
            } else {
                items.iter().find_map(|item| item.measured.baseline)
            }
        } else {
            items.first().and_then(|item| item.measured.baseline)
        };
        Ok(StackPlan {
            items,
            content: make(used_main, cross),
            baseline,
            ascent,
        })
    }

    /// Pass 1 of [`LayoutEngine::plan_stack`]: the natural size of each
    /// laid-out child of `node`. The stack keeps the result, so that the
    /// next call measures only the children that changed.
    fn stack_items(
        &mut self,
        tree: &Tree,
        node: NodeId,
        key: StackKey,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<Vec<StackItem>, EngineError> {
        let revision = tree.children_revision(node).unwrap_or_default();
        let kept = self
            .cache
            .get_mut(&node)
            .and_then(|cache| cache.stack.take());
        let mut memo = match kept {
            Some(memo) if memo.revision == revision => memo,
            _ => Box::new(StackMemo {
                revision,
                passes: Vec::new(),
                rects: Vec::new(),
                direction: key.direction,
                rearrange: Vec::new(),
            }),
        };
        let mut pass = match memo.passes.iter().position(|pass| pass.key == key) {
            Some(index) => memo.passes.remove(index),
            None => StackPass {
                key,
                items: Vec::new(),
                remeasure: Vec::new(),
            },
        };
        let mut reuse = !pass.items.is_empty();
        for child in std::mem::take(&mut pass.remeasure) {
            let found = pass.items.iter().position(|item| item.node == child);
            let Some(index) = found.filter(|_| reuse) else {
                reuse = false;
                break;
            };
            pass.items[index] = self.stack_item(tree, child, key, leaves)?;
        }
        if !reuse {
            let children = if key.shown {
                self.laid_out_children(tree, node)
            } else {
                Vec::new()
            };
            pass.items = Vec::with_capacity(children.len());
            for child in children {
                let item = self.stack_item(tree, child, key, leaves)?;
                pass.items.push(item);
            }
        }
        let items = pass.items.clone();
        if memo.passes.len() == STACK_PASSES {
            memo.passes.remove(0);
        }
        memo.passes.push(pass);
        self.cache.entry(node).or_default().stack = Some(memo);
        Ok(items)
    }

    /// Measures one child of a stack along an unbounded main axis.
    fn stack_item(
        &mut self,
        tree: &Tree,
        child: NodeId,
        key: StackKey,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<StackItem, EngineError> {
        let horizontal = key.horizontal;
        let main_of = |s: Size| if horizontal { s.width } else { s.height };
        let make = |main: f64, cross: f64| {
            if horizontal {
                Size::new(main, cross)
            } else {
                Size::new(cross, main)
            }
        };
        let min_cross = f64::from_bits(key.min_cross);
        let max_cross = f64::from_bits(key.max_cross);
        let style = self.styles.get(&child);
        let align = style.and_then(|style| style.align_self);
        let align = align.unwrap_or(key.cross_align);
        let min = style.map_or(Size::ZERO, |style| style.min);
        let max = style.map_or(Size::new(f64::INFINITY, f64::INFINITY), |style| style.max);
        let grow = style.map_or(0.0, |style| style.grow);
        let shrink = style.is_some_and(|style| style.shrink);
        let cross_min = if align == ItemAlign::Stretch && max_cross.is_finite() {
            min_cross
        } else {
            0.0
        };
        let c = Constraints {
            min: make(0.0, cross_min),
            max: make(f64::INFINITY, max_cross),
        };
        let measured = self.measure_node(tree, child, c, key.direction, leaves)?;
        Ok(StackItem {
            node: child,
            main: main_of(measured.size),
            min_main: main_of(min),
            max_main: main_of(max),
            grow,
            shrink,
            align,
            measured,
        })
    }

    fn plan_grid(
        &mut self,
        tree: &Tree,
        node: NodeId,
        style: &LayoutStyle,
        inner: Constraints,
        direction: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<GridPlan, EngineError> {
        let LayoutKind::Grid {
            columns,
            rows,
            column_gap,
            row_gap,
        } = &style.kind
        else {
            unreachable!("plan_grid requires a grid");
        };
        let children = self.laid_out_children(tree, node);
        let mut cells = Vec::with_capacity(children.len());
        for child in children {
            let child_style = self.style_of(child);
            let p = child_style.grid;
            if p.row_span == 0
                || p.column_span == 0
                || p.row + p.row_span > rows.len()
                || p.column + p.column_span > columns.len()
            {
                return Err(EngineError::InvalidPlacement(child));
            }
            cells.push((child, p, child_style.align_self, child_style.justify_self));
        }
        // Columns: fixed and auto sizes from single-span cell content.
        let mut col_sizes = base_track_sizes(columns);
        for (child, p, _, _) in &cells {
            if p.column_span == 1 && columns[p.column] == Track::Auto {
                let m =
                    self.measure_node(tree, *child, Constraints::UNBOUNDED, direction, leaves)?;
                col_sizes[p.column] = col_sizes[p.column].max(m.size.width);
            }
        }
        for (child, p, _, _) in &cells {
            if p.column_span > 1 {
                let m =
                    self.measure_node(tree, *child, Constraints::UNBOUNDED, direction, leaves)?;
                widen_span(
                    &mut col_sizes,
                    columns,
                    p.column,
                    p.column_span,
                    *column_gap,
                    m.size.width,
                );
            }
        }
        let width_budget = inner.max.width.is_finite().then_some(inner.max.width);
        let overflow_x = resolve_flex(&mut col_sizes, columns, *column_gap, width_budget);
        // Rows: measure each cell at its resolved column width.
        let mut row_sizes = base_track_sizes(rows);
        let mut measured = Vec::with_capacity(cells.len());
        for (child, p, _, _) in &cells {
            let width = span_extent(&col_sizes, p.column, p.column_span, *column_gap);
            let m = self.measure_node(
                tree,
                *child,
                Constraints {
                    min: Size::new(0.0, 0.0),
                    max: Size::new(width, f64::INFINITY),
                },
                direction,
                leaves,
            )?;
            measured.push(m);
        }
        let mut row_ascent = vec![0.0f64; rows.len()];
        let mut row_descent = vec![0.0f64; rows.len()];
        for ((_, p, align, _), m) in cells.iter().zip(&measured) {
            if p.row_span == 1 {
                if rows[p.row] == Track::Auto {
                    row_sizes[p.row] = row_sizes[p.row].max(m.size.height);
                }
                if align.unwrap_or(ItemAlign::Baseline) == ItemAlign::Baseline {
                    let b = m.baseline.unwrap_or(m.size.height);
                    row_ascent[p.row] = row_ascent[p.row].max(b);
                    row_descent[p.row] = row_descent[p.row].max(m.size.height - b);
                }
            }
        }
        for (index, size) in row_sizes.iter_mut().enumerate() {
            if rows[index] == Track::Auto {
                *size = size.max(row_ascent[index] + row_descent[index]);
            }
        }
        for ((_, p, _, _), m) in cells.iter().zip(&measured) {
            if p.row_span > 1 {
                widen_span(
                    &mut row_sizes,
                    rows,
                    p.row,
                    p.row_span,
                    *row_gap,
                    m.size.height,
                );
            }
        }
        let height_budget = inner.max.height.is_finite().then_some(inner.max.height);
        let overflow_y = resolve_flex(&mut row_sizes, rows, *row_gap, height_budget);
        if overflow_x || overflow_y {
            if self.strict {
                return Err(EngineError::Unsatisfiable(node));
            }
            if !self.stats.overflowed.contains(&node) {
                self.stats.overflowed.push(node);
            }
        }
        let width = total(&col_sizes, *column_gap);
        let height = total(&row_sizes, *row_gap);
        let baseline = row_ascent.first().copied().filter(|b| *b > 0.0);
        let _ = direction;
        Ok(GridPlan {
            cells,
            measured,
            col_sizes,
            row_sizes,
            row_ascent,
            content: Size::new(width, height),
            baseline,
        })
    }

    fn arrange_node(
        &mut self,
        tree: &Tree,
        node: NodeId,
        rect: Rect,
        inherited: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
        out: &mut Vec<(NodeId, Rect)>,
    ) -> Result<(), EngineError> {
        let arranged = (rect, inherited as u8);
        if self.arranged.get(&node) == Some(&arranged) {
            return Ok(());
        }
        self.arranged.insert(node, arranged);
        out.push((node, rect));
        let style = self.style_of(node);
        let direction = style.direction.unwrap_or(inherited);
        let pad = style.padding;
        let content = Rect::from_xywh(
            rect.origin.x + pad.left,
            rect.origin.y + pad.top,
            (rect.size.width - pad.left - pad.right).max(0.0),
            (rect.size.height - pad.top - pad.bottom).max(0.0),
        );
        let tight = Constraints::tight(content.size);
        match &style.kind {
            LayoutKind::Leaf => {}
            LayoutKind::Stack {
                axis,
                spacing,
                main_align,
                cross_align,
            } => {
                let plan = self.plan_stack(
                    tree,
                    node,
                    *axis,
                    *spacing,
                    *cross_align,
                    tight,
                    direction,
                    leaves,
                )?;
                let horizontal = *axis == crate::Axis::Horizontal;
                let main_len = if horizontal {
                    content.size.width
                } else {
                    content.size.height
                };
                let cross_len = if horizontal {
                    content.size.height
                } else {
                    content.size.width
                };
                let mut children = Vec::with_capacity(plan.items.len());
                let used: f64 = plan.items.iter().map(|i| i.main).sum();
                let gaps = spacing * plan.items.len().saturating_sub(1) as f64;
                let free = (main_len - used - gaps).max(0.0);
                let (mut cursor, extra_gap) = match main_align {
                    MainAlign::Start => (0.0, 0.0),
                    MainAlign::Center => (free / 2.0, 0.0),
                    MainAlign::End => (free, 0.0),
                    MainAlign::SpaceBetween if plan.items.len() > 1 => {
                        (0.0, free / (plan.items.len() - 1) as f64)
                    }
                    MainAlign::SpaceBetween => (0.0, 0.0),
                };
                for item in &plan.items {
                    let item_cross = if horizontal {
                        item.measured.size.height
                    } else {
                        item.measured.size.width
                    };
                    let (cross_offset, cross_size) = match item.align {
                        ItemAlign::Start => (0.0, item_cross),
                        ItemAlign::Center => ((cross_len - item_cross) / 2.0, item_cross),
                        ItemAlign::End => (cross_len - item_cross, item_cross),
                        ItemAlign::Stretch => (0.0, cross_len),
                        ItemAlign::Baseline if horizontal => {
                            let b = item.measured.baseline.unwrap_or(item_cross);
                            (plan.ascent - b, item_cross)
                        }
                        ItemAlign::Baseline => (0.0, item_cross),
                    };
                    let child_rect = if horizontal {
                        let x = if direction == LayoutDirection::Rtl {
                            content.max_x() - cursor - item.main
                        } else {
                            content.min_x() + cursor
                        };
                        Rect::from_xywh(x, content.min_y() + cross_offset, item.main, cross_size)
                    } else {
                        let x = if direction == LayoutDirection::Rtl
                            && !matches!(item.align, ItemAlign::Stretch | ItemAlign::Center)
                        {
                            content.max_x() - cross_offset - cross_size
                        } else {
                            content.min_x() + cross_offset
                        };
                        Rect::from_xywh(x, content.min_y() + cursor, cross_size, item.main)
                    };
                    children.push((item.node, child_rect));
                    cursor += item.main + spacing + extra_gap;
                }
                self.arrange_stack(tree, node, children, direction, leaves, out)?;
            }
            LayoutKind::Grid {
                column_gap,
                row_gap,
                ..
            } => {
                let plan = self.plan_grid(tree, node, &style, tight, direction, leaves)?;
                let col_starts = starts(&plan.col_sizes, *column_gap);
                let row_starts = starts(&plan.row_sizes, *row_gap);
                for ((child, p, align, justify), m) in plan.cells.iter().zip(&plan.measured) {
                    let cell_w = span_extent(&plan.col_sizes, p.column, p.column_span, *column_gap);
                    let cell_h = span_extent(&plan.row_sizes, p.row, p.row_span, *row_gap);
                    let mut x = content.min_x() + col_starts[p.column];
                    if direction == LayoutDirection::Rtl {
                        x = content.max_x() - col_starts[p.column] - cell_w;
                    }
                    let y = content.min_y() + row_starts[p.row];
                    let align = align.unwrap_or(ItemAlign::Baseline);
                    let (dy, h) = match align {
                        ItemAlign::Start => (0.0, m.size.height),
                        ItemAlign::Center => ((cell_h - m.size.height) / 2.0, m.size.height),
                        ItemAlign::End => (cell_h - m.size.height, m.size.height),
                        ItemAlign::Stretch => (0.0, cell_h),
                        ItemAlign::Baseline => {
                            let b = m.baseline.unwrap_or(m.size.height);
                            let ascent = if p.row_span == 1 {
                                plan.row_ascent[p.row]
                            } else {
                                b
                            };
                            (ascent - b, m.size.height)
                        }
                    };
                    let w = m.size.width.min(cell_w);
                    let leading = match justify.unwrap_or(ItemAlign::Stretch) {
                        ItemAlign::Stretch => None,
                        ItemAlign::Start | ItemAlign::Baseline => Some(0.0),
                        ItemAlign::Center => Some((cell_w - w) / 2.0),
                        ItemAlign::End => Some(cell_w - w),
                    };
                    let (dx, w) = match leading {
                        None => (0.0, cell_w),
                        Some(offset) if direction == LayoutDirection::Rtl => {
                            (cell_w - w - offset, w)
                        }
                        Some(offset) => (offset, w),
                    };
                    let child_rect = Rect::from_xywh(x + dx, y + dy, w, h);
                    self.arrange_node(tree, *child, child_rect, direction, leaves, out)?;
                }
            }
            LayoutKind::Overlay => {
                for child in self.laid_out_children(tree, node) {
                    let child_style = self.style_of(child);
                    let m = self.measure_node(
                        tree,
                        child,
                        Constraints::loose(content.size),
                        direction,
                        leaves,
                    )?;
                    let vertical = child_style.align_self.unwrap_or(ItemAlign::Stretch);
                    let horizontal = child_style.justify_self.unwrap_or(vertical);
                    let (x, width) = overlay_axis(
                        horizontal,
                        content.min_x(),
                        content.size.width,
                        m.size.width,
                        direction == LayoutDirection::Rtl,
                    );
                    let (y, height) = overlay_axis(
                        vertical,
                        content.min_y(),
                        content.size.height,
                        m.size.height,
                        false,
                    );
                    let child_rect = Rect::from_xywh(x, y, width, height);
                    self.arrange_node(tree, child, child_rect, direction, leaves, out)?;
                }
            }
            LayoutKind::Scroll { axis } => {
                if let Some(child) = self.laid_out_children(tree, node).first().copied() {
                    let c = scroll_child_constraints(*axis, Constraints::tight(content.size));
                    let m = self.measure_node(tree, child, c, direction, leaves)?;
                    let size = Size::new(
                        m.size.width.max(content.size.width),
                        m.size.height.max(content.size.height),
                    );
                    self.content_sizes.insert(node, size);
                    let offset = self.scroll_offset(node);
                    let max = Point::new(
                        (size.width - content.size.width).max(0.0),
                        (size.height - content.size.height).max(0.0),
                    );
                    let offset = Point::new(offset.x.clamp(0.0, max.x), offset.y.clamp(0.0, max.y));
                    self.scroll_offsets.insert(node, offset);
                    // The horizontal offset is in reading order: a
                    // right-to-left scroll starts at the right edge of
                    // its content.
                    let x = if direction == LayoutDirection::Rtl {
                        content.max_x() - size.width + offset.x
                    } else {
                        content.min_x() - offset.x
                    };
                    let child_rect = Rect::new(Point::new(x, content.min_y() - offset.y), size);
                    self.arrange_node(tree, child, child_rect, direction, leaves, out)?;
                }
            }
            LayoutKind::Custom(layout) => {
                let children = self.laid_out_children(tree, node);
                let (_, rects) =
                    self.run_custom(tree, layout, &children, tight, direction, leaves)?;
                for (index, child) in children.iter().enumerate() {
                    let rect = rects.get(index).copied().filter(|rect| rect.is_finite());
                    let child_rect = rect.map_or(Rect::new(content.origin, Size::ZERO), |rect| {
                        Rect::new(
                            Point::new(
                                content.min_x() + rect.origin.x,
                                content.min_y() + rect.origin.y,
                            ),
                            rect.size,
                        )
                    });
                    self.arrange_node(tree, *child, child_rect, direction, leaves, out)?;
                }
            }
        }
        Ok(())
    }

    /// Arranges each child of the stack `node` in its rectangle. A child
    /// that keeps its rectangle and its arrangement is not visited, so a
    /// change to one child of a long stack arranges only that child.
    fn arrange_stack(
        &mut self,
        tree: &Tree,
        node: NodeId,
        children: Vec<(NodeId, Rect)>,
        direction: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
        out: &mut Vec<(NodeId, Rect)>,
    ) -> Result<(), EngineError> {
        let (mut old, mut stale) = (Vec::new(), Vec::new());
        if let Some(memo) = self.stack_memo(node) {
            stale = std::mem::take(&mut memo.rearrange);
            if memo.direction == direction {
                old = std::mem::take(&mut memo.rects);
            }
        }
        for (index, &(child, rect)) in children.iter().enumerate() {
            if old.get(index) != Some(&(child, rect)) || stale.contains(&child) {
                self.arrange_node(tree, child, rect, direction, leaves, out)?;
            }
        }
        if let Some(memo) = self.stack_memo(node) {
            memo.rects = children;
            memo.direction = direction;
        }
        Ok(())
    }

    /// Runs a custom layout over `children` within `space`. A size that
    /// is not finite becomes zero, and the first failed child measure is
    /// the error.
    fn run_custom(
        &mut self,
        tree: &Tree,
        layout: &SharedLayout,
        children: &[NodeId],
        space: Constraints,
        direction: LayoutDirection,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<(Size, Vec<Rect>), EngineError> {
        let mut measure = Children {
            engine: self,
            tree,
            nodes: children,
            direction,
            leaves,
            error: None,
        };
        let (size, rects) = layout.0.layout(space, direction, &mut measure);
        if let Some(error) = measure.error {
            return Err(error);
        }
        let finite = |value: f64| if value.is_finite() { value } else { 0.0 };
        Ok((Size::new(finite(size.width), finite(size.height)), rects))
    }
}

/// The children of a custom layout node, measured through the engine.
struct Children<'a, 'b> {
    engine: &'a mut LayoutEngine,
    tree: &'a Tree,
    nodes: &'a [NodeId],
    direction: LayoutDirection,
    leaves: &'a mut (dyn LeafMeasure + 'b),
    error: Option<EngineError>,
}

impl ChildMeasure for Children<'_, '_> {
    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn measure(&mut self, index: usize, constraints: Constraints) -> Measured {
        let Some(node) = self.nodes.get(index).copied() else {
            return Measured::new(Size::ZERO);
        };
        match self
            .engine
            .measure_node(self.tree, node, constraints, self.direction, self.leaves)
        {
            Ok(measured) => measured,
            Err(error) => {
                self.error.get_or_insert(error);
                Measured::new(Size::ZERO)
            }
        }
    }
}

/// The start and extent of an overlay child along one axis. `Start` is
/// the leading edge, which is the right edge when `mirrored`.
fn overlay_axis(align: ItemAlign, start: f64, space: f64, size: f64, mirrored: bool) -> (f64, f64) {
    let end = start + space - size;
    match align {
        ItemAlign::Stretch => (start, space),
        ItemAlign::Center => (start + (space - size) / 2.0, size),
        ItemAlign::End if mirrored => (start, size),
        ItemAlign::End => (end, size),
        ItemAlign::Start | ItemAlign::Baseline if mirrored => (end, size),
        ItemAlign::Start | ItemAlign::Baseline => (start, size),
    }
}

fn own_constraints(style: &LayoutStyle, parent: Constraints) -> Constraints {
    let mut min = Size::new(
        style.min.width.max(parent.min.width),
        style.min.height.max(parent.min.height),
    );
    let mut max = Size::new(
        style.max.width.min(parent.max.width),
        style.max.height.min(parent.max.height),
    );
    if let Length::Points(w) = style.width {
        let w = w.clamp(min.width, max.width.max(min.width));
        min.width = w;
        max.width = w;
    }
    if let Length::Points(h) = style.height {
        let h = h.clamp(min.height, max.height.max(min.height));
        min.height = h;
        max.height = h;
    }
    max.width = max.width.max(min.width);
    max.height = max.height.max(min.height);
    Constraints { min, max }
}

fn scroll_child_constraints(axis: crate::Axis, inner: Constraints) -> Constraints {
    match axis {
        crate::Axis::Vertical => Constraints {
            min: Size::new(inner.min.width, 0.0),
            max: Size::new(inner.max.width, f64::INFINITY),
        },
        crate::Axis::Horizontal => Constraints {
            min: Size::new(0.0, inner.min.height),
            max: Size::new(f64::INFINITY, inner.max.height),
        },
    }
}

#[derive(Clone, Copy)]
struct StackItem {
    node: NodeId,
    main: f64,
    min_main: f64,
    max_main: f64,
    grow: f64,
    shrink: bool,
    align: ItemAlign,
    measured: Measured,
}

struct StackPlan {
    items: Vec<StackItem>,
    content: Size,
    baseline: Option<f64>,
    ascent: f64,
}

struct GridPlan {
    cells: Vec<(NodeId, GridPlacement, Option<ItemAlign>, Option<ItemAlign>)>,
    measured: Vec<Measured>,
    col_sizes: Vec<f64>,
    row_sizes: Vec<f64>,
    row_ascent: Vec<f64>,
    content: Size,
    baseline: Option<f64>,
}

/// Distributes positive free space by grow weight, respecting max sizes.
fn distribute_grow(items: &mut [StackItem], mut free: f64) {
    for _ in 0..items.len() {
        let weight: f64 = items
            .iter()
            .filter(|i| i.grow > 0.0 && i.main < i.max_main)
            .map(|i| i.grow)
            .sum();
        if weight <= 0.0 || free <= 1e-9 {
            return;
        }
        let mut used = 0.0;
        for item in items
            .iter_mut()
            .filter(|i| i.grow > 0.0 && i.main < i.max_main)
        {
            let share = free * item.grow / weight;
            let next = (item.main + share).min(item.max_main);
            used += next - item.main;
            item.main = next;
        }
        free -= used;
    }
}

/// Shrinks growable items, then `shrink` items, toward their minimum.
/// Returns the overflow that could not be absorbed.
fn shrink_items(items: &mut [StackItem], mut deficit: f64) -> f64 {
    for pass in [true, false] {
        let candidates =
            |i: &StackItem| (if pass { i.grow > 0.0 } else { i.shrink }) && i.main > i.min_main;
        for _ in 0..items.len() {
            let room: f64 = items
                .iter()
                .filter(|i| candidates(i))
                .map(|i| i.main - i.min_main)
                .sum();
            if room <= 1e-9 || deficit <= 1e-9 {
                break;
            }
            let take = deficit.min(room);
            for item in items.iter_mut().filter(|i| candidates(i)) {
                let share = take * (item.main - item.min_main) / room;
                item.main -= share;
            }
            deficit -= take;
        }
    }
    deficit.max(0.0)
}

fn base_track_sizes(tracks: &[Track]) -> Vec<f64> {
    tracks
        .iter()
        .map(|track| match *track {
            Track::Fixed(size) => size,
            Track::Auto => 0.0,
            Track::Flex { min, .. } => min,
        })
        .collect()
}

fn span_extent(sizes: &[f64], start: usize, span: usize, gap: f64) -> f64 {
    sizes[start..start + span].iter().sum::<f64>() + gap * (span - 1) as f64
}

fn total(sizes: &[f64], gap: f64) -> f64 {
    sizes.iter().sum::<f64>() + gap * sizes.len().saturating_sub(1) as f64
}

fn starts(sizes: &[f64], gap: f64) -> Vec<f64> {
    let mut cursor = 0.0;
    sizes
        .iter()
        .map(|size| {
            let start = cursor;
            cursor += size + gap;
            start
        })
        .collect()
}

/// Grows auto tracks (else flex tracks) evenly so a spanning cell fits.
fn widen_span(
    sizes: &mut [f64],
    tracks: &[Track],
    start: usize,
    span: usize,
    gap: f64,
    needed: f64,
) {
    let have = span_extent(sizes, start, span, gap);
    if needed <= have {
        return;
    }
    let range = start..start + span;
    let mut targets: Vec<usize> = range
        .clone()
        .filter(|i| tracks[*i] == Track::Auto)
        .collect();
    if targets.is_empty() {
        targets = range
            .filter(|i| matches!(tracks[*i], Track::Flex { .. }))
            .collect();
    }
    if targets.is_empty() {
        return;
    }
    let extra = (needed - have) / targets.len() as f64;
    for index in targets {
        sizes[index] += extra;
    }
}

/// Assigns remaining budget to flex tracks by weight. Returns true when
/// even minimum sizes exceed the budget.
fn resolve_flex(sizes: &mut [f64], tracks: &[Track], gap: f64, budget: Option<f64>) -> bool {
    let Some(budget) = budget else {
        return false;
    };
    let used = total(sizes, gap);
    if used > budget + 1e-9 {
        return true;
    }
    let mut free = budget - used;
    for _ in 0..tracks.len() {
        let open: Vec<usize> = (0..tracks.len())
            .filter(|i| matches!(tracks[*i], Track::Flex { max, .. } if sizes[*i] < max))
            .collect();
        let weight: f64 = open
            .iter()
            .map(|i| match tracks[*i] {
                Track::Flex { weight, .. } => weight,
                _ => 0.0,
            })
            .sum();
        if weight <= 0.0 || free <= 1e-9 {
            break;
        }
        let mut consumed = 0.0;
        for index in open {
            let Track::Flex { weight: w, max, .. } = tracks[index] else {
                continue;
            };
            let next = (sizes[index] + free * w / weight).min(max);
            consumed += next - sizes[index];
            sizes[index] = next;
        }
        free -= consumed;
    }
    false
}
