//! Platform-neutral part of the macOS accessibility bridge.
//!
//! [`AccessibilityProjection`] reconciles each `SemanticTree` snapshot into
//! [`AccessibilityElement`]s. A node keeps the same [`AccessibilityIdentity`]
//! while it stays in the snapshot. A node that leaves the snapshot is
//! retired, and its identity is not used again. The projection also maps
//! each `SemanticChange` to an [`AccessibilityNotification`].
//!
//! [`Window::set_accessibility`](crate::Window::set_accessibility) applies an
//! [`AccessibilityUpdate`] to native `NSAccessibilityElement` objects. It
//! maps each `SemanticRole` to an AppKit role, sets the attributes, and posts
//! the AppKit notifications. Assistive-technology actions come back as
//! [`AccessibilityActionRequest`]s through
//! [`Window::take_accessibility_actions`](crate::Window::take_accessibility_actions).
//! The bridge queues an action only when the node declares it.
//!
//! The `choscorui-macos` README describes the role mapping, the
//! notifications, and the action callbacks.
#![deny(unsafe_code)]

use choscorui_core::{
    NodeId, Rect, SemanticAction, SemanticChange, SemanticCollection, SemanticEntry, SemanticNode,
};
use std::collections::{HashMap, HashSet};

/// Stable identity of one native accessibility element. It does not change
/// while its node stays in the snapshot, and a retired identity is never
/// given to a different node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AccessibilityIdentity(pub(crate) u64);

/// One semantic node as the native element sees it: identity, parent,
/// bounds, focus, semantics, and the collection data of virtualized views.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityElement {
    pub identity: AccessibilityIdentity,
    pub node: NodeId,
    pub parent: Option<AccessibilityIdentity>,
    pub bounds: Rect,
    pub focused: bool,
    pub semantic: SemanticNode,
    /// Full item and column count of a virtualized collection. It becomes
    /// `accessibilityRowCount` and `accessibilityColumnCount`.
    pub collection: Option<SemanticCollection>,
    /// Absolute index of a realized item. It becomes `accessibilityIndex`.
    pub item_index: Option<usize>,
}

/// A change that the window posts as an AppKit accessibility notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessibilityNotification {
    /// Nodes were added, removed, or moved. Posted on the content view as
    /// `NSAccessibilityLayoutChangedNotification`.
    LayoutChanged,
    /// Posted as `NSAccessibilityTitleChangedNotification`.
    LabelChanged(AccessibilityIdentity),
    /// Posted as `NSAccessibilityValueChangedNotification`. Text elements
    /// also get `NSAccessibilitySelectedTextChangedNotification`.
    ValueChanged(AccessibilityIdentity),
    /// Posted the same way as [`AccessibilityNotification::ValueChanged`].
    StateChanged(AccessibilityIdentity),
    /// Posted as `NSAccessibilityFocusedUIElementChangedNotification`.
    FocusChanged(AccessibilityIdentity),
    /// Focus left every element. Posted on the content view as
    /// `NSAccessibilityFocusedUIElementChangedNotification`.
    FocusCleared,
}

/// The result of one reconciliation: the live elements in snapshot order,
/// the retired identities, and the notifications to post.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityUpdate {
    pub elements: Vec<AccessibilityElement>,
    pub retired: Vec<AccessibilityIdentity>,
    pub notifications: Vec<AccessibilityNotification>,
}

/// An action that an assistive technology asked for on a node. The
/// application routes it into the same control input as a key or pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibilityActionRequest {
    pub node: NodeId,
    pub action: SemanticAction,
    /// The new value for [`SemanticAction::SetValue`], as text.
    pub value: Option<String>,
}

/// Why [`AccessibilityProjection::request_action`] refused an action.
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

/// Keeps the identity of each semantic node across snapshots and queues
/// action requests.
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

    /// Reconciles `snapshot` with the previous one. New nodes get a new
    /// identity, nodes that are still present keep theirs, and absent nodes
    /// are retired. `changes` become notifications for live elements.
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

    /// Queues `action` for the element. It fails when the element is retired
    /// or when its node does not declare the action.
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

    /// Removes and returns the queued action requests, oldest first.
    pub fn take_actions(&mut self) -> Vec<AccessibilityActionRequest> {
        std::mem::take(&mut self.actions)
    }
}
