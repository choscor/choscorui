//! Semantic parts: the child nodes of a control that has no retained node
//! for each of its parts, such as menu items, tabs, buttons, or cells.

use crate::{Rect, SemanticCollection, SemanticNode, SemanticRole, SemanticState};

/// The key bit of a part that is an item of a collection. The rest of the
/// key is the absolute item index, so the part keeps its identity while the
/// visible range moves.
const ITEM_KEY: u64 = 1 << 48;

/// One part of a node, published as a child semantic node.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticPart {
    /// The identity of the part inside its node. It is the position in the
    /// list, or the item index of a collection item.
    pub key: u64,
    /// The position of the part that contains this one, or `None` for a
    /// part directly under the node.
    pub parent: Option<usize>,
    /// The bounds in window coordinates.
    pub bounds: Rect,
    pub node: SemanticNode,
    /// The full size of the collection that this part contains.
    pub collection: Option<SemanticCollection>,
    /// The absolute position of the part in its collection.
    pub item_index: Option<usize>,
}

impl SemanticPart {
    /// Marks the part as item `index` of its collection. The key becomes
    /// the item index, so the part keeps its identity when the visible
    /// range moves.
    pub fn item(&mut self, index: usize) -> &mut Self {
        self.item_index = Some(index);
        self.key = ITEM_KEY | index as u64;
        self
    }
}

/// The parts of one node, in order, and the part that has the keyboard
/// focus. A control fills it in
/// `CustomControl::semantic_parts`; the semantic tree publishes each part
/// as a child node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticParts {
    parts: Vec<SemanticPart>,
    focused: Option<usize>,
    collection: Option<SemanticCollection>,
}

impl SemanticParts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a part with `role`, `name`, and `bounds` under `parent`, and
    /// returns it. Its position is `len() - 1`. A `parent` that is not an
    /// earlier part puts the part directly under the node.
    pub fn push(
        &mut self,
        parent: Option<usize>,
        role: SemanticRole,
        name: &str,
        bounds: Rect,
    ) -> &mut SemanticPart {
        let key = self.parts.len();
        self.parts.push(SemanticPart {
            key: key as u64,
            parent: parent.filter(|parent| *parent < key),
            bounds,
            node: SemanticNode {
                role,
                name: name.to_owned(),
                description: String::new(),
                value: None,
                state: SemanticState::default(),
                relations: Vec::new(),
                actions: Vec::new(),
                text: None,
            },
            collection: None,
            item_index: None,
        });
        &mut self.parts[key]
    }

    /// Gives the keyboard focus to the part at `position`, or to the node
    /// itself with `None`. A position past the end clears it.
    pub fn set_focused(&mut self, position: Option<usize>) {
        self.focused = position.filter(|position| *position < self.parts.len());
    }

    /// The position of the part that has the keyboard focus.
    pub fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Gives the node itself the full size of the collection that its
    /// parts are items of, for a node that adds only the items in view.
    pub fn set_collection(&mut self, collection: SemanticCollection) {
        self.collection = Some(collection);
    }

    /// The collection of the node, from [`Self::set_collection`].
    pub fn collection(&self) -> Option<SemanticCollection> {
        self.collection
    }

    pub fn parts(&self) -> &[SemanticPart] {
        &self.parts
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Marks every part disabled and removes its actions, for a disabled
    /// node.
    pub fn disable(&mut self) {
        for part in &mut self.parts {
            part.node.state.disabled = true;
            part.node.actions.clear();
        }
    }

    /// The part with `key`.
    pub fn get(&self, key: u64) -> Option<&SemanticPart> {
        self.parts.iter().find(|part| part.key == key)
    }

    /// The key of the focused part.
    pub(crate) fn focused_key(&self) -> Option<u64> {
        self.focused
            .and_then(|position| self.parts.get(position))
            .map(|part| part.key)
    }
}
