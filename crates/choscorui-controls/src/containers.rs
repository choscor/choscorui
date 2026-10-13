use crate::style::{
    ControlKind, ControlSize, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState,
    fill_shape,
};
use choscorui_core::{
    DesignTokens, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRelation, SemanticRole, SemanticState, Size, Tree, TreeError,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContainerError {
    Empty,
    DuplicateNode(NodeId),
    MissingNode(NodeId),
    InvalidDescendant { node: NodeId, container: NodeId },
    InvalidGeometry,
    InvalidPosition,
    UnknownPage(NodeId),
    Tree(TreeError),
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the container has no pages"),
            Self::DuplicateNode(node) => write!(f, "node {node:?} is already in the container"),
            Self::MissingNode(node) => write!(f, "node {node:?} is not in the tree"),
            Self::InvalidDescendant { node, container } => write!(
                f,
                "node {node:?} is not a descendant of container {container:?}"
            ),
            Self::InvalidGeometry => f.write_str("container geometry is not finite or is negative"),
            Self::InvalidPosition => {
                f.write_str("the splitter position is outside its allowed range")
            }
            Self::UnknownPage(node) => write!(f, "node {node:?} is not a page of this container"),
            Self::Tree(error) => write!(f, "tree update failed: {error}"),
        }
    }
}

impl std::error::Error for ContainerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TreeError> for ContainerError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

/// The axis along which a split view places its panes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizableInput {
    /// A press on the handle starts a drag, a move resizes, and a release
    /// ends it.
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SemanticAction),
    /// Ends a drag and releases pointer capture. The position stays.
    Cancel,
}

/// The old name of [`ResizableInput`].
#[deprecated(since = "0.1.0", note = "renamed to `ResizableInput`")]
pub type SplitterInput = ResizableInput;

/// The old name of [`Resizable`].
#[deprecated(since = "0.1.0", note = "renamed to `Resizable`")]
pub type Splitter = Resizable;

/// Two resizable panels and the handle between them. `position` is the
/// first panel's width or height.
pub struct Resizable {
    label: String,
    orientation: SplitOrientation,
    first: NodeId,
    handle: NodeId,
    second: NodeId,
    bounds: Rect,
    minimum_first: f64,
    minimum_second: f64,
    handle_extent: f64,
    step: f64,
    position: f64,
    grip: bool,
    /// The pointer's offset from the handle start during a drag.
    drag: Option<f64>,
}

impl Resizable {
    #[expect(
        clippy::too_many_arguments,
        reason = "public constructor: each pane, the handle, and the geometry are separate arguments"
    )]
    pub fn new(
        label: impl Into<String>,
        orientation: SplitOrientation,
        first: NodeId,
        handle: NodeId,
        second: NodeId,
        bounds: Rect,
        minimum_first: f64,
        minimum_second: f64,
        handle_extent: f64,
        position: f64,
    ) -> Result<Self, ContainerError> {
        validate_unique([first, handle, second])?;
        let primary = primary_size(bounds, orientation);
        if !valid_rect(bounds)
            || !valid_number(minimum_first)
            || !valid_number(minimum_second)
            || !valid_number(handle_extent)
            || handle_extent <= 0.0
            || primary < minimum_first + handle_extent + minimum_second
        {
            return Err(ContainerError::InvalidGeometry);
        }
        let maximum = primary - handle_extent - minimum_second;
        if !position.is_finite() || position < minimum_first || position > maximum {
            return Err(ContainerError::InvalidPosition);
        }
        Ok(Self {
            label: label.into(),
            orientation,
            first,
            handle,
            second,
            bounds,
            minimum_first,
            minimum_second,
            handle_extent,
            step: 10.0,
            position,
            grip: false,
            drag: None,
        })
    }

    /// Shows a grip on the handle.
    #[must_use]
    pub fn with_grip(mut self, grip: bool) -> Self {
        self.grip = grip;
        self
    }

    pub const fn orientation(&self) -> SplitOrientation {
        self.orientation
    }

    /// The handle node, which takes focus and paints the divider.
    pub const fn handle_node(&self) -> NodeId {
        self.handle
    }

    pub fn install(&self, tree: &mut Tree) -> Result<(), ContainerError> {
        validate_attached(tree, [self.first, self.handle, self.second])?;
        tree.set_focusable(self.handle, true)?;
        self.arrange(tree, self.position)
    }

    pub const fn position(&self) -> f64 {
        self.position
    }

    pub fn set_position(&mut self, tree: &mut Tree, position: f64) -> Result<bool, ContainerError> {
        validate_attached(tree, [self.first, self.handle, self.second])?;
        if !position.is_finite() || position < self.minimum_first || position > self.maximum() {
            return Err(ContainerError::InvalidPosition);
        }
        if position == self.position {
            return Ok(false);
        }
        self.arrange(tree, position)?;
        self.position = position;
        Ok(true)
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        input: ResizableInput,
    ) -> Result<Option<f64>, ContainerError> {
        let target = match input {
            ResizableInput::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if tree.hit_test(position) == Some(self.handle) => {
                tree.focus_by_pointer(self.handle)?;
                tree.set_pointer_capture(Some(self.handle))?;
                self.drag = Some(self.along(position) - self.position);
                return Ok(None);
            }
            ResizableInput::Pointer(PointerEvent::Move { position }) => {
                let Some(offset) = self
                    .drag
                    .filter(|_| tree.pointer_capture() == Some(self.handle))
                else {
                    return Ok(None);
                };
                self.along(position) - offset
            }
            ResizableInput::Pointer(PointerEvent::Up {
                button: PointerButton::Primary,
                ..
            })
            | ResizableInput::Cancel => {
                self.drag = None;
                if tree.pointer_capture() == Some(self.handle) {
                    tree.set_pointer_capture(None)?;
                }
                return Ok(None);
            }
            ResizableInput::Semantic(SemanticAction::Focus) => {
                validate_attached(tree, [self.handle])?;
                tree.set_focus(Some(self.handle))?;
                return Ok(None);
            }
            ResizableInput::Semantic(SemanticAction::Increment) => self.position + self.step,
            ResizableInput::Semantic(SemanticAction::Decrement) => self.position - self.step,
            ResizableInput::Key(KeyEvent::Down { key, .. })
                if tree.focus() == Some(self.handle) =>
            {
                match (self.orientation, key) {
                    (_, Key::Home) => self.minimum_first,
                    (_, Key::End) => self.maximum(),
                    (SplitOrientation::Horizontal, Key::ArrowLeft)
                    | (SplitOrientation::Vertical, Key::ArrowUp) => self.position - self.step,
                    (SplitOrientation::Horizontal, Key::ArrowRight)
                    | (SplitOrientation::Vertical, Key::ArrowDown) => self.position + self.step,
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        let target = target.clamp(self.minimum_first, self.maximum());
        Ok(self.set_position(tree, target)?.then_some(target))
    }

    pub fn semantics(&self, _tree: &Tree) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: self.label.clone(),
            description: String::new(),
            value: Some(format_number(self.position)),
            state: SemanticState::default(),
            relations: vec![
                SemanticRelation::Controls(self.first),
                SemanticRelation::Controls(self.second),
            ],
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
            text: None,
        }
    }

    /// The flags the resizable recipe reads for the handle.
    pub fn visual_state(&self, tree: &Tree) -> VisualState {
        VisualState::NONE
            .set(VisualState::FOCUSED, tree.focus() == Some(self.handle))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(self.handle))
    }

    /// Paints the 1 pt divider centered in the handle bounds, across the
    /// split, and the grip when it has one.
    pub fn paint_handle(&self, cx: &mut PaintContext<'_>, tree: &Tree) {
        let bounds = cx.bounds;
        let state = self.visual_state(tree);
        paint_divider(cx, bounds, self.orientation, self.grip, state);
    }

    fn maximum(&self) -> f64 {
        primary_size(self.bounds, self.orientation) - self.handle_extent - self.minimum_second
    }

    /// The distance of `point` from the start of the bounds along the split.
    fn along(&self, point: Point) -> f64 {
        match self.orientation {
            SplitOrientation::Horizontal => point.x - self.bounds.origin.x,
            SplitOrientation::Vertical => point.y - self.bounds.origin.y,
        }
    }

    fn arrange(&self, tree: &mut Tree, position: f64) -> Result<(), ContainerError> {
        let (first, handle, second) =
            split_rects(self.bounds, self.orientation, position, self.handle_extent);
        tree.set_bounds(self.first, first)?;
        tree.set_bounds(self.handle, handle)?;
        tree.set_bounds(self.second, second)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupBox {
    label: String,
    container: NodeId,
    title: Option<NodeId>,
    members: Vec<NodeId>,
}

impl GroupBox {
    pub fn new(
        label: impl Into<String>,
        container: NodeId,
        title: Option<NodeId>,
        members: Vec<NodeId>,
    ) -> Result<Self, ContainerError> {
        if members.is_empty() {
            return Err(ContainerError::Empty);
        }
        validate_unique(
            std::iter::once(container)
                .chain(title)
                .chain(members.iter().copied()),
        )?;
        Ok(Self {
            label: label.into(),
            container,
            title,
            members,
        })
    }

    pub fn install(&self, tree: &Tree) -> Result<(), ContainerError> {
        validate_attached(
            tree,
            std::iter::once(self.container)
                .chain(self.title)
                .chain(self.members.iter().copied()),
        )?;
        for member in &self.members {
            validate_descendant(tree, *member, self.container)?;
        }
        if let Some(title) = self.title {
            validate_descendant(tree, title, self.container)?;
        }
        Ok(())
    }

    pub fn semantics(&self) -> SemanticNode {
        let mut relations: Vec<_> = self
            .members
            .iter()
            .copied()
            .map(SemanticRelation::Controls)
            .collect();
        if let Some(title) = self.title {
            relations.insert(0, SemanticRelation::LabelledBy(title));
        }
        SemanticNode {
            role: SemanticRole::Group,
            name: self.label.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations,
            actions: Vec::new(),
            text: None,
        }
    }

    /// The members' area in `bounds`: under the title, inside the small
    /// `Card` padding.
    pub fn content_rect(&self, bounds: Rect, styles: &Styles, tokens: &DesignTokens) -> Rect {
        group_content(bounds, styles, tokens)
    }

    /// Paints the small `Card` surface and the title in the context's
    /// bounds. The host paints the members.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        paint_group(cx, &self.label);
    }

    /// A group box for a control host, with no member nodes: the hosted
    /// control paints the card and the title, and the view layer places
    /// the members in [`HostedGroupBox::content_rect`].
    pub fn hosted(label: impl Into<String>) -> HostedGroupBox {
        HostedGroupBox {
            label: label.into(),
            content: Rect::default(),
        }
    }
}

/// The members' area in `bounds`: under the title, inside the small
/// `Card` padding.
fn group_content(bounds: Rect, styles: &Styles, tokens: &DesignTokens) -> Rect {
    let (inline_start, top, chrome) = group_chrome(styles, tokens);
    Rect::from_xywh(
        bounds.origin.x + inline_start,
        bounds.origin.y + top,
        (bounds.size.width - chrome.width).max(0.0),
        (bounds.size.height - chrome.height).max(0.0),
    )
}

/// The inline start inset, the top inset, and the total chrome size of a
/// group box: the small `Card` padding and the title line.
fn group_chrome(styles: &Styles, tokens: &DesignTokens) -> (f64, f64, Size) {
    let card = |slot| {
        styles.resolve(
            tokens,
            StyleQuery::new(ControlKind::Card, slot).size(ControlSize::Sm),
        )
    };
    let (root, header, title) = (card(Slot::Root), card(Slot::Header), card(Slot::Title));
    let top = root.padding.block_start + title.text.line_height + root.gap;
    (
        header.padding.inline_start,
        top,
        Size::new(header.padding.horizontal(), top + root.padding.block_end),
    )
}

/// Paints the small `Card` surface and `label` in the context's bounds.
fn paint_group(cx: &mut PaintContext<'_>, label: &str) {
    let card = |slot| StyleQuery::new(ControlKind::Card, slot).size(ControlSize::Sm);
    let (root, header, title) = (
        cx.style(card(Slot::Root)),
        cx.style(card(Slot::Header)),
        cx.style(card(Slot::Title)),
    );
    let bounds = cx.bounds;
    cx.slot(bounds, &root, |_, _| {});
    let line = Rect::from_xywh(
        bounds.origin.x + header.padding.inline_start,
        bounds.origin.y + root.padding.block_start,
        (bounds.size.width - header.padding.horizontal()).max(0.0),
        title.text.line_height,
    );
    cx.label(line, label, &title, TextAlign::Start);
}

/// The size a hosted container takes: the offered size on each bounded
/// axis, and `natural` on an unbounded one.
fn offered(cx: &crate::MeasureContext<'_>, natural: Size) -> choscorui_core::Measured {
    let max = cx.constraints.max;
    let size = Size::new(
        if max.width.is_finite() {
            max.width
        } else {
            natural.width
        },
        if max.height.is_finite() {
            max.height
        } else {
            natural.height
        },
    );
    choscorui_core::Measured::new(cx.constraints.constrain(size))
}

/// A [`GroupBox`] mounted in a control host. It paints the card and the
/// title, takes no input, and reports the members' area, from its last
/// arranged bounds, in [`content_rect`](Self::content_rect).
#[derive(Clone, Debug, PartialEq)]
pub struct HostedGroupBox {
    label: String,
    content: Rect,
}

impl HostedGroupBox {
    /// The members' area from the last arrange: under the title, inside
    /// the small `Card` padding.
    pub fn content_rect(&self) -> Rect {
        self.content
    }
}

impl crate::CustomControl for HostedGroupBox {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let (_, _, chrome) = group_chrome(cx.styles, cx.tokens);
        offered(cx, chrome)
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, bounds: Rect) {
        self.content = group_content(bounds, cx.styles, cx.tokens);
    }

    fn event(
        &mut self,
        _cx: &mut crate::ControlContext<'_>,
        _event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        choscorui_core::EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        paint_group(cx, &self.label);
    }

    fn semantics(&self) -> SemanticNode {
        crate::hosted::node(SemanticRole::Group, &self.label, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl Resizable {
    /// A resizable for a control host, with no pane nodes. The hosted node
    /// covers both panes and the handle: it paints the handle and takes
    /// the pointer and the keys, and the view layer places the panes in
    /// [`HostedResizable::panes`]. Fails like [`Resizable::new`] for
    /// geometry that is not finite, a handle that is not positive, or a
    /// position below `minimum_first`.
    pub fn hosted(
        label: impl Into<String>,
        orientation: SplitOrientation,
        minimum_first: f64,
        minimum_second: f64,
        handle_extent: f64,
        position: f64,
    ) -> Result<HostedResizable, ContainerError> {
        Self::hosted_group(
            label,
            orientation,
            vec![minimum_first, minimum_second],
            handle_extent,
            vec![position],
        )
    }

    /// A panel group for a control host: one pane for each value in
    /// `minimums`, and a handle between each two neighbors. `sizes` gives
    /// the size of each pane but the last, which takes the rest. Fails
    /// with `InvalidGeometry` for fewer than two panes, a `sizes` count
    /// that is not one less than the pane count, a minimum that is not
    /// finite or is negative, or a handle that is not positive. Fails with
    /// `InvalidPosition` for a size that is not finite or is below its
    /// pane minimum.
    pub fn hosted_group(
        label: impl Into<String>,
        orientation: SplitOrientation,
        minimums: Vec<f64>,
        handle_extent: f64,
        sizes: Vec<f64>,
    ) -> Result<HostedResizable, ContainerError> {
        if minimums.len() < 2
            || sizes.len() + 1 != minimums.len()
            || !minimums.iter().all(|minimum| valid_number(*minimum))
            || !valid_number(handle_extent)
            || handle_extent <= 0.0
        {
            return Err(ContainerError::InvalidGeometry);
        }
        if sizes
            .iter()
            .zip(&minimums)
            .any(|(size, minimum)| !size.is_finite() || size < minimum)
        {
            return Err(ContainerError::InvalidPosition);
        }
        Ok(HostedResizable {
            label: label.into(),
            orientation,
            minimums,
            handle_extent,
            hit_extent: handle_extent,
            step: 10.0,
            sizes,
            grip: false,
            bounds: None,
            rtl: false,
            active: 0,
            drag: None,
            handle_nodes: false,
        })
    }
}

/// A [`Resizable`] mounted in a control host. The node covers the panes
/// and the handles. A primary press on a handle starts a drag, a move
/// resizes the two panes next to that handle, and a release ends the drag.
/// The arrow keys along the split axis, Home, End, and the `Increment` and
/// `Decrement` actions move the active handle. Each change reports
/// `Changed(Number(size))` with the size of the pane before the handle.
/// After a change, [`active`](Self::active) names the handle that moved,
/// and [`sizes`](Self::sizes) gives every pane.
/// In a right-to-left layout a horizontal split puts the first pane on
/// the right and mirrors the arrow keys and the drag.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedResizable {
    label: String,
    orientation: SplitOrientation,
    /// The minimum of each pane, in order. There are two or more.
    minimums: Vec<f64>,
    handle_extent: f64,
    /// The extent along the split that a press hits: the handle, widened
    /// to the recipe's hit area and the grip.
    hit_extent: f64,
    step: f64,
    /// The size of each pane but the last, which takes the rest.
    sizes: Vec<f64>,
    grip: bool,
    /// The bounds from the last arrange.
    bounds: Option<Rect>,
    rtl: bool,
    /// The handle that the keys and actions move.
    active: usize,
    /// The dragged handle and the pointer's offset from its start.
    drag: Option<(usize, f64)>,
    /// Each handle is its own node, driven through the `handle_*`
    /// methods; the group node is a plain group.
    handle_nodes: bool,
}

impl HostedResizable {
    /// Shows a grip on each handle.
    #[must_use]
    pub fn with_grip(mut self, grip: bool) -> Self {
        self.grip = grip;
        self
    }

    /// Puts each handle on its own node. The group node then has the
    /// `Group` role, takes no focus, paints nothing, and ignores input.
    /// The view layer mounts one node for each handle, places it in its
    /// handle rectangle, and forwards the node's input, paint, semantics,
    /// and actions to [`handle_event`](Self::handle_event),
    /// [`paint_handle`](Self::paint_handle),
    /// [`handle_semantics`](Self::handle_semantics), and
    /// [`handle_perform`](Self::handle_perform). Each handle is then its
    /// own stop in the focus order.
    #[must_use]
    pub fn with_handle_nodes(mut self, handle_nodes: bool) -> Self {
        self.handle_nodes = handle_nodes;
        self
    }

    /// The number of handles: one less than the number of panes.
    pub fn handle_count(&self) -> usize {
        self.minimums.len() - 1
    }

    /// The first pane's width or height, within the pane minimums of the
    /// last arranged bounds. Bounds too small for the position keep it for
    /// when they grow again.
    pub fn position(&self) -> f64 {
        self.layout()[0]
    }

    /// The width or height of each pane, within the pane minimums of the
    /// last arranged bounds.
    pub fn sizes(&self) -> Vec<f64> {
        self.layout()
    }

    /// The handle that the keys and actions move, from 0. A press on a
    /// handle makes it active, so after a change it is the handle that
    /// moved.
    pub fn active(&self) -> usize {
        self.active
    }

    /// The first pane, the first handle, and the second pane in the last
    /// arranged bounds. A right-to-left horizontal split mirrors them.
    pub fn panes(&self) -> (Rect, Rect, Rect) {
        let (panes, handles) = self.rects();
        (panes[0], handles[0], panes[1])
    }

    /// Each pane rectangle and each handle rectangle in the last arranged
    /// bounds, in pane order. A right-to-left horizontal split mirrors
    /// them.
    pub fn rects(&self) -> (Vec<Rect>, Vec<Rect>) {
        let count = self.minimums.len();
        match self.bounds {
            Some(bounds) => self.rects_in(bounds, self.rtl),
            None => (
                vec![Rect::default(); count],
                vec![Rect::default(); count - 1],
            ),
        }
    }

    /// Each pane rectangle and each handle rectangle in `bounds`, in pane
    /// order, for the stored sizes. `rtl` mirrors a horizontal split. A
    /// view layer uses it to place pane views before the control arranges.
    pub fn rects_in(&self, bounds: Rect, rtl: bool) -> (Vec<Rect>, Vec<Rect>) {
        let count = self.minimums.len();
        let mut panes = Vec::with_capacity(count);
        let mut handles = Vec::with_capacity(count - 1);
        let mut start = 0.0;
        for (index, size) in self.layout_in(Some(bounds)).into_iter().enumerate() {
            panes.push(self.span(bounds, rtl, start, size));
            start += size;
            if index + 1 < count {
                handles.push(self.span(bounds, rtl, start, self.handle_extent));
                start += self.handle_extent;
            }
        }
        (panes, handles)
    }

    /// Gives each pane but the last the size in `sizes`, as
    /// [`Resizable::hosted_group`] takes them. Returns false and changes
    /// nothing for a wrong count, or a size that is not finite or is below
    /// its pane minimum. The layout keeps the sizes within the bounds.
    pub fn set_sizes(&mut self, sizes: &[f64]) -> bool {
        if sizes.len() != self.sizes.len()
            || sizes
                .iter()
                .zip(&self.minimums)
                .any(|(size, minimum)| !size.is_finite() || size < minimum)
        {
            return false;
        }
        self.sizes = sizes.to_vec();
        true
    }

    /// The smallest extent along the split: each pane minimum and each
    /// handle.
    pub fn minimum_extent(&self) -> f64 {
        self.minimums.iter().sum::<f64>() + self.handle_extent * (self.minimums.len() - 1) as f64
    }

    /// The size of each pane: each stored size kept between its minimum
    /// and the space that the later pane minimums leave. The last pane
    /// takes the rest. Before the first arrange the space has no limit.
    fn layout(&self) -> Vec<f64> {
        self.layout_in(self.bounds)
    }

    /// The size of each pane in `bounds`, as [`layout`](Self::layout)
    /// gives them. Without bounds the space has no limit.
    fn layout_in(&self, bounds: Option<Rect>) -> Vec<f64> {
        let count = self.minimums.len();
        let mut free = bounds.map_or(f64::INFINITY, |bounds| {
            primary_size(bounds, self.orientation) - self.handle_extent * (count - 1) as f64
        });
        let mut reserved: f64 = self.minimums.iter().sum();
        let mut sizes = Vec::with_capacity(count);
        for (index, minimum) in self.minimums.iter().enumerate() {
            reserved -= minimum;
            let size = match self.sizes.get(index) {
                Some(size) => size.min(free - reserved).max(*minimum),
                None => free.max(0.0),
            };
            free -= size;
            sizes.push(size);
        }
        sizes
    }

    /// The rectangle `extent` long that starts `start` from the start edge
    /// of `bounds` along the split. `rtl` mirrors a horizontal split.
    fn span(&self, bounds: Rect, rtl: bool, start: f64, extent: f64) -> Rect {
        match self.orientation {
            SplitOrientation::Horizontal => {
                let x = if rtl {
                    bounds.max_x() - start - extent
                } else {
                    bounds.origin.x + start
                };
                Rect::from_xywh(x, bounds.origin.y, extent, bounds.size.height)
            }
            SplitOrientation::Vertical => Rect::from_xywh(
                bounds.origin.x,
                bounds.origin.y + start,
                bounds.size.width,
                extent,
            ),
        }
    }

    /// The distance of `point` from the start edge along the split. A
    /// right-to-left horizontal split measures from the right.
    fn along(&self, bounds: Rect, point: Point) -> f64 {
        match self.orientation {
            SplitOrientation::Horizontal if self.rtl => bounds.max_x() - point.x,
            SplitOrientation::Horizontal => point.x - bounds.origin.x,
            SplitOrientation::Vertical => point.y - bounds.origin.y,
        }
    }

    /// The start of pane `index` along the split, from `sizes`.
    fn pane_start(&self, sizes: &[f64], index: usize) -> f64 {
        sizes[..index].iter().sum::<f64>() + self.handle_extent * index as f64
    }

    /// The handle whose hit area contains `point`.
    fn handle_at(&self, bounds: Rect, point: Point) -> Option<usize> {
        if !bounds.contains(point) {
            return None;
        }
        let along = self.along(bounds, point);
        let sizes = self.layout();
        (0..self.minimums.len() - 1).find(|&index| {
            let center = self.pane_start(&sizes, index + 1) - self.handle_extent / 2.0;
            (along - center).abs() <= self.hit_extent / 2.0
        })
    }

    /// The range of the pane before handle `index`, from `sizes`: its
    /// minimum, and the pair's total less the next pane's minimum.
    fn range(&self, sizes: &[f64], index: usize) -> (f64, f64) {
        let minimum = self.minimums[index];
        let pair = sizes[index] + sizes[index + 1];
        (minimum, (pair - self.minimums[index + 1]).max(minimum))
    }

    /// Stores `size` for the pane before handle `index`, and gives the
    /// rest of the pair to the pane after it.
    fn store(&mut self, sizes: &[f64], index: usize, size: f64) {
        let pair = sizes[index] + sizes[index + 1];
        self.sizes[index] = size;
        if let Some(next) = self.sizes.get_mut(index + 1) {
            *next = pair - size;
        }
    }

    /// Moves handle `index` so that the pane before it is `target` long,
    /// clamped, and reports a change.
    fn move_to(&mut self, cx: &mut crate::ControlContext<'_>, index: usize, target: f64) {
        let sizes = self.layout();
        let (minimum, maximum) = self.range(&sizes, index);
        let target = target.min(maximum).max(minimum);
        if target == sizes[index] {
            return;
        }
        self.active = index;
        self.store(&sizes, index, target);
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
        cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
            target,
        )));
    }

    /// Ends a drag and releases pointer capture.
    fn end_drag(&mut self, cx: &mut crate::ControlContext<'_>) {
        if self.drag.take().is_some() {
            cx.capture_pointer(false);
            cx.request_paint();
        }
    }

    fn pointer(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: PointerEvent,
    ) -> choscorui_core::EventReply {
        let bounds = cx.bounds();
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => match self.handle_at(bounds, position) {
                Some(index) => self.press(cx, bounds, index, position),
                None => choscorui_core::EventReply::Continue,
            },
            event => self.drag_event(cx, bounds, event),
        }
    }

    /// Starts a drag of handle `index` from a press at `position`, in the
    /// group `bounds`, and makes the handle active.
    fn press(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        bounds: Rect,
        index: usize,
        position: Point,
    ) -> choscorui_core::EventReply {
        let start = self.pane_start(&self.layout(), index + 1) - self.handle_extent;
        self.active = index;
        self.drag = Some((index, self.along(bounds, position) - start));
        cx.capture_pointer(true);
        cx.request_paint();
        cx.request_semantics();
        choscorui_core::EventReply::Stop
    }

    /// Moves the dragged handle, or ends the drag on a primary release,
    /// in the group `bounds`.
    fn drag_event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        bounds: Rect,
        event: PointerEvent,
    ) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        match event {
            PointerEvent::Move { position } => {
                let Some((index, offset)) = self.drag else {
                    return EventReply::Continue;
                };
                let start = self.pane_start(&self.layout(), index);
                let target = self.along(bounds, position) - offset - start;
                self.move_to(cx, index, target);
                EventReply::Stop
            }
            PointerEvent::Up {
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => {
                self.end_drag(cx);
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn key(&mut self, cx: &mut crate::ControlContext<'_>, key: Key) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        let shift = cx.modifiers().shift;
        let last = self.minimums.len() - 2;
        match key {
            Key::Tab if !shift && self.active < last => {
                self.active += 1;
                cx.request_paint();
                cx.request_semantics();
                return EventReply::Stop;
            }
            Key::Tab if shift && self.active > 0 => {
                self.active -= 1;
                cx.request_paint();
                cx.request_semantics();
                return EventReply::Stop;
            }
            _ => {}
        }
        self.step_key(cx, self.active, key)
    }

    /// Moves handle `index` for an arrow key along the split, Home, or
    /// End. Other keys continue.
    fn step_key(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        index: usize,
        key: Key,
    ) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        let (back, forward) = match (self.orientation, self.rtl) {
            (SplitOrientation::Horizontal, false) => (Key::ArrowLeft, Key::ArrowRight),
            (SplitOrientation::Horizontal, true) => (Key::ArrowRight, Key::ArrowLeft),
            (SplitOrientation::Vertical, _) => (Key::ArrowUp, Key::ArrowDown),
        };
        let current = self.layout()[index];
        let target = match key {
            Key::Home => self.minimums[index],
            Key::End => f64::INFINITY,
            key if key == back => current - self.step,
            key if key == forward => current + self.step,
            _ => return EventReply::Continue,
        };
        self.move_to(cx, index, target);
        EventReply::Stop
    }
}

/// The handle nodes of a group made with
/// [`with_handle_nodes`](HostedResizable::with_handle_nodes). Each method
/// takes the handle's index, from 0, and does nothing for an index past
/// the last handle or for a group without handle nodes.
impl HostedResizable {
    /// Handles an event on the node of handle `index`. A primary press
    /// starts a drag of the handle, a move resizes its two panes, and a
    /// release ends the drag. The arrow keys along the split, Home, and
    /// End move the handle. Focus makes it the active handle. The drag
    /// uses the group bounds of the last arrange.
    pub fn handle_event(
        &mut self,
        index: usize,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        if !self.handle_nodes || index >= self.handle_count() {
            return EventReply::Continue;
        }
        if let crate::ControlEvent::FocusChanged(true) = event {
            self.active = index;
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Continue;
        }
        let Some(bounds) = self.bounds else {
            return EventReply::Continue;
        };
        let dragged = self.drag.is_some_and(|(dragged, _)| dragged == index);
        match crate::hosted::route(cx, event) {
            Some(crate::hosted::Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            })) => self.press(cx, bounds, index, position),
            Some(crate::hosted::Routed::Pointer(event)) if dragged => {
                self.drag_event(cx, bounds, event)
            }
            Some(crate::hosted::Routed::Key(KeyEvent::Down { key, .. })) => {
                self.step_key(cx, index, key)
            }
            Some(crate::hosted::Routed::Cancel) if dragged => {
                self.end_drag(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints handle `index` in `cx.bounds`, the bounds of its node: the
    /// divider and the grip, with the node's focus state, pressed while
    /// the handle is dragged.
    pub fn paint_handle(&self, index: usize, cx: &mut PaintContext<'_>) {
        if !self.handle_nodes || index >= self.handle_count() {
            return;
        }
        let state = cx.state.set(
            VisualState::PRESSED,
            self.drag.is_some_and(|(dragged, _)| dragged == index),
        );
        let bounds = cx.bounds;
        paint_divider(cx, bounds, self.orientation, self.grip, state);
    }

    /// The semantics of handle `index`: a `Slider` with the group label,
    /// the size of the pane before the handle as the value, and the range
    /// of that size as the description, for example `50–150`.
    pub fn handle_semantics(&self, index: usize) -> SemanticNode {
        let mut node = crate::hosted::node(
            SemanticRole::Slider,
            &self.label,
            vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
        );
        if index < self.handle_count() {
            let sizes = self.layout();
            node.value = Some(format_number(sizes[index]));
            let (minimum, maximum) = self.range(&sizes, index);
            if maximum.is_finite() {
                node.description = format!("{}–{}", format_number(minimum), format_number(maximum));
            }
        }
        node
    }

    /// Performs `Increment`, `Decrement`, or `Focus` on handle `index`.
    /// Returns false for other actions.
    pub fn handle_perform(
        &mut self,
        index: usize,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
    ) -> bool {
        if !self.handle_nodes || index >= self.handle_count() {
            return false;
        }
        let current = self.layout()[index];
        match action {
            SemanticAction::Increment => self.move_to(cx, index, current + self.step),
            SemanticAction::Decrement => self.move_to(cx, index, current - self.step),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }
}

impl crate::CustomControl for HostedResizable {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let along = self.minimum_extent();
        let natural = match self.orientation {
            SplitOrientation::Horizontal => Size::new(along, 0.0),
            SplitOrientation::Vertical => Size::new(0.0, along),
        };
        offered(cx, natural)
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, bounds: Rect) {
        self.bounds = Some(bounds);
        self.rtl = cx.rtl();
        // The recipe padding gives the hit area around the line. The grip
        // also hits.
        let line = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Handle));
        let mut hit = self
            .handle_extent
            .max(line.padding.horizontal())
            .max(line.min_width);
        if self.grip {
            let grip = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Thumb));
            hit = hit.max(grip.min_width);
        }
        self.hit_extent = hit;
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        if self.handle_nodes {
            return EventReply::Continue;
        }
        if let crate::ControlEvent::FocusChanged(true) = event {
            // Shift+Tab arrives at the last handle, and Tab at the first.
            if self.drag.is_none() {
                self.active = if cx.modifiers().shift {
                    self.minimums.len() - 2
                } else {
                    0
                };
            }
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Continue;
        }
        match crate::hosted::route(cx, event) {
            Some(crate::hosted::Routed::Pointer(event)) => self.pointer(cx, event),
            Some(crate::hosted::Routed::Key(KeyEvent::Down { key, .. })) => self.key(cx, key),
            Some(crate::hosted::Routed::Cancel) => {
                self.end_drag(cx);
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        if self.handle_nodes {
            return;
        }
        let (_, handles) = self.rects();
        let host = cx.state;
        for (index, handle) in handles.into_iter().enumerate() {
            let state = host
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && index == self.active,
                )
                .set(
                    VisualState::PRESSED,
                    self.drag.is_some_and(|(dragged, _)| dragged == index),
                );
            paint_divider(cx, handle, self.orientation, self.grip, state);
        }
    }

    fn semantics(&self) -> SemanticNode {
        if self.handle_nodes {
            return crate::hosted::node(SemanticRole::Group, &self.label, Vec::new());
        }
        let mut node = crate::hosted::node(
            SemanticRole::Slider,
            &self.label,
            vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
        );
        node.value = Some(format_number(self.layout()[self.active]));
        node
    }

    fn focusable(&self) -> bool {
        !self.handle_nodes
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if self.handle_nodes {
            return false;
        }
        let index = self.active;
        let current = self.layout()[index];
        match action {
            SemanticAction::Increment => self.move_to(cx, index, current + self.step),
            SemanticAction::Decrement => self.move_to(cx, index, current - self.step),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        // The host releases the pointer capture of a disabled control.
        if !enabled {
            self.drag = None;
        }
    }

    /// Takes `Number`: the size of the pane before the active handle.
    /// Rejects a size that is not finite or is outside the handle's range.
    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        let crate::ControlValue::Number(size) = *value else {
            return false;
        };
        let sizes = self.layout();
        let (minimum, maximum) = self.range(&sizes, self.active);
        if !size.is_finite() || size < minimum || size > maximum {
            return false;
        }
        self.store(&sizes, self.active, size);
        true
    }
}

impl StackedContainer {
    /// A stack for a control host, with `pages` pages and no page nodes.
    /// The hosted control paints nothing and takes no input; the view
    /// layer shows page [`HostedStack::current`] in the stack's bounds.
    /// Fails with `Empty` for no pages and `InvalidPosition` for a
    /// current page past the end.
    pub fn hosted(
        label: impl Into<String>,
        pages: usize,
        current: usize,
    ) -> Result<HostedStack, ContainerError> {
        if pages == 0 {
            return Err(ContainerError::Empty);
        }
        if current >= pages {
            return Err(ContainerError::InvalidPosition);
        }
        Ok(HostedStack {
            label: label.into(),
            pages,
            current,
        })
    }
}

/// A [`StackedContainer`] mounted in a control host. The application
/// selects the page with `set_value(Index(Some(page)))`.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedStack {
    label: String,
    pages: usize,
    current: usize,
}

impl HostedStack {
    /// The page shown, from 0.
    pub fn current(&self) -> usize {
        self.current
    }
}

impl crate::CustomControl for HostedStack {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        offered(cx, Size::new(0.0, 0.0))
    }

    fn event(
        &mut self,
        _cx: &mut crate::ControlContext<'_>,
        _event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        choscorui_core::EventReply::Continue
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn semantics(&self) -> SemanticNode {
        let mut node = crate::hosted::node(SemanticRole::Group, &self.label, Vec::new());
        node.value = Some((self.current + 1).to_string());
        node
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match *value {
            crate::ControlValue::Index(Some(page)) if page < self.pages => {
                self.current = page;
                true
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackPage {
    pub panel: NodeId,
    pub initial_focus: Option<NodeId>,
}

impl StackPage {
    pub const fn new(panel: NodeId, initial_focus: Option<NodeId>) -> Self {
        Self {
            panel,
            initial_focus,
        }
    }
}

pub struct StackedContainer {
    label: String,
    container: NodeId,
    pages: Vec<StackPage>,
    current: NodeId,
    remembered_focus: HashMap<NodeId, NodeId>,
}

impl StackedContainer {
    pub fn new(
        label: impl Into<String>,
        container: NodeId,
        pages: Vec<StackPage>,
        current: NodeId,
    ) -> Result<Self, ContainerError> {
        if pages.is_empty() {
            return Err(ContainerError::Empty);
        }
        validate_unique(pages.iter().map(|page| page.panel))?;
        if !pages.iter().any(|page| page.panel == current) {
            return Err(ContainerError::UnknownPage(current));
        }
        Ok(Self {
            label: label.into(),
            container,
            pages,
            current,
            remembered_focus: HashMap::new(),
        })
    }

    pub const fn current(&self) -> NodeId {
        self.current
    }

    pub fn install(&self, tree: &mut Tree) -> Result<(), ContainerError> {
        self.validate(tree)?;
        for page in &self.pages {
            tree.set_visible(page.panel, page.panel == self.current)?;
        }
        Ok(())
    }

    pub fn select(&mut self, tree: &mut Tree, panel: NodeId) -> Result<bool, ContainerError> {
        let target = self
            .pages
            .iter()
            .position(|page| page.panel == panel)
            .ok_or(ContainerError::UnknownPage(panel))?;
        self.validate(tree)?;
        if panel == self.current {
            return Ok(false);
        }
        let old = self.current;
        let old_focus = tree
            .focus()
            .filter(|focus| is_descendant(tree, *focus, old));
        let restored = self
            .remembered_focus
            .get(&panel)
            .copied()
            .or(self.pages[target].initial_focus);
        if let Some(focus) = restored {
            validate_descendant(tree, focus, panel)?;
        }
        if let Some(focus) = old_focus {
            self.remembered_focus.insert(old, focus);
        }
        tree.set_visible(old, false)?;
        tree.set_visible(panel, true)?;
        self.current = panel;
        if old_focus.is_some() {
            tree.set_focus(restored)?;
        }
        Ok(true)
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Group,
            name: self.label.clone(),
            description: String::new(),
            value: Some(
                (self
                    .pages
                    .iter()
                    .position(|page| page.panel == self.current)
                    .expect("current page is retained")
                    + 1)
                .to_string(),
            ),
            state: SemanticState::default(),
            relations: self
                .pages
                .iter()
                .map(|page| SemanticRelation::Controls(page.panel))
                .collect(),
            actions: Vec::new(),
            text: None,
        }
    }

    pub fn page_semantics(&self, panel: NodeId) -> Option<SemanticNode> {
        self.pages.iter().find(|page| page.panel == panel)?;
        Some(SemanticNode {
            role: SemanticRole::Group,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState {
                hidden: panel != self.current,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        })
    }

    fn validate(&self, tree: &Tree) -> Result<(), ContainerError> {
        validate_attached(
            tree,
            std::iter::once(self.container).chain(
                self.pages
                    .iter()
                    .flat_map(|page| std::iter::once(page.panel).chain(page.initial_focus)),
            ),
        )?;
        for page in &self.pages {
            validate_descendant(tree, page.panel, self.container)?;
            if let Some(focus) = page.initial_focus {
                validate_descendant(tree, focus, page.panel)?;
            }
        }
        for (&panel, &focus) in &self.remembered_focus {
            validate_descendant(tree, focus, panel)?;
        }
        Ok(())
    }
}

fn split_rects(
    bounds: Rect,
    orientation: SplitOrientation,
    position: f64,
    handle_extent: f64,
) -> (Rect, Rect, Rect) {
    match orientation {
        SplitOrientation::Horizontal => (
            Rect::new(bounds.origin, Size::new(position, bounds.size.height)),
            Rect::new(
                Point::new(bounds.origin.x + position, bounds.origin.y),
                Size::new(handle_extent, bounds.size.height),
            ),
            Rect::new(
                Point::new(bounds.origin.x + position + handle_extent, bounds.origin.y),
                Size::new(
                    bounds.size.width - position - handle_extent,
                    bounds.size.height,
                ),
            ),
        ),
        SplitOrientation::Vertical => (
            Rect::new(bounds.origin, Size::new(bounds.size.width, position)),
            Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + position),
                Size::new(bounds.size.width, handle_extent),
            ),
            Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + position + handle_extent),
                Size::new(
                    bounds.size.width,
                    bounds.size.height - position - handle_extent,
                ),
            ),
        ),
    }
}

/// Paints the 1 pt divider centered in `bounds`, across the split, and the
/// grip when `grip` is set.
fn paint_divider(
    cx: &mut PaintContext<'_>,
    bounds: Rect,
    orientation: SplitOrientation,
    grip: bool,
    state: VisualState,
) {
    let line = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Handle).state(state));
    let center = bounds.center();
    let divider = match orientation {
        SplitOrientation::Horizontal => Rect::from_xywh(
            center.x - line.min_width / 2.0,
            bounds.origin.y,
            line.min_width,
            bounds.size.height,
        ),
        SplitOrientation::Vertical => Rect::from_xywh(
            bounds.origin.x,
            center.y - line.min_height / 2.0,
            bounds.size.width,
            line.min_height,
        ),
    };
    cx.slot(divider, &line, |_, _| {});
    if !grip {
        return;
    }
    let grip = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Thumb).state(state));
    let size = match orientation {
        SplitOrientation::Horizontal => Size::new(grip.min_width, grip.min_height),
        SplitOrientation::Vertical => Size::new(grip.min_height, grip.min_width),
    };
    let rect = Rect::new(
        Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
        size,
    );
    fill_shape(cx.list, rect, &grip.radius, cx.rtl, &grip.background);
}

fn primary_size(bounds: Rect, orientation: SplitOrientation) -> f64 {
    match orientation {
        SplitOrientation::Horizontal => bounds.size.width,
        SplitOrientation::Vertical => bounds.size.height,
    }
}

fn valid_number(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn valid_rect(rect: Rect) -> bool {
    rect.origin.x.is_finite()
        && rect.origin.y.is_finite()
        && valid_number(rect.size.width)
        && valid_number(rect.size.height)
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn validate_unique(ids: impl IntoIterator<Item = NodeId>) -> Result<(), ContainerError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ContainerError::DuplicateNode(id));
        }
    }
    Ok(())
}

fn validate_attached(
    tree: &Tree,
    ids: impl IntoIterator<Item = NodeId>,
) -> Result<(), ContainerError> {
    for id in ids {
        if !tree.contains(id) {
            return Err(ContainerError::MissingNode(id));
        }
    }
    Ok(())
}

fn validate_descendant(tree: &Tree, node: NodeId, container: NodeId) -> Result<(), ContainerError> {
    if !is_descendant(tree, node, container) {
        return Err(ContainerError::InvalidDescendant { node, container });
    }
    Ok(())
}

fn is_descendant(tree: &Tree, node: NodeId, container: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == container {
            return node != container;
        }
        current = tree.parent(id);
    }
    false
}
