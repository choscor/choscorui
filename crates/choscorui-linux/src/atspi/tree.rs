//! The accessible objects of one window and the events of a snapshot.
//!
//! Each published node keeps one identity, and thus one object path,
//! while it stays in the snapshot. A node that leaves the snapshot loses
//! its identity, and the identity is never given to another node.

use super::roles::{self, state};
use super::text;
use crate::dbus::Value;
use choscorui_core::{NodeId, SemanticEntry};
use std::collections::{HashMap, HashSet};

/// One live accessible node.
pub struct Node {
    pub entry: SemanticEntry,
    /// The identity of the parent, or `None` under the window.
    pub parent: Option<u64>,
    pub children: Vec<u64>,
}

/// An AT-SPI event that a snapshot causes, before it becomes a signal.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `object:children-changed:add` or `:remove` on `parent` (`None`
    /// is the window).
    Children {
        parent: Option<u64>,
        added: bool,
        index: usize,
        child: u64,
    },
    /// `object:state-changed:<state>`.
    State { node: u64, state: u32, on: bool },
    /// `object:property-change:<name>`.
    Property {
        node: u64,
        name: &'static str,
        value: Value,
    },
    /// `object:text-changed:insert` or `:delete`.
    Text {
        node: u64,
        inserted: bool,
        start: usize,
        text: String,
    },
    /// `object:text-caret-moved`.
    Caret { node: u64, offset: usize },
    /// `object:text-selection-changed`.
    TextSelection { node: u64 },
    /// `focus:` on the newly focused node.
    Focus { node: u64 },
}

/// The accessible nodes of one window.
#[derive(Default)]
pub struct WindowTree {
    next_identity: u64,
    identities: HashMap<(NodeId, Option<u64>), u64>,
    pub nodes: HashMap<u64, Node>,
    /// The nodes directly under the window, in snapshot order.
    pub roots: Vec<u64>,
    pub focused: Option<u64>,
}

impl WindowTree {
    /// The identity of the node `id` itself, not of a part.
    pub fn identity(&self, id: NodeId) -> Option<u64> {
        self.identities.get(&(id, None)).copied()
    }

    /// The children of `parent`, or the roots for `None`.
    pub fn children(&self, parent: Option<u64>) -> &[u64] {
        match parent {
            None => &self.roots,
            Some(parent) => self
                .nodes
                .get(&parent)
                .map_or(&[][..], |node| &node.children),
        }
    }

    /// Replaces the nodes with `snapshot` and returns the events of the
    /// differences. The work grows with the snapshot, not with the model.
    pub fn reconcile(&mut self, snapshot: &[SemanticEntry]) -> Vec<Event> {
        let live: HashSet<(NodeId, Option<u64>)> = snapshot
            .iter()
            .map(|entry| (entry.id, entry.part))
            .collect();
        self.identities.retain(|key, _| live.contains(key));
        for entry in snapshot {
            let next = &mut self.next_identity;
            self.identities
                .entry((entry.id, entry.part))
                .or_insert_with(|| {
                    *next += 1;
                    *next
                });
        }
        let mut nodes: HashMap<u64, Node> = HashMap::with_capacity(snapshot.len());
        let mut roots = Vec::new();
        let mut order = Vec::with_capacity(snapshot.len());
        let mut focused = None;
        for entry in snapshot {
            let identity = self.identities[&(entry.id, entry.part)];
            let parent = entry
                .parent
                .and_then(|parent| self.identities.get(&(parent, entry.parent_part)).copied())
                .filter(|&parent| parent != identity);
            if entry.focused {
                focused = Some(identity);
            }
            order.push(identity);
            nodes.insert(
                identity,
                Node {
                    entry: entry.clone(),
                    parent,
                    children: Vec::new(),
                },
            );
        }
        for &identity in &order {
            let parent = nodes[&identity].parent;
            match parent.and_then(|parent| nodes.get_mut(&parent)) {
                Some(parent) => parent.children.push(identity),
                None => roots.push(identity),
            }
        }
        // A parent that left the snapshot makes its child a root.
        for &identity in &roots {
            if let Some(node) = nodes.get_mut(&identity) {
                node.parent = None;
            }
        }
        let mut events = Vec::new();
        for &identity in &order {
            if let Some(old) = self.nodes.get(&identity) {
                node_events(identity, &old.entry, &nodes[&identity].entry, &mut events);
            }
        }
        if focused != self.focused
            && let Some(node) = focused
        {
            events.push(Event::Focus { node });
        }
        // The children of each parent that lives on, the window included.
        let mut parents: Vec<Option<u64>> = vec![None];
        parents.extend(
            order
                .iter()
                .filter(|identity| self.nodes.contains_key(identity))
                .map(|&identity| Some(identity)),
        );
        for parent in parents {
            let old = self.children(parent);
            let new = match parent {
                None => &roots,
                Some(parent) => &nodes[&parent].children,
            };
            if old == new.as_slice() {
                continue;
            }
            let kept: HashSet<u64> = new.iter().copied().collect();
            for (index, &child) in old.iter().enumerate().rev() {
                if !kept.contains(&child) {
                    events.push(Event::Children {
                        parent,
                        added: false,
                        index,
                        child,
                    });
                }
            }
            let had: HashSet<u64> = old.iter().copied().collect();
            for (index, &child) in new.iter().enumerate() {
                if !had.contains(&child) {
                    events.push(Event::Children {
                        parent,
                        added: true,
                        index,
                        child,
                    });
                }
            }
        }
        self.nodes = nodes;
        self.roots = roots;
        self.focused = focused;
        events
    }
}

/// The events of the changes of one node that stays in the snapshot.
fn node_events(node: u64, old: &SemanticEntry, new: &SemanticEntry, events: &mut Vec<Event>) {
    let property = |name, value| Event::Property { node, name, value };
    if old.node.name != new.node.name {
        events.push(property(
            "accessible-name",
            Value::from(new.node.name.as_str()),
        ));
    }
    if old.node.description != new.node.description {
        events.push(property(
            "accessible-description",
            Value::from(new.node.description.as_str()),
        ));
    }
    let multiline = |entry: &SemanticEntry| entry.node.text.as_ref().is_some_and(|t| t.multiline);
    let old_role = roles::atspi_role(old.node.role, multiline(old));
    let new_role = roles::atspi_role(new.node.role, multiline(new));
    if old_role != new_role {
        events.push(property("accessible-role", Value::from(new_role.number)));
    }
    let old_states = roles::states(old);
    let new_states = roles::states(new);
    for changed in old_states.changed(new_states) {
        if !state::name(changed).is_empty() {
            events.push(Event::State {
                node,
                state: changed,
                on: new_states.contains(changed),
            });
        }
    }
    match (text::content(old), text::content(new)) {
        (Some(old_text), Some(new_text)) => {
            if let Some((start, removed, inserted)) = text::change(old_text, new_text) {
                if !removed.is_empty() {
                    events.push(Event::Text {
                        node,
                        inserted: false,
                        start,
                        text: removed,
                    });
                }
                if !inserted.is_empty() {
                    events.push(Event::Text {
                        node,
                        inserted: true,
                        start,
                        text: inserted,
                    });
                }
            }
            let (old_selection, new_selection) = (text::selection(old), text::selection(new));
            if old_selection != new_selection
                && let Some(selection) = new_selection
            {
                if old_selection.as_ref().map(|range| range.end) != Some(selection.end) {
                    events.push(Event::Caret {
                        node,
                        offset: selection.end,
                    });
                }
                let empty = |range: &Option<std::ops::Range<usize>>| {
                    range.as_ref().is_none_or(|range| range.is_empty())
                };
                if !(empty(&old_selection) && selection.is_empty()) {
                    events.push(Event::TextSelection { node });
                }
            }
        }
        _ => {
            if old.node.value != new.node.value || old.node.state.range != new.node.state.range {
                let value = match (new.node.state.range, &new.node.value) {
                    (Some(range), _) => Value::from(range.value),
                    (None, Some(value)) => Value::from(value.as_str()),
                    (None, None) => Value::from(""),
                };
                events.push(property("accessible-value", value));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choscorui_core::{
        Point, Rect, SemanticNode, SemanticRole, SemanticState, SemanticTree, Size, Tree,
    };

    fn rect() -> Rect {
        Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0))
    }

    fn node(role: SemanticRole, name: &str) -> SemanticNode {
        SemanticNode {
            role,
            name: name.to_owned(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    #[test]
    fn identities_last_while_nodes_live() {
        let mut tree = Tree::new(rect());
        let group = tree.attach(tree.root(), rect()).unwrap();
        let first = tree.attach(group, rect()).unwrap();
        let second = tree.attach(group, rect()).unwrap();
        let mut semantics = SemanticTree::new();
        semantics
            .set(group, node(SemanticRole::Group, "group"), &tree)
            .unwrap();
        semantics
            .set(first, node(SemanticRole::Checkbox, "first"), &tree)
            .unwrap();
        semantics
            .set(second, node(SemanticRole::Button, "second"), &tree)
            .unwrap();
        let mut window = WindowTree::default();
        let events = window.reconcile(&semantics.snapshot(&tree));
        let group_id = window.identity(group).unwrap();
        let first_id = window.identity(first).unwrap();
        let second_id = window.identity(second).unwrap();
        assert_eq!(window.roots, vec![group_id]);
        assert_eq!(window.children(Some(group_id)), &[first_id, second_id]);
        assert_eq!(
            events,
            vec![Event::Children {
                parent: None,
                added: true,
                index: 0,
                child: group_id
            }]
        );

        let mut checked = node(SemanticRole::Checkbox, "first");
        checked.state.checked = Some(true);
        semantics.set(first, checked, &tree).unwrap();
        tree.detach(second).unwrap();
        semantics.prune(&tree);
        let events = window.reconcile(&semantics.snapshot(&tree));
        assert_eq!(window.identity(first), Some(first_id));
        assert_eq!(window.identity(second), None);
        assert!(events.contains(&Event::State {
            node: first_id,
            state: state::CHECKED,
            on: true
        }));
        assert!(events.contains(&Event::Children {
            parent: Some(group_id),
            added: false,
            index: 1,
            child: second_id
        }));
        assert!(!window.nodes.contains_key(&second_id));
    }
}
