use crate::style::{
    ControlKind, ControlSize, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState,
    fill_shape,
};
use choscorui_core::{
    DesignTokens, Key, KeyEvent, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SemanticRelation, SemanticRole, SemanticState, Size, Tree, TreeError,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizableInput {
    Key(KeyEvent),
    Semantic(SemanticAction),
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
}

impl Resizable {
    #[allow(clippy::too_many_arguments)]
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
    /// the keys, and the view layer places the panes in
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
        if !valid_number(minimum_first)
            || !valid_number(minimum_second)
            || !valid_number(handle_extent)
            || handle_extent <= 0.0
        {
            return Err(ContainerError::InvalidGeometry);
        }
        if !position.is_finite() || position < minimum_first {
            return Err(ContainerError::InvalidPosition);
        }
        Ok(HostedResizable {
            label: label.into(),
            orientation,
            minimum_first,
            minimum_second,
            handle_extent,
            step: 10.0,
            position,
            grip: false,
            bounds: None,
            rtl: false,
        })
    }
}

/// A [`Resizable`] mounted in a control host. The node covers both panes
/// and the handle. The arrow keys along the split axis, Home, End, and
/// the `Increment` and `Decrement` actions move the handle and report
/// `Changed(Number(position))`. In a right-to-left layout a horizontal
/// split puts the first pane on the right and mirrors the arrow keys.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedResizable {
    label: String,
    orientation: SplitOrientation,
    minimum_first: f64,
    minimum_second: f64,
    handle_extent: f64,
    step: f64,
    position: f64,
    grip: bool,
    /// The bounds from the last arrange.
    bounds: Option<Rect>,
    rtl: bool,
}

impl HostedResizable {
    /// Shows a grip on the handle.
    #[must_use]
    pub fn with_grip(mut self, grip: bool) -> Self {
        self.grip = grip;
        self
    }

    /// The first pane's width or height, within the pane minimums of the
    /// last arranged bounds. Bounds too small for the position keep it for
    /// when they grow again.
    pub fn position(&self) -> f64 {
        self.clamp(self.position)
    }

    /// The first pane, the handle, and the second pane in the last
    /// arranged bounds. A right-to-left horizontal split mirrors them.
    pub fn panes(&self) -> (Rect, Rect, Rect) {
        let bounds = self.bounds.unwrap_or_default();
        let (first, handle, second) = split_rects(
            bounds,
            self.orientation,
            self.position(),
            self.handle_extent,
        );
        if self.rtl && self.orientation == SplitOrientation::Horizontal {
            let mirror = |rect: Rect| {
                Rect::from_xywh(
                    bounds.origin.x + bounds.max_x() - rect.max_x(),
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                )
            };
            (mirror(first), mirror(handle), mirror(second))
        } else {
            (first, handle, second)
        }
    }

    /// The largest position, once the control has bounds.
    fn maximum(&self) -> Option<f64> {
        let bounds = self.bounds?;
        let maximum =
            primary_size(bounds, self.orientation) - self.handle_extent - self.minimum_second;
        Some(maximum.max(self.minimum_first))
    }

    fn clamp(&self, position: f64) -> f64 {
        let position = position.max(self.minimum_first);
        self.maximum()
            .map_or(position, |maximum| position.min(maximum))
    }

    /// Moves the handle to `target`, clamped, and reports a change.
    fn move_to(&mut self, cx: &mut crate::ControlContext<'_>, target: f64) {
        let target = self.clamp(target);
        if target == self.position() {
            return;
        }
        self.position = target;
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
        cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
            target,
        )));
    }
}

impl crate::CustomControl for HostedResizable {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let along = self.minimum_first + self.handle_extent + self.minimum_second;
        let natural = match self.orientation {
            SplitOrientation::Horizontal => Size::new(along, 0.0),
            SplitOrientation::Vertical => Size::new(0.0, along),
        };
        offered(cx, natural)
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, bounds: Rect) {
        self.bounds = Some(bounds);
        self.rtl = cx.rtl();
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use choscorui_core::EventReply;
        if let crate::ControlEvent::FocusChanged(_) = event {
            cx.request_paint();
            return EventReply::Continue;
        }
        let Some(crate::hosted::Routed::Key(KeyEvent::Down { key, .. })) =
            crate::hosted::route(cx, event)
        else {
            return EventReply::Continue;
        };
        let (back, forward) = match (self.orientation, self.rtl) {
            (SplitOrientation::Horizontal, false) => (Key::ArrowLeft, Key::ArrowRight),
            (SplitOrientation::Horizontal, true) => (Key::ArrowRight, Key::ArrowLeft),
            (SplitOrientation::Vertical, _) => (Key::ArrowUp, Key::ArrowDown),
        };
        let target = match key {
            Key::Home => self.minimum_first,
            Key::End => f64::INFINITY,
            key if key == back => self.position() - self.step,
            key if key == forward => self.position() + self.step,
            _ => return EventReply::Continue,
        };
        self.move_to(cx, target);
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (_, handle, _) = self.panes();
        let state = cx.state;
        paint_divider(cx, handle, self.orientation, self.grip, state);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = crate::hosted::node(
            SemanticRole::Slider,
            &self.label,
            vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
        );
        node.value = Some(format_number(self.position()));
        node
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Increment => self.move_to(cx, self.position() + self.step),
            SemanticAction::Decrement => self.move_to(cx, self.position() - self.step),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        let crate::ControlValue::Number(position) = *value else {
            return false;
        };
        if !position.is_finite() || self.clamp(position) != position {
            return false;
        }
        self.position = position;
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
    fill_shape(cx.list, rect, &grip.radius, cx.rtl, grip.background);
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
