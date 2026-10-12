use crate::style::{
    Content, ControlKind, Orientation, PaintContext, Slot, StyleQuery, Styles, TabsVariant,
    TextAlign, TextMeasure, VisualState, choice_size, fill_shape, paint_choice,
};
use choscorui_core::{
    DesignTokens, Dirty, Key, KeyEvent, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SemanticRelation, SemanticRole, SemanticState, Size, Tree, TreeError,
};
use std::collections::HashSet;

mod hosted;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionError {
    Empty,
    DuplicateNode(NodeId),
    UnknownSelection,
    MissingNode(NodeId),
    DisabledSelection,
    Tree(TreeError),
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the group has no options"),
            Self::DuplicateNode(node) => write!(f, "node {node:?} is already an option"),
            Self::UnknownSelection => f.write_str("the selection is not one of the options"),
            Self::MissingNode(node) => write!(f, "node {node:?} is not in the tree"),
            Self::DisabledSelection => f.write_str("a disabled option cannot be selected"),
            Self::Tree(error) => write!(f, "tree update failed: {error}"),
        }
    }
}

impl std::error::Error for SelectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TreeError> for SelectionError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RadioOption {
    id: NodeId,
    label: String,
    enabled: bool,
}

impl RadioOption {
    pub fn new(id: NodeId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
        }
    }

    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadioInput {
    Activate,
    Key(KeyEvent),
    Semantic(SemanticAction),
}

pub struct RadioGroup {
    label: String,
    options: Vec<RadioOption>,
    selected: NodeId,
}

impl RadioGroup {
    pub fn new(
        label: impl Into<String>,
        options: Vec<RadioOption>,
        selected: NodeId,
    ) -> Result<Self, SelectionError> {
        validate_unique(options.iter().map(|option| option.id))?;
        let selected_option = options
            .iter()
            .find(|option| option.id == selected)
            .ok_or(SelectionError::UnknownSelection)?;
        if !selected_option.enabled {
            return Err(SelectionError::DisabledSelection);
        }
        Ok(Self {
            label: label.into(),
            options,
            selected,
        })
    }

    pub const fn selected(&self) -> NodeId {
        self.selected
    }

    pub fn install(&self, tree: &mut Tree) -> Result<(), SelectionError> {
        validate_attached(tree, self.options.iter().map(|option| option.id))?;
        for option in &self.options {
            tree.set_focusable(option.id, option.id == self.selected && option.enabled)?;
        }
        Ok(())
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: RadioInput,
    ) -> Result<Option<NodeId>, SelectionError> {
        let index = self.index_of(id)?;
        if !self.options[index].enabled {
            return Ok(None);
        }
        let target = match input {
            RadioInput::Activate | RadioInput::Semantic(SemanticAction::Activate) => Some(index),
            RadioInput::Semantic(SemanticAction::Focus) => {
                tree.set_focus(Some(id))?;
                return Ok(None);
            }
            RadioInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(id) => match key {
                Key::Space | Key::Enter => Some(index),
                _ => navigation_target(&self.options, index, key, |option| option.enabled),
            },
            _ => None,
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let target_id = self.options[target].id;
        if target_id == self.selected {
            return Ok(None);
        }
        let previous = self.selected;
        validate_attached(tree, [previous, target_id].into_iter())?;
        self.selected = target_id;
        for candidate in [previous, target_id] {
            tree.set_focusable(candidate, candidate == target_id)?;
            tree.invalidate(candidate, Dirty::PAINT)?;
            tree.invalidate(candidate, Dirty::SEMANTICS)?;
        }
        tree.set_focus(Some(target_id))?;
        Ok(Some(target_id))
    }

    pub fn semantics(&self, id: NodeId) -> Option<SemanticNode> {
        let option = self.options.iter().find(|option| option.id == id)?;
        Some(selectable_semantics(
            SemanticRole::RadioButton,
            &option.label,
            option.enabled,
            option.id == self.selected,
            Vec::new(),
        ))
    }

    pub fn group_semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::RadioGroup,
            name: self.label.clone(),
            description: String::new(),
            value: self
                .options
                .iter()
                .find(|option| option.id == self.selected)
                .map(|option| option.label.clone()),
            state: SemanticState::default(),
            relations: self
                .options
                .iter()
                .map(|option| SemanticRelation::Controls(option.id))
                .collect(),
            actions: Vec::new(),
            text: None,
        }
    }

    /// The label of the option at `id`.
    pub fn option_label(&self, id: NodeId) -> Option<&str> {
        self.options
            .iter()
            .find(|option| option.id == id)
            .map(|option| option.label.as_str())
    }

    /// The flags the radio recipe reads for the option at `id`.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> Option<VisualState> {
        let option = self.options.iter().find(|option| option.id == id)?;
        Some(
            VisualState::NONE
                .set(VisualState::DISABLED, !option.enabled)
                .set(VisualState::CHECKED, id == self.selected)
                .set(VisualState::FOCUSED, tree.focus() == Some(id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(id)),
        )
    }

    /// The natural size of the option at `id`: the circle, the gap, and
    /// the label.
    pub fn measure_option(
        &self,
        id: NodeId,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
    ) -> Option<Size> {
        let label = self.option_label(id)?;
        Some(choice_size(
            ControlKind::RadioGroup,
            label,
            styles,
            tokens,
            text,
        ))
    }

    /// Paints the option at `id`: the circle at the start of the bounds,
    /// the dot when selected, and the label after it.
    pub fn paint_option(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let (Some(state), Some(label)) = (self.visual_state(tree, id), self.option_label(id))
        else {
            return;
        };
        paint_radio(cx, label, state);
    }

    fn index_of(&self, id: NodeId) -> Result<usize, SelectionError> {
        self.options
            .iter()
            .position(|option| option.id == id)
            .ok_or(SelectionError::MissingNode(id))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tab {
    tab: NodeId,
    panel: NodeId,
    label: String,
    enabled: bool,
}

impl Tab {
    pub fn new(tab: NodeId, panel: NodeId, label: impl Into<String>) -> Self {
        Self {
            tab,
            panel,
            label: label.into(),
            enabled: true,
        }
    }

    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabInput {
    Activate,
    Key(KeyEvent),
    Semantic(SemanticAction),
}

/// The old name of [`Tabs`].
#[deprecated(since = "0.1.0", note = "renamed to `Tabs`")]
pub type TabSet = Tabs;

/// A tab list with one panel per tab. Arrow keys move the selection
/// along the list, wrapping at its ends.
pub struct Tabs {
    label: String,
    tabs: Vec<Tab>,
    selected: NodeId,
    variant: TabsVariant,
    orientation: Orientation,
}

impl Tabs {
    pub fn new(
        label: impl Into<String>,
        tabs: Vec<Tab>,
        selected: NodeId,
    ) -> Result<Self, SelectionError> {
        validate_unique(tabs.iter().flat_map(|tab| [tab.tab, tab.panel]))?;
        let selected_tab = tabs
            .iter()
            .find(|tab| tab.tab == selected)
            .ok_or(SelectionError::UnknownSelection)?;
        if !selected_tab.enabled {
            return Err(SelectionError::DisabledSelection);
        }
        Ok(Self {
            label: label.into(),
            tabs,
            selected,
            variant: TabsVariant::Default,
            orientation: Orientation::Horizontal,
        })
    }

    #[must_use]
    pub fn with_variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Lays the tab list out along `orientation`. The line variant draws
    /// its indicator under a horizontal tab and beside a vertical one.
    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn variant(&self) -> TabsVariant {
        self.variant
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub const fn selected_tab(&self) -> NodeId {
        self.selected
    }

    pub fn selected_panel(&self) -> NodeId {
        self.tabs
            .iter()
            .find(|tab| tab.tab == self.selected)
            .expect("selected tab is retained")
            .panel
    }

    pub fn install(&self, tree: &mut Tree) -> Result<(), SelectionError> {
        validate_attached(tree, self.tabs.iter().flat_map(|tab| [tab.tab, tab.panel]))?;
        for tab in &self.tabs {
            tree.set_focusable(tab.tab, tab.tab == self.selected && tab.enabled)?;
        }
        Ok(())
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: TabInput,
    ) -> Result<Option<NodeId>, SelectionError> {
        let index = self.index_of_tab(id)?;
        if !self.tabs[index].enabled {
            return Ok(None);
        }
        let target = match input {
            TabInput::Activate | TabInput::Semantic(SemanticAction::Activate) => Some(index),
            TabInput::Semantic(SemanticAction::Focus) => {
                tree.set_focus(Some(id))?;
                return Ok(None);
            }
            TabInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(id) => match key {
                Key::Space | Key::Enter => Some(index),
                _ => navigation_target(&self.tabs, index, key, |tab| tab.enabled),
            },
            _ => None,
        };
        let Some(target) = target else {
            return Ok(None);
        };
        self.select(tree, target)
    }

    pub fn tab_semantics(&self, id: NodeId) -> Option<SemanticNode> {
        let tab = self.tabs.iter().find(|tab| tab.tab == id)?;
        Some(selectable_semantics(
            SemanticRole::Tab,
            &tab.label,
            tab.enabled,
            id == self.selected,
            vec![SemanticRelation::Controls(tab.panel)],
        ))
    }

    pub fn panel_semantics(&self, id: NodeId) -> Option<SemanticNode> {
        let tab = self.tabs.iter().find(|tab| tab.panel == id)?;
        Some(SemanticNode {
            role: SemanticRole::TabPanel,
            name: tab.label.clone(),
            description: String::new(),
            value: None,
            state: SemanticState {
                hidden: tab.tab != self.selected,
                ..SemanticState::default()
            },
            relations: vec![SemanticRelation::LabelledBy(tab.tab)],
            actions: Vec::new(),
            text: None,
        })
    }

    pub fn group_semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::TabList,
            name: self.label.clone(),
            description: String::new(),
            value: self
                .tabs
                .iter()
                .find(|tab| tab.tab == self.selected)
                .map(|tab| tab.label.clone()),
            state: SemanticState::default(),
            relations: self
                .tabs
                .iter()
                .map(|tab| SemanticRelation::Controls(tab.tab))
                .collect(),
            actions: Vec::new(),
            text: None,
        }
    }

    /// The label of the tab at `id`.
    pub fn tab_label(&self, id: NodeId) -> Option<&str> {
        self.tabs
            .iter()
            .find(|tab| tab.tab == id)
            .map(|tab| tab.label.as_str())
    }

    /// The flags the tabs recipe reads for the tab at `id`.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> Option<VisualState> {
        let tab = self.tabs.iter().find(|tab| tab.tab == id)?;
        Some(
            VisualState::NONE
                .set(VisualState::DISABLED, !tab.enabled)
                .set(VisualState::SELECTED, tab.tab == self.selected)
                .set(VisualState::FOCUSED, tree.focus() == Some(id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(id)),
        )
    }

    fn query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        tabs_query(self.variant, slot, state)
    }

    /// The natural size of the tab at `id`: its label and padding.
    pub fn measure_tab(
        &self,
        id: NodeId,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
    ) -> Option<Size> {
        let label = self.tab_label(id)?;
        let style = styles.resolve(tokens, self.query(Slot::Trigger, VisualState::NONE));
        Some(Content::label(label).size(&style, text))
    }

    /// Paints the tab list surface in the context's bounds. Paint it
    /// before the tabs.
    pub fn paint_list(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(self.query(Slot::Group, VisualState::NONE));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |_, _| {});
    }

    /// Paints the tab at `id` in the context's bounds: its trigger box,
    /// label, and the line indicator when selected.
    pub fn paint_tab(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let (Some(state), Some(label)) = (self.visual_state(tree, id), self.tab_label(id)) else {
            return;
        };
        paint_tab(cx, self.variant, self.orientation, label, state);
    }

    fn index_of_tab(&self, id: NodeId) -> Result<usize, SelectionError> {
        self.tabs
            .iter()
            .position(|tab| tab.tab == id)
            .ok_or(SelectionError::MissingNode(id))
    }

    fn select(&mut self, tree: &mut Tree, target: usize) -> Result<Option<NodeId>, SelectionError> {
        let target_id = self.tabs[target].tab;
        if target_id == self.selected {
            return Ok(None);
        }
        let previous = self.index_of_tab(self.selected)?;
        validate_attached(
            tree,
            [
                self.tabs[previous].tab,
                self.tabs[previous].panel,
                self.tabs[target].tab,
                self.tabs[target].panel,
            ]
            .into_iter(),
        )?;
        self.selected = target_id;
        tree.set_focusable(self.tabs[previous].tab, false)?;
        tree.set_focusable(target_id, true)?;
        for index in [previous, target] {
            tree.invalidate(self.tabs[index].tab, Dirty::PAINT)?;
            tree.invalidate(self.tabs[index].tab, Dirty::SEMANTICS)?;
            tree.invalidate(self.tabs[index].panel, Dirty::PAINT)?;
            tree.invalidate(self.tabs[index].panel, Dirty::SEMANTICS)?;
        }
        tree.set_focus(Some(target_id))?;
        Ok(Some(target_id))
    }
}

/// Paints a radio option in the context's bounds: the circle at the start,
/// the dot when checked, and the label after it.
fn paint_radio(cx: &mut PaintContext<'_>, label: &str, state: VisualState) {
    let dot = cx.style(StyleQuery::new(ControlKind::RadioGroup, Slot::Indicator).state(state));
    paint_choice(cx, ControlKind::RadioGroup, label, state, |cx, rect| {
        if state.checked() {
            let size = Size::new(dot.min_width, dot.min_height);
            let center = rect.center();
            let dot_rect = Rect::new(
                Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
                size,
            );
            fill_shape(cx.list, dot_rect, &dot.radius, cx.rtl, dot.background);
        }
    });
}

fn tabs_query(variant: TabsVariant, slot: Slot, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Tabs, slot)
        .variant(variant)
        .state(state)
}

/// Paints a tab in the context's bounds: its trigger box, label, and the
/// line indicator when selected.
fn paint_tab(
    cx: &mut PaintContext<'_>,
    variant: TabsVariant,
    orientation: Orientation,
    label: &str,
    state: VisualState,
) {
    let style = cx.style(tabs_query(variant, Slot::Trigger, state));
    let bounds = cx.bounds;
    cx.slot(bounds, &style, |cx, rect| {
        cx.label(rect, label, &style, TextAlign::Center);
    });
    let indicator = cx.style(tabs_query(variant, Slot::Indicator, state));
    if indicator.background.alpha == 0 {
        return;
    }
    // The indicator sits just outside the tab, over the list padding.
    let offset = -indicator.padding.block_start;
    let line = match orientation {
        Orientation::Horizontal => Rect::new(
            Point::new(
                bounds.origin.x,
                bounds.max_y() + offset - indicator.min_height,
            ),
            Size::new(bounds.size.width, indicator.min_height),
        ),
        Orientation::Vertical => {
            let x = if cx.rtl {
                bounds.origin.x - offset
            } else {
                bounds.max_x() + offset - indicator.min_width
            };
            Rect::new(
                Point::new(x, bounds.origin.y),
                Size::new(indicator.min_width, bounds.size.height),
            )
        }
    };
    cx.list.fill_rect(line, indicator.background);
}

fn selectable_semantics(
    role: SemanticRole,
    label: &str,
    enabled: bool,
    selected: bool,
    relations: Vec<SemanticRelation>,
) -> SemanticNode {
    SemanticNode {
        role,
        name: label.to_owned(),
        description: String::new(),
        value: Some(selected.to_string()),
        state: SemanticState {
            disabled: !enabled,
            checked: (role == SemanticRole::RadioButton).then_some(selected),
            selected: (role == SemanticRole::Tab).then_some(selected),
            ..SemanticState::default()
        },
        relations,
        actions: if enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        },
        text: None,
    }
}

fn validate_unique(ids: impl Iterator<Item = NodeId>) -> Result<(), SelectionError> {
    let mut seen = HashSet::new();
    let mut count = 0;
    for id in ids {
        count += 1;
        if !seen.insert(id) {
            return Err(SelectionError::DuplicateNode(id));
        }
    }
    if count == 0 {
        Err(SelectionError::Empty)
    } else {
        Ok(())
    }
}

fn validate_attached(tree: &Tree, ids: impl Iterator<Item = NodeId>) -> Result<(), SelectionError> {
    for id in ids {
        if !tree.contains(id) {
            return Err(SelectionError::MissingNode(id));
        }
    }
    Ok(())
}

fn navigation_target<T>(
    items: &[T],
    current: usize,
    key: Key,
    enabled: impl Fn(&T) -> bool,
) -> Option<usize> {
    match key {
        Key::Home => (0..items.len()).find(|index| enabled(&items[*index])),
        Key::End => (0..items.len()).rev().find(|index| enabled(&items[*index])),
        Key::ArrowRight | Key::ArrowDown => (1..=items.len())
            .map(|offset| (current + offset) % items.len())
            .find(|index| enabled(&items[*index])),
        Key::ArrowLeft | Key::ArrowUp => (1..=items.len())
            .map(|offset| (current + items.len() - offset) % items.len())
            .find(|index| enabled(&items[*index])),
        _ => None,
    }
}
