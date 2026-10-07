use crate::{NodeId, Rect, Tree};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRole {
    Application,
    Window,
    Group,
    Button,
    Checkbox,
    RadioButton,
    RadioGroup,
    TextField,
    PasswordField,
    StaticText,
    List,
    ListItem,
    ScrollArea,
    Slider,
    SpinButton,
    ProgressIndicator,
    Tab,
    TabList,
    TabPanel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticAction {
    Activate,
    SetValue,
    Increment,
    Decrement,
    Focus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRelation {
    LabelledBy(NodeId),
    DescribedBy(NodeId),
    Controls(NodeId),
}

impl SemanticRelation {
    fn target(self) -> NodeId {
        match self {
            Self::LabelledBy(id) | Self::DescribedBy(id) | Self::Controls(id) => id,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SemanticState {
    pub disabled: bool,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub read_only: bool,
    pub invalid: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    pub role: SemanticRole,
    pub name: String,
    pub description: String,
    pub value: Option<String>,
    pub state: SemanticState,
    pub relations: Vec<SemanticRelation>,
    pub actions: Vec<SemanticAction>,
    pub text: Option<SemanticText>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticText {
    pub character_count_utf16: usize,
    pub selection_utf16: Range<usize>,
    pub multiline: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEntry {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub bounds: Rect,
    pub focused: bool,
    pub node: SemanticNode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticChange {
    None,
    Tree,
    Label(NodeId),
    Value(NodeId),
    State(NodeId),
    ValueAndState(NodeId),
    Focus {
        old: Option<NodeId>,
        new: Option<NodeId>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticError {
    MissingNode,
    MissingRelationTarget(NodeId),
}

#[derive(Default)]
pub struct SemanticTree {
    nodes: HashMap<NodeId, SemanticNode>,
    focus: Option<NodeId>,
}

impl SemanticTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(
        &mut self,
        id: NodeId,
        node: SemanticNode,
        tree: &Tree,
    ) -> Result<SemanticChange, SemanticError> {
        if !tree.contains(id) {
            return Err(SemanticError::MissingNode);
        }
        for relation in &node.relations {
            if !tree.contains(relation.target()) {
                return Err(SemanticError::MissingRelationTarget(relation.target()));
            }
        }
        let change = match self.nodes.get(&id) {
            None => SemanticChange::Tree,
            Some(old) if old == &node => SemanticChange::None,
            Some(old)
                if old.role == node.role
                    && old.value == node.value
                    && old.state == node.state
                    && old.relations == node.relations
                    && old.actions == node.actions
                    && old.text == node.text =>
            {
                SemanticChange::Label(id)
            }
            Some(old)
                if old.role == node.role
                    && old.name == node.name
                    && old.description == node.description
                    && old.relations == node.relations
                    && old.actions == node.actions =>
            {
                let value_changed = old.value != node.value || old.text != node.text;
                let state_changed = old.state != node.state;
                match (value_changed, state_changed) {
                    (true, true) => SemanticChange::ValueAndState(id),
                    (true, false) => SemanticChange::Value(id),
                    (false, true) => SemanticChange::State(id),
                    (false, false) => SemanticChange::Tree,
                }
            }
            Some(_) => SemanticChange::Tree,
        };
        self.nodes.insert(id, node);
        Ok(change)
    }

    pub fn supports(&self, id: NodeId, action: SemanticAction) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|node| node.actions.contains(&action))
    }

    pub fn sync_focus(&mut self, tree: &Tree) -> Option<SemanticChange> {
        let new = tree.focus().filter(|id| self.nodes.contains_key(id));
        if self.focus == new {
            return None;
        }
        let old = self.focus;
        self.focus = new;
        Some(SemanticChange::Focus { old, new })
    }

    pub fn validate(&self, tree: &Tree) -> Result<(), SemanticError> {
        for (&id, node) in &self.nodes {
            if !tree.contains(id) {
                return Err(SemanticError::MissingNode);
            }
            for relation in &node.relations {
                if !tree.contains(relation.target()) || !self.nodes.contains_key(&relation.target())
                {
                    return Err(SemanticError::MissingRelationTarget(relation.target()));
                }
            }
        }
        Ok(())
    }

    pub fn prune(&mut self, tree: &Tree) -> Vec<NodeId> {
        let mut removed: Vec<_> = self
            .nodes
            .keys()
            .copied()
            .filter(|id| !tree.contains(*id))
            .collect();
        removed.sort_by_key(|id| id.0);
        for id in &removed {
            self.nodes.remove(id);
        }
        if self.focus.is_some_and(|id| !tree.contains(id)) {
            self.focus = None;
        }
        removed
    }

    pub fn snapshot(&self, tree: &Tree) -> Vec<SemanticEntry> {
        let mut result = Vec::new();
        let mut pending = vec![(tree.root, None)];
        while let Some((id, semantic_parent)) = pending.pop() {
            let Some(retained) = tree.nodes.get(&id) else {
                continue;
            };
            let next_parent = if let Some(node) = self.nodes.get(&id) {
                result.push(SemanticEntry {
                    id,
                    parent: semantic_parent,
                    bounds: retained.bounds,
                    focused: tree.focus == Some(id),
                    node: node.clone(),
                });
                Some(id)
            } else {
                semantic_parent
            };
            pending.extend(
                retained
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, next_parent)),
            );
        }
        result
    }
}
