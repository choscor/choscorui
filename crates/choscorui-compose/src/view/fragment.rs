//! Fragments record which of a node's children each view produced, so a
//! view that changes its structure later (a `Dynamic` branch or a
//! `ForEach` item) can put its new nodes back at the same position among
//! its siblings.

use choscorui_core::{NodeId, Tree};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FragmentId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Item {
    Node(NodeId),
    Fragment(FragmentId),
}

struct Fragment {
    parent: NodeId,
    items: Vec<Item>,
    owner: Option<FragmentId>,
}

#[derive(Default)]
pub(crate) struct Fragments {
    map: HashMap<FragmentId, Fragment>,
    roots: HashMap<NodeId, FragmentId>,
    /// The fragment that directly holds each node.
    owners: HashMap<NodeId, FragmentId>,
    next: u64,
}

impl Fragments {
    fn allocate(&mut self, fragment: Fragment) -> FragmentId {
        let id = FragmentId(self.next);
        self.next += 1;
        self.map.insert(id, fragment);
        id
    }

    /// The fragment that holds the children of `node`.
    pub(crate) fn root_of(&mut self, node: NodeId) -> FragmentId {
        if let Some(id) = self.roots.get(&node) {
            return *id;
        }
        let id = self.allocate(Fragment {
            parent: node,
            items: Vec::new(),
            owner: None,
        });
        self.roots.insert(node, id);
        id
    }

    /// A new fragment at the end of `owner`.
    pub(crate) fn child(&mut self, owner: FragmentId) -> FragmentId {
        let parent = self.map[&owner].parent;
        let id = self.allocate(Fragment {
            parent,
            items: Vec::new(),
            owner: Some(owner),
        });
        if let Some(fragment) = self.map.get_mut(&owner) {
            fragment.items.push(Item::Fragment(id));
        }
        id
    }

    /// The node whose children the fragment holds.
    pub(crate) fn parent(&self, fragment: FragmentId) -> Option<NodeId> {
        self.map.get(&fragment).map(|fragment| fragment.parent)
    }

    /// Records `node` at the end of `fragment`.
    pub(crate) fn push_node(&mut self, fragment: FragmentId, node: NodeId) {
        if let Some(entry) = self.map.get_mut(&fragment) {
            entry.items.push(Item::Node(node));
            self.owners.insert(node, fragment);
        }
    }

    /// Puts `new` in the place of `old` in the fragment that holds `old`.
    pub(crate) fn replace_node(&mut self, old: NodeId, new: NodeId) {
        let Some(fragment) = self.owners.remove(&old) else {
            return;
        };
        if let Some(entry) = self.map.get_mut(&fragment) {
            for item in &mut entry.items {
                if *item == Item::Node(old) {
                    *item = Item::Node(new);
                }
            }
            self.owners.insert(new, fragment);
        }
    }

    /// Makes `fragment` hold the children of `node`, after its nodes moved
    /// under `node`; `node` takes the fragment's place in its owner.
    pub(crate) fn rehome(&mut self, fragment: FragmentId, node: NodeId) {
        if let Some(owner) = self.map.get_mut(&fragment).and_then(|f| f.owner.take())
            && let Some(entry) = self.map.get_mut(&owner)
        {
            for item in &mut entry.items {
                if *item == Item::Fragment(fragment) {
                    *item = Item::Node(node);
                }
            }
            self.owners.insert(node, owner);
        }
        self.set_parent(fragment, node);
        self.roots.insert(node, fragment);
    }

    fn set_parent(&mut self, fragment: FragmentId, node: NodeId) {
        if let Some(entry) = self.map.get_mut(&fragment) {
            entry.parent = node;
        }
        for child in self.children(fragment) {
            self.set_parent(child, node);
        }
    }

    /// The index among the parent's children that a node appended to
    /// `fragment` takes. The node is already attached as the parent's
    /// last child.
    pub(crate) fn append_index(&self, tree: &Tree, fragment: FragmentId) -> usize {
        if self.at_end(fragment) {
            let parent = self.map[&fragment].parent;
            return tree
                .children(parent)
                .map_or(0, |children| children.len().saturating_sub(1));
        }
        self.offset(fragment) + self.count(fragment)
    }

    /// Whether the fragment and every owner are the last item of their
    /// owner, so appending to it appends to the parent.
    fn at_end(&self, fragment: FragmentId) -> bool {
        let mut current = fragment;
        while let Some(owner) = self.map.get(&current).and_then(|f| f.owner) {
            let last = self.map[&owner].items.last().copied();
            if last != Some(Item::Fragment(current)) {
                return false;
            }
            current = owner;
        }
        true
    }

    /// The number of nodes in the fragment, nested fragments included.
    fn count(&self, fragment: FragmentId) -> usize {
        self.map.get(&fragment).map_or(0, |fragment| {
            fragment
                .items
                .iter()
                .map(|item| match item {
                    Item::Node(_) => 1,
                    Item::Fragment(inner) => self.count(*inner),
                })
                .sum()
        })
    }

    /// The index of the fragment's first node among the parent's children.
    pub(crate) fn offset(&self, fragment: FragmentId) -> usize {
        let Some(owner) = self.map.get(&fragment).and_then(|f| f.owner) else {
            return 0;
        };
        let mut before = 0;
        for item in &self.map[&owner].items {
            match item {
                Item::Fragment(inner) if *inner == fragment => break,
                Item::Fragment(inner) => before += self.count(*inner),
                Item::Node(_) => before += 1,
            }
        }
        self.offset(owner) + before
    }

    /// The fragment's nodes in order, nested fragments included.
    pub(crate) fn nodes(&self, fragment: FragmentId) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.collect(fragment, &mut out);
        out
    }

    fn collect(&self, fragment: FragmentId, out: &mut Vec<NodeId>) {
        if let Some(fragment) = self.map.get(&fragment) {
            for item in &fragment.items {
                match item {
                    Item::Node(node) => out.push(*node),
                    Item::Fragment(inner) => self.collect(*inner, out),
                }
            }
        }
    }

    /// The fragments directly inside `fragment`, in order.
    pub(crate) fn children(&self, fragment: FragmentId) -> Vec<FragmentId> {
        self.map.get(&fragment).map_or_else(Vec::new, |fragment| {
            fragment
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Fragment(inner) => Some(*inner),
                    Item::Node(_) => None,
                })
                .collect()
        })
    }

    /// Puts the nested fragments of `fragment` in `order`. Nodes directly
    /// in the fragment keep their place before the nested fragments.
    pub(crate) fn reorder(&mut self, fragment: FragmentId, order: &[FragmentId]) {
        if let Some(fragment) = self.map.get_mut(&fragment) {
            fragment.items.retain(|item| matches!(item, Item::Node(_)));
            fragment
                .items
                .extend(order.iter().map(|inner| Item::Fragment(*inner)));
        }
    }

    /// Removes the fragment's items and nested fragments, and returns the
    /// nodes that it held, which the caller detaches.
    pub(crate) fn clear(&mut self, fragment: FragmentId) -> Vec<NodeId> {
        let nodes = self.nodes(fragment);
        let inner = self.children(fragment);
        for child in inner {
            self.drop_tree(child);
        }
        if let Some(fragment) = self.map.get_mut(&fragment) {
            fragment.items.clear();
        }
        nodes
    }

    /// Removes the fragment from its owner and returns its nodes.
    pub(crate) fn remove(&mut self, fragment: FragmentId) -> Vec<NodeId> {
        let nodes = self.clear(fragment);
        if let Some(owner) = self.map.get(&fragment).and_then(|f| f.owner)
            && let Some(owner) = self.map.get_mut(&owner)
        {
            owner.items.retain(|item| *item != Item::Fragment(fragment));
        }
        self.map.remove(&fragment);
        nodes
    }

    fn drop_tree(&mut self, fragment: FragmentId) {
        for child in self.children(fragment) {
            self.drop_tree(child);
        }
        self.map.remove(&fragment);
    }

    /// Forgets fragments whose parent node left the tree.
    pub(crate) fn prune(&mut self, tree: &Tree) {
        self.map
            .retain(|_, fragment| tree.contains(fragment.parent));
        self.roots.retain(|node, _| tree.contains(*node));
        self.owners.retain(|node, _| tree.contains(*node));
    }
}
