use choscorui_core::{NodeId, Rect, Tree, TreeError};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Key(String);

impl Key {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub key: Key,
    pub bounds: Rect,
    pub focusable: bool,
    pub children: Vec<Self>,
}

impl Element {
    pub fn new(key: Key, bounds: Rect) -> Self {
        Self {
            key,
            bounds,
            focusable: false,
            children: Vec::new(),
        }
    }

    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    pub fn with_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children.extend(children);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposeError {
    DuplicateKey(Key),
    InvalidBounds(Key),
    Tree(TreeError),
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(f, "duplicate element key {key:?}"),
            Self::InvalidBounds(key) => write!(f, "element {key:?} has invalid bounds"),
            Self::Tree(error) => write!(f, "tree update failed: {error}"),
        }
    }
}

impl std::error::Error for ComposeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TreeError> for ComposeError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReconcileReport {
    pub created: usize,
    pub reused: usize,
    pub moved: usize,
    pub removed: usize,
}

#[derive(Default)]
pub struct Reconciler {
    nodes: HashMap<Key, NodeId>,
}

impl Reconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_id(&self, key: &Key) -> Option<NodeId> {
        self.nodes.get(key).copied()
    }

    pub fn reconcile(
        &mut self,
        tree: &mut Tree,
        elements: &[Element],
    ) -> Result<ReconcileReport, ComposeError> {
        let desired = validate(elements)?;
        let mut report = ReconcileReport::default();
        self.place_elements(tree, tree.root(), elements, &mut report)?;

        let removed: HashSet<_> = self
            .nodes
            .keys()
            .filter(|key| !desired.contains(*key))
            .cloned()
            .collect();
        let by_id: HashMap<_, _> = self.nodes.iter().map(|(key, id)| (*id, key)).collect();
        let roots: Vec<_> = removed
            .iter()
            .filter_map(|key| {
                let id = self.nodes[key];
                let parent_removed = tree
                    .parent(id)
                    .and_then(|parent| by_id.get(&parent))
                    .is_some_and(|parent_key| removed.contains(*parent_key));
                (!parent_removed).then_some(id)
            })
            .collect();
        for root in roots {
            tree.detach(root)?;
        }
        report.removed = removed.len();
        self.nodes.retain(|key, _| !removed.contains(key));
        Ok(report)
    }

    fn place_elements(
        &mut self,
        tree: &mut Tree,
        parent: NodeId,
        elements: &[Element],
        report: &mut ReconcileReport,
    ) -> Result<(), ComposeError> {
        for (index, element) in elements.iter().enumerate() {
            let existing = self.nodes.get(&element.key).copied();
            let id = if let Some(id) = existing {
                report.reused += 1;
                id
            } else {
                let id = tree.attach(parent, element.bounds)?;
                self.nodes.insert(element.key.clone(), id);
                report.created += 1;
                id
            };
            let is_placed = tree.parent(id) == Some(parent)
                && tree
                    .children(parent)
                    .and_then(|children| children.get(index))
                    == Some(&id);
            if !is_placed {
                tree.place_child(parent, id, index)?;
                if existing.is_some() {
                    report.moved += 1;
                }
            }
            tree.set_bounds(id, element.bounds)?;
            tree.set_focusable(id, element.focusable)?;
            self.place_elements(tree, id, &element.children, report)?;
        }
        Ok(())
    }
}

fn validate(elements: &[Element]) -> Result<HashSet<Key>, ComposeError> {
    let mut keys = HashSet::new();
    let mut pending: Vec<_> = elements.iter().collect();
    while let Some(element) = pending.pop() {
        if !keys.insert(element.key.clone()) {
            return Err(ComposeError::DuplicateKey(element.key.clone()));
        }
        let bounds = element.bounds;
        if !bounds.origin.x.is_finite()
            || !bounds.origin.y.is_finite()
            || !bounds.size.width.is_finite()
            || !bounds.size.height.is_finite()
            || bounds.size.width < 0.0
            || bounds.size.height < 0.0
        {
            return Err(ComposeError::InvalidBounds(element.key.clone()));
        }
        pending.extend(&element.children);
    }
    Ok(keys)
}
