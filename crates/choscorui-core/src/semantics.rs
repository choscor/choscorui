use crate::order::stable_order;
use crate::{NodeId, Rect, SemanticParts, Tree};
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
    /// A text field for a search query.
    SearchField,
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
    Table,
    Row,
    Cell,
    ColumnHeader,
    Tree,
    TreeItem,
    Menu,
    MenuItem,
    ComboBox,
    /// A button that opens a list of choices and shows the chosen one.
    PopUpButton,
    Tooltip,
    Dialog,
    Toolbar,
    StatusBar,
    Image,
    Link,
    ScrollBar,
    /// The bar of menu titles at the top of a window or a control.
    MenuBar,
    /// One menu title in a menu bar. It opens its menu.
    MenuBarItem,
    /// A checkbox that looks and acts like an on and off switch.
    Switch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticAction {
    Activate,
    SetValue,
    Increment,
    Decrement,
    Focus,
    Expand,
    Collapse,
    /// Opens a context menu or popup.
    ShowMenu,
    Dismiss,
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

#[derive(Clone, Copy, Default, PartialEq)]
pub struct SemanticState {
    pub disabled: bool,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub read_only: bool,
    pub invalid: bool,
    pub hidden: bool,
    /// The numeric value and its bounds, for a node with a range, such as
    /// a progress indicator. Assistive technology reads it as a number.
    pub range: Option<SemanticRange>,
    /// True for a dialog that keeps input inside it while it is open.
    pub modal: bool,
    /// True for a container whose content is still loading, such as a
    /// region of skeleton placeholders.
    pub busy: bool,
}

/// The numeric value of a range node, its bounds, and its step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticRange {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    /// The change of one increment or decrement.
    pub step: f64,
}

/// Lists `range` only when the node has one, and `modal` only when it is
/// true, so the output of a plain node stays short.
impl std::fmt::Debug for SemanticState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = f.debug_struct("SemanticState");
        out.field("disabled", &self.disabled)
            .field("checked", &self.checked)
            .field("selected", &self.selected)
            .field("expanded", &self.expanded)
            .field("read_only", &self.read_only)
            .field("invalid", &self.invalid)
            .field("hidden", &self.hidden);
        if let Some(range) = &self.range {
            out.field("range", range);
        }
        if self.modal {
            out.field("modal", &self.modal);
        }
        if self.busy {
            out.field("busy", &self.busy);
        }
        out.finish()
    }
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

/// Full size of a collection whose items may be virtualized. Only realized
/// items exist as semantic nodes; the container reports the total so
/// assistive technology can say "row 40 of 10,000".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticCollection {
    pub item_count: usize,
    pub column_count: Option<usize>,
}

/// Urgency of a spoken announcement. `High` interrupts current speech.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AnnouncementPriority {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEntry {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub bounds: Rect,
    pub focused: bool,
    pub node: SemanticNode,
    pub collection: Option<SemanticCollection>,
    /// Absolute position of a realized item inside its collection.
    pub item_index: Option<usize>,
    /// The key of the part of node `id` that this entry publishes, or
    /// `None` for the node itself. See [`SemanticParts`].
    pub part: Option<u64>,
    /// The key of the part that contains this part, or `None` when the
    /// entry is directly under `parent`.
    pub parent_part: Option<u64>,
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
        /// The key of the focused part of `new`, or `None` when the node
        /// itself has the focus.
        part: Option<u64>,
    },
    /// The name, value, or state of one part of a node changed.
    Part {
        node: NodeId,
        part: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticError {
    MissingNode,
    MissingRelationTarget(NodeId),
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode => f.write_str("the node is not in the tree"),
            Self::MissingRelationTarget(node) => {
                write!(f, "relation target {node:?} is not in the tree")
            }
        }
    }
}

impl std::error::Error for SemanticError {}

#[derive(Default)]
pub struct SemanticTree {
    nodes: HashMap<NodeId, SemanticNode>,
    collections: HashMap<NodeId, SemanticCollection>,
    item_indices: HashMap<NodeId, usize>,
    parts: HashMap<NodeId, SemanticParts>,
    focus: Option<(NodeId, Option<u64>)>,
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

    /// Declares `id` a collection of `collection.item_count` items, of which
    /// only the realized ones are semantic nodes. `None` clears it.
    pub fn set_collection(
        &mut self,
        id: NodeId,
        collection: Option<SemanticCollection>,
    ) -> Result<SemanticChange, SemanticError> {
        if !self.nodes.contains_key(&id) {
            return Err(SemanticError::MissingNode);
        }
        let old = match collection {
            Some(collection) => self.collections.insert(id, collection),
            None => self.collections.remove(&id),
        };
        Ok(if old == collection {
            SemanticChange::None
        } else {
            SemanticChange::Tree
        })
    }

    /// Sets the absolute index of a realized collection item.
    pub fn set_item_index(
        &mut self,
        id: NodeId,
        index: Option<usize>,
    ) -> Result<SemanticChange, SemanticError> {
        if !self.nodes.contains_key(&id) {
            return Err(SemanticError::MissingNode);
        }
        let old = match index {
            Some(index) => self.item_indices.insert(id, index),
            None => self.item_indices.remove(&id),
        };
        Ok(if old == index {
            SemanticChange::None
        } else {
            SemanticChange::Tree
        })
    }

    /// Publishes `parts` as the child nodes of `id`, and returns the
    /// changes. A change of the part count, a key, a parent, a role, or
    /// the collection of `parts` is a tree change; other changes are
    /// [`SemanticChange::Part`]. Parts without a collection keep the
    /// collection of `id`.
    pub fn set_parts(
        &mut self,
        id: NodeId,
        parts: SemanticParts,
    ) -> Result<Vec<SemanticChange>, SemanticError> {
        if !self.nodes.contains_key(&id) {
            return Err(SemanticError::MissingNode);
        }
        let collection = match parts.collection() {
            Some(collection) => self.set_collection(id, Some(collection))?,
            None => SemanticChange::None,
        };
        let old = if parts.is_empty() {
            self.parts.remove(&id)
        } else {
            self.parts.insert(id, parts)
        };
        let new = self.parts.get(&id).map_or(&[][..], SemanticParts::parts);
        let old = old.as_ref().map_or(&[][..], SemanticParts::parts);
        let same_shape = old.len() == new.len()
            && old.iter().zip(new).all(|(old, new)| {
                old.key == new.key && old.parent == new.parent && old.node.role == new.node.role
            });
        if !same_shape || collection != SemanticChange::None {
            return Ok(vec![SemanticChange::Tree]);
        }
        Ok(old
            .iter()
            .zip(new)
            .filter(|(old, new)| old != new)
            .map(|(_, new)| SemanticChange::Part {
                node: id,
                part: new.key,
            })
            .collect())
    }

    /// The published parts of `id`.
    pub fn parts(&self, id: NodeId) -> Option<&SemanticParts> {
        self.parts.get(&id)
    }

    /// Removes the semantic node of `id`, which stays in the tree.
    /// Returns true when there was one.
    pub fn remove(&mut self, id: NodeId) -> bool {
        self.collections.remove(&id);
        self.item_indices.remove(&id);
        self.parts.remove(&id);
        if self.focus.is_some_and(|(node, _)| node == id) {
            self.focus = None;
        }
        self.nodes.remove(&id).is_some()
    }

    pub fn supports(&self, id: NodeId, action: SemanticAction) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|node| node.actions.contains(&action))
    }

    /// Follows the focus of `tree` and the focused part of the focused
    /// node. Returns the change when either moved.
    pub fn sync_focus(&mut self, tree: &Tree) -> Option<SemanticChange> {
        let new = tree.focus().filter(|id| self.nodes.contains_key(id));
        let part = new
            .and_then(|id| self.parts.get(&id))
            .and_then(SemanticParts::focused_key);
        let focus = new.map(|id| (id, part));
        if self.focus == focus {
            return None;
        }
        let old = self.focus.map(|(id, _)| id);
        self.focus = focus;
        Some(SemanticChange::Focus { old, new, part })
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
        let found: Vec<_> = self
            .nodes
            .keys()
            .copied()
            .filter(|id| !tree.contains(*id))
            .collect();
        let order = stable_order(found.len(), &|a, b| found[a].cmp(&found[b]));
        let removed: Vec<_> = order.into_iter().map(|index| found[index]).collect();
        for id in &removed {
            self.nodes.remove(id);
            self.collections.remove(id);
            self.item_indices.remove(id);
            self.parts.remove(id);
        }
        if self.focus.is_some_and(|(id, _)| !tree.contains(id)) {
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
                let focused = tree.focus == Some(id);
                let parts = self.parts.get(&id);
                let focused_part = parts.and_then(SemanticParts::focused);
                result.push(SemanticEntry {
                    id,
                    parent: semantic_parent,
                    bounds: retained.bounds,
                    focused: focused && focused_part.is_none(),
                    node: node.clone(),
                    collection: self.collections.get(&id).copied(),
                    item_index: self.item_indices.get(&id).copied(),
                    part: None,
                    parent_part: None,
                });
                let parts = parts.map_or(&[][..], SemanticParts::parts);
                for (position, part) in parts.iter().enumerate() {
                    result.push(SemanticEntry {
                        id,
                        parent: Some(id),
                        bounds: part.bounds,
                        focused: focused && focused_part == Some(position),
                        node: part.node.clone(),
                        collection: part.collection,
                        item_index: part.item_index,
                        part: Some(part.key),
                        parent_part: part
                            .parent
                            .and_then(|parent| parts.get(parent))
                            .map(|parent| parent.key),
                    });
                }
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
