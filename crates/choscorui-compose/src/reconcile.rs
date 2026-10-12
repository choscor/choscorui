//! Keyed reconciliation. [`plan_keyed`] compares the old and new keys of
//! one child list. [`Reconciler`] applies `Element` descriptions to a
//! `Tree` with it, and `ForEach` uses it for its items.

use choscorui_controls::{ControlKind, ControlValue};
use choscorui_core::{NodeId, Rect, Tree, TreeError};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Key(String);

impl Key {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// The named values of an [`Element`], in name order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties(BTreeMap<String, ControlValue>);

impl Properties {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `value` under `name`, or replaces the value that `name` has.
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: ControlValue) -> Self {
        self.insert(name, value);
        self
    }

    /// Sets `value` under `name` and returns the old value.
    pub fn insert(&mut self, name: impl Into<String>, value: ControlValue) -> Option<ControlValue> {
        self.0.insert(name.into(), value)
    }

    pub fn get(&self, name: &str) -> Option<&ControlValue> {
        self.0.get(name)
    }

    /// The names and values in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ControlValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub key: Key,
    pub bounds: Rect,
    pub focusable: bool,
    /// The control that the node shows. A change of kind gives a new node.
    pub kind: Option<ControlKind>,
    /// The values of the node. A change keeps the node.
    pub properties: Properties,
    pub children: Vec<Self>,
}

impl Element {
    pub fn new(key: Key, bounds: Rect) -> Self {
        Self {
            key,
            bounds,
            focusable: false,
            kind: None,
            properties: Properties::default(),
            children: Vec::new(),
        }
    }

    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    #[must_use]
    pub fn kind(mut self, kind: ControlKind) -> Self {
        self.kind = Some(kind);
        self
    }

    #[must_use]
    pub fn properties(mut self, properties: Properties) -> Self {
        self.properties = properties;
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

/// What one reconciliation did. `moved` counts the kept children that
/// changed their place among their siblings or changed their parent.
/// `updated` counts the kept elements whose properties changed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReconcileReport {
    pub created: usize,
    pub reused: usize,
    pub moved: usize,
    pub removed: usize,
    pub updated: usize,
}

/// One child of a [`KeyedPlan`], in the new order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyedChild<K> {
    pub key: K,
    /// The index of the child in the new keys.
    pub index: usize,
    /// The index of the old child that it keeps, or `None` for a new child.
    pub reuse: Option<usize>,
}

/// The decisions of a keyed diff of one child list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyedPlan<K> {
    /// The new children without duplicates, in order.
    pub children: Vec<KeyedChild<K>>,
    /// The indices of the old children that no new child keeps, ascending.
    pub removed: Vec<usize>,
    /// The keys that occur again in the new keys. Only the first child of
    /// each key stays.
    pub duplicates: Vec<K>,
    /// The counts of the plan. `updated` is always zero.
    pub report: ReconcileReport,
}

/// Compares the old and new keys of one child list.
///
/// - A new child keeps the old child with the same key when
///   `reusable(old_index, new_index)` is true. Otherwise the child is new
///   and the old child is removed.
/// - `report.moved` is the smallest number of kept children that must
///   move to get the new order.
///
/// The cost is proportional to the number of children.
pub fn plan_keyed<K: Clone + Eq + Hash>(
    old: &[K],
    new: &[K],
    mut reusable: impl FnMut(usize, usize) -> bool,
) -> KeyedPlan<K> {
    let mut by_key: HashMap<&K, usize> = HashMap::with_capacity(old.len());
    for (index, key) in old.iter().enumerate() {
        by_key.entry(key).or_insert(index);
    }
    let mut kept = vec![false; old.len()];
    let mut seen = HashSet::with_capacity(new.len());
    let mut plan = KeyedPlan {
        children: Vec::with_capacity(new.len()),
        removed: Vec::new(),
        duplicates: Vec::new(),
        report: ReconcileReport::default(),
    };
    for (index, key) in new.iter().enumerate() {
        if !seen.insert(key) {
            plan.duplicates.push(key.clone());
            continue;
        }
        let reuse = by_key.get(key).copied().filter(|old| reusable(*old, index));
        match reuse {
            Some(old) => {
                kept[old] = true;
                plan.report.reused += 1;
            }
            None => plan.report.created += 1,
        }
        plan.children.push(KeyedChild {
            key: key.clone(),
            index,
            reuse,
        });
    }
    plan.removed = (0..old.len()).filter(|index| !kept[*index]).collect();
    plan.report.removed = plan.removed.len();
    let order: Vec<usize> = plan
        .children
        .iter()
        .filter_map(|child| child.reuse)
        .collect();
    plan.report.moved = order.len() - longest_increasing(&order);
    plan
}

/// The length of the longest strictly increasing subsequence: the number
/// of kept children that can stay where they are.
fn longest_increasing(values: &[usize]) -> usize {
    let mut tails: Vec<usize> = Vec::new();
    for value in values {
        let at = tails.partition_point(|tail| tail < value);
        if at == tails.len() {
            tails.push(*value);
        } else {
            tails[at] = *value;
        }
    }
    tails.len()
}

/// The node, control kind, and properties of one key.
struct Entry {
    id: NodeId,
    kind: Option<ControlKind>,
    properties: Properties,
}

/// The state of one [`Reconciler::reconcile`] call.
struct Pass<'a> {
    /// The key of each node before the call.
    keys: &'a HashMap<NodeId, Key>,
    /// The old nodes of keys whose control kind changed.
    replaced: Vec<NodeId>,
    report: ReconcileReport,
}

/// Applies keyed `Element` descriptions to a `Tree`. A key is unique in
/// the whole description, so an element keeps its node when it moves to
/// a different parent. Each child list goes through [`plan_keyed`].
#[derive(Default)]
pub struct Reconciler {
    nodes: HashMap<Key, Entry>,
}

impl Reconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_id(&self, key: &Key) -> Option<NodeId> {
        self.nodes.get(key).map(|entry| entry.id)
    }

    /// The control kind of the element with `key`.
    pub fn kind(&self, key: &Key) -> Option<ControlKind> {
        self.nodes.get(key).and_then(|entry| entry.kind)
    }

    /// The properties of the element with `key`.
    pub fn properties(&self, key: &Key) -> Option<&Properties> {
        self.nodes.get(key).map(|entry| &entry.properties)
    }

    /// Makes the children of the tree root follow `elements`.
    ///
    /// - A kept key keeps its `NodeId`, focus, and semantics.
    /// - A kept key whose control kind changed gets a new node. The report
    ///   counts it as removed and created.
    /// - A kept key whose properties changed keeps its node. The report
    ///   counts it as updated.
    ///
    /// A duplicate key or invalid bounds return an error before the tree
    /// changes.
    pub fn reconcile(
        &mut self,
        tree: &mut Tree,
        elements: &[Element],
    ) -> Result<ReconcileReport, ComposeError> {
        let desired = validate(elements)?;
        let keys: HashMap<NodeId, Key> = self
            .nodes
            .iter()
            .map(|(key, entry)| (entry.id, key.clone()))
            .collect();
        let mut pass = Pass {
            keys: &keys,
            replaced: Vec::new(),
            report: ReconcileReport::default(),
        };
        self.place_elements(tree, tree.root(), elements, &mut pass)?;
        // The kept descendants of the replaced nodes are already under
        // their new parents.
        for id in pass.replaced {
            if tree.contains(id) {
                tree.detach(id)?;
            }
        }
        let removed: Vec<Key> = self
            .nodes
            .keys()
            .filter(|key| !desired.contains(*key))
            .cloned()
            .collect();
        for key in &removed {
            if let Some(entry) = self.nodes.remove(key)
                && tree.contains(entry.id)
            {
                tree.detach(entry.id)?;
            }
        }
        let mut report = pass.report;
        report.removed += removed.len();
        Ok(report)
    }

    fn place_elements(
        &mut self,
        tree: &mut Tree,
        parent: NodeId,
        elements: &[Element],
        pass: &mut Pass<'_>,
    ) -> Result<(), ComposeError> {
        let old: Vec<Key> = tree
            .children(parent)
            .unwrap_or_default()
            .iter()
            .filter_map(|id| pass.keys.get(id).cloned())
            .collect();
        let new: Vec<Key> = elements.iter().map(|element| element.key.clone()).collect();
        let plan = plan_keyed(&old, &new, |old_index, new_index| {
            self.nodes
                .get(&old[old_index])
                .is_some_and(|entry| entry.kind == elements[new_index].kind)
        });
        pass.report.moved += plan.report.moved;
        for (index, child) in plan.children.iter().enumerate() {
            let element = &elements[child.index];
            let id = self.place_one(tree, parent, element, child.reuse.is_some(), pass)?;
            let is_placed = tree
                .children(parent)
                .and_then(|children| children.get(index))
                == Some(&id);
            if !is_placed {
                tree.place_child(parent, id, index)?;
            }
            tree.set_bounds(id, element.bounds)?;
            tree.set_focusable(id, element.focusable)?;
            self.place_elements(tree, id, &element.children, pass)?;
        }
        Ok(())
    }

    /// The node of `element`: its kept node, or a new node under `parent`.
    /// `in_list` says whether the plan kept the node in this child list.
    fn place_one(
        &mut self,
        tree: &mut Tree,
        parent: NodeId,
        element: &Element,
        in_list: bool,
        pass: &mut Pass<'_>,
    ) -> Result<NodeId, ComposeError> {
        if let Some(entry) = self.nodes.get_mut(&element.key) {
            if entry.kind == element.kind {
                pass.report.reused += 1;
                if !in_list {
                    // The element moved from a different parent.
                    pass.report.moved += 1;
                }
                if entry.properties != element.properties {
                    entry.properties = element.properties.clone();
                    pass.report.updated += 1;
                }
                return Ok(entry.id);
            }
            pass.replaced.push(entry.id);
            pass.report.removed += 1;
        }
        let id = tree.attach(parent, element.bounds)?;
        self.nodes.insert(
            element.key.clone(),
            Entry {
                id,
                kind: element.kind,
                properties: element.properties.clone(),
            },
        );
        pass.report.created += 1;
        Ok(id)
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
