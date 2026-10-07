use choscorui_core::{
    NodeId, Rect, SemanticAction, SemanticChange, SemanticCollection, SemanticEntry, SemanticNode,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AccessibilityIdentity(pub(crate) u64);

#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityElement {
    pub identity: AccessibilityIdentity,
    pub node: NodeId,
    pub parent: Option<AccessibilityIdentity>,
    pub bounds: Rect,
    pub focused: bool,
    pub semantic: SemanticNode,
    pub collection: Option<SemanticCollection>,
    pub item_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessibilityNotification {
    LayoutChanged,
    LabelChanged(AccessibilityIdentity),
    ValueChanged(AccessibilityIdentity),
    StateChanged(AccessibilityIdentity),
    FocusChanged(AccessibilityIdentity),
    FocusCleared,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityUpdate {
    pub elements: Vec<AccessibilityElement>,
    pub retired: Vec<AccessibilityIdentity>,
    pub notifications: Vec<AccessibilityNotification>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibilityActionRequest {
    pub node: NodeId,
    pub action: SemanticAction,
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessibilityActionError {
    MissingElement,
    UnsupportedAction,
}

impl std::fmt::Display for AccessibilityActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingElement => f.write_str("the accessibility element no longer exists"),
            Self::UnsupportedAction => f.write_str("the element does not support this action"),
        }
    }
}

impl std::error::Error for AccessibilityActionError {}

#[derive(Default)]
pub struct AccessibilityProjection {
    next_identity: u64,
    identities: HashMap<NodeId, AccessibilityIdentity>,
    elements: HashMap<AccessibilityIdentity, AccessibilityElement>,
    actions: Vec<AccessibilityActionRequest>,
}

impl AccessibilityProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reconcile(
        &mut self,
        snapshot: &[SemanticEntry],
        changes: &[SemanticChange],
    ) -> AccessibilityUpdate {
        let live: HashSet<_> = snapshot.iter().map(|entry| entry.id).collect();
        let removed_nodes: Vec<_> = self
            .identities
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        let mut retired = Vec::with_capacity(removed_nodes.len());
        for node in removed_nodes {
            if let Some(identity) = self.identities.remove(&node) {
                self.elements.remove(&identity);
                retired.push(identity);
            }
        }
        retired.sort_by_key(|identity| identity.0);

        for entry in snapshot {
            self.identities.entry(entry.id).or_insert_with(|| {
                let identity = AccessibilityIdentity(self.next_identity);
                self.next_identity = self
                    .next_identity
                    .checked_add(1)
                    .expect("accessibility identity exhausted");
                identity
            });
        }
        let mut elements = Vec::with_capacity(snapshot.len());
        for entry in snapshot {
            let identity = self.identities[&entry.id];
            let element = AccessibilityElement {
                identity,
                node: entry.id,
                parent: entry
                    .parent
                    .and_then(|id| self.identities.get(&id).copied()),
                bounds: entry.bounds,
                focused: entry.focused,
                semantic: entry.node.clone(),
                collection: entry.collection,
                item_index: entry.item_index,
            };
            self.elements.insert(identity, element.clone());
            elements.push(element);
        }
        let notifications = changes
            .iter()
            .filter_map(|change| match *change {
                SemanticChange::None => None,
                SemanticChange::Tree => Some(AccessibilityNotification::LayoutChanged),
                SemanticChange::Label(node) => self
                    .identities
                    .get(&node)
                    .copied()
                    .map(AccessibilityNotification::LabelChanged),
                SemanticChange::Value(node) => self
                    .identities
                    .get(&node)
                    .copied()
                    .map(AccessibilityNotification::ValueChanged),
                SemanticChange::ValueAndState(node) => self
                    .identities
                    .get(&node)
                    .copied()
                    .map(AccessibilityNotification::ValueChanged),
                SemanticChange::State(node) => self
                    .identities
                    .get(&node)
                    .copied()
                    .map(AccessibilityNotification::StateChanged),
                SemanticChange::Focus {
                    new: Some(node), ..
                } => self
                    .identities
                    .get(&node)
                    .copied()
                    .map(AccessibilityNotification::FocusChanged),
                SemanticChange::Focus { new: None, .. } => {
                    Some(AccessibilityNotification::FocusCleared)
                }
            })
            .collect();
        AccessibilityUpdate {
            elements,
            retired,
            notifications,
        }
    }

    pub fn request_action(
        &mut self,
        identity: AccessibilityIdentity,
        action: SemanticAction,
    ) -> Result<(), AccessibilityActionError> {
        let element = self
            .elements
            .get(&identity)
            .ok_or(AccessibilityActionError::MissingElement)?;
        if !element.semantic.actions.contains(&action) {
            return Err(AccessibilityActionError::UnsupportedAction);
        }
        self.actions.push(AccessibilityActionRequest {
            node: element.node,
            action,
            value: None,
        });
        Ok(())
    }

    pub fn take_actions(&mut self) -> Vec<AccessibilityActionRequest> {
        std::mem::take(&mut self.actions)
    }
}
