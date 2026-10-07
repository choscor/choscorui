use choscorui_core::{
    Color, Dirty, DisplayList, Key, KeyEvent, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SemanticRelation, SemanticRole, SemanticState, Size, Tree, TreeError,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionError {
    Empty,
    DuplicateNode(NodeId),
    UnknownSelection,
    MissingNode(NodeId),
    DisabledSelection,
    Tree(TreeError),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RadioVisualState {
    pub enabled: bool,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RadioStyle {
    pub ring_color: Color,
    pub selected_color: Color,
    pub disabled_color: Color,
    pub focus_color: Color,
}

impl RadioStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: RadioVisualState) {
        let ring = if state.enabled {
            self.ring_color
        } else {
            self.disabled_color
        };
        list.fill_rect(bounds, ring);
        if state.selected {
            let inset = 6.0_f64
                .min(bounds.size.width / 2.0)
                .min(bounds.size.height / 2.0);
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x + inset, bounds.origin.y + inset),
                    Size::new(
                        (bounds.size.width - inset * 2.0).max(0.0),
                        (bounds.size.height - inset * 2.0).max(0.0),
                    ),
                ),
                self.selected_color,
            );
        }
        if state.focused {
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 2.0),
                    Size::new(bounds.size.width, 2.0),
                ),
                self.focus_color,
            );
        }
    }
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

    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> Option<RadioVisualState> {
        let option = self.options.iter().find(|option| option.id == id)?;
        Some(RadioVisualState {
            enabled: option.enabled,
            selected: id == self.selected,
            focused: tree.focus() == Some(id),
        })
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabVisualState {
    pub enabled: bool,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TabStyle {
    pub background: Color,
    pub selected: Color,
    pub disabled: Color,
    pub focus: Color,
}

impl TabStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: TabVisualState) {
        let color = if !state.enabled {
            self.disabled
        } else if state.selected {
            self.selected
        } else {
            self.background
        };
        list.fill_rect(bounds, color);
        if state.focused {
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 2.0),
                    Size::new(bounds.size.width, 2.0),
                ),
                self.focus,
            );
        }
    }
}

pub struct TabSet {
    label: String,
    tabs: Vec<Tab>,
    selected: NodeId,
}

impl TabSet {
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
        })
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

    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> Option<TabVisualState> {
        let tab = self.tabs.iter().find(|tab| tab.tab == id)?;
        Some(TabVisualState {
            enabled: tab.enabled,
            selected: tab.tab == self.selected,
            focused: tree.focus() == Some(id),
        })
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
