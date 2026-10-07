use choscorui_core::{
    Color, DisplayList, Key, KeyEvent, NodeId, Point, Rect, SemanticAction, SemanticNode,
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
pub enum SplitterInput {
    Key(KeyEvent),
    Semantic(SemanticAction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitterVisualState {
    pub focused: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SplitterStyle {
    pub handle: Color,
    pub focused_handle: Color,
}

impl SplitterStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: SplitterVisualState) {
        list.fill_rect(
            bounds,
            if state.focused {
                self.focused_handle
            } else {
                self.handle
            },
        );
    }
}

/// A two-pane split whose `position` is the first pane's width or height.
pub struct Splitter {
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
}

impl Splitter {
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
        })
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
        input: SplitterInput,
    ) -> Result<Option<f64>, ContainerError> {
        let target = match input {
            SplitterInput::Semantic(SemanticAction::Focus) => {
                validate_attached(tree, [self.handle])?;
                tree.set_focus(Some(self.handle))?;
                return Ok(None);
            }
            SplitterInput::Semantic(SemanticAction::Increment) => self.position + self.step,
            SplitterInput::Semantic(SemanticAction::Decrement) => self.position - self.step,
            SplitterInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(self.handle) => {
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

    pub fn visual_state(&self, tree: &Tree) -> SplitterVisualState {
        SplitterVisualState {
            focused: tree.focus() == Some(self.handle),
        }
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
