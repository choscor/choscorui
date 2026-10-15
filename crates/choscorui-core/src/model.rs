//! Item models, stable item identity, change notifications, and selection.
//!
//! Views never store row numbers as identity. Every item has an [`ItemKey`]
//! that survives insertion, removal, sorting, and filtering, and selection is
//! a set of keys. Rows are positions in the model's current order.

use std::collections::{BTreeSet, HashMap, VecDeque};

/// Stable identity of a model item. Keys are unique within one model and
/// never reused while the model lives.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemKey(pub u64);

/// Which aspect of an item a view requests.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    /// Text shown to the user.
    Display,
    /// Value placed in an editor.
    Edit,
    /// Value used by sort proxies; falls back to `Edit`, then `Display`.
    Sort,
    ToolTip,
    /// Accessible description when it differs from the display text.
    Accessibility,
    Check,
    Icon,
    Custom(u32),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckState {
    Unchecked,
    Mixed,
    Checked,
}

/// Owned item data. `Float` uses a total order for sorting.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Check(CheckState),
    /// A resource name resolved through the application's resource bundle.
    Icon(String),
}

impl Value {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) | Self::Icon(text) => Some(text),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Display string for views without a custom delegate.
    pub fn to_display_string(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Text(text) | Self::Icon(text) => text.clone(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Check(state) => format!("{state:?}"),
        }
    }

    /// Total order used by sorting: None < Bool < numbers < Text.
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(value: &Value) -> u8 {
            match value {
                Value::None => 0,
                Value::Bool(_) | Value::Check(_) => 1,
                Value::Int(_) | Value::Float(_) => 2,
                Value::Text(_) | Value::Icon(_) => 3,
            }
        }
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            (Self::Int(a), Self::Float(b)) => (*a as f64).total_cmp(b),
            (Self::Float(a), Self::Int(b)) => a.total_cmp(&(*b as f64)),
            (Self::Float(a), Self::Float(b)) => a.total_cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) | (Self::Icon(a), Self::Icon(b)) => a.cmp(b),
            (Self::Check(a), Self::Check(b)) => (*a as u8).cmp(&(*b as u8)),
            _ => rank(self).cmp(&rank(other)),
        }
    }
}

/// One incremental change, expressed by key so consumers never act on a
/// stale row index.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelChange {
    /// Everything may have changed; views rebuild and drop missing keys.
    Reset,
    Inserted {
        parent: Option<ItemKey>,
        first_row: usize,
        keys: Vec<ItemKey>,
    },
    Removed {
        parent: Option<ItemKey>,
        keys: Vec<ItemKey>,
    },
    /// Order of siblings under `parent` changed; keys are unchanged.
    Reordered {
        parent: Option<ItemKey>,
    },
    DataChanged {
        key: ItemKey,
        roles: Vec<Role>,
    },
    HeaderChanged {
        column: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelError {
    MissingItem,
    InvalidRow,
    InvalidColumn,
    ReadOnly,
    InvalidValue,
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingItem => f.write_str("the item is not in the model"),
            Self::InvalidRow => f.write_str("the row is out of range"),
            Self::InvalidColumn => f.write_str("the column is out of range"),
            Self::ReadOnly => f.write_str("the item is not editable"),
            Self::InvalidValue => f.write_str("the value has the wrong type for this item"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Bounded log of changes. Consumers remember the last revision they saw.
#[derive(Clone, Debug)]
pub struct ChangeLog {
    revision: u64,
    entries: VecDeque<(u64, ModelChange)>,
    capacity: usize,
}

impl Default for ChangeLog {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl ChangeLog {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            revision: 0,
            entries: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn record(&mut self, change: ModelChange) {
        self.revision += 1;
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((self.revision, change));
    }

    /// Changes after `revision`, oldest first. A consumer that fell behind
    /// the bounded log receives a single [`ModelChange::Reset`].
    pub fn since(&self, revision: u64) -> Vec<ModelChange> {
        if revision >= self.revision {
            return Vec::new();
        }
        let oldest = self.entries.front().map_or(self.revision, |(rev, _)| *rev);
        if revision + 1 < oldest {
            return vec![ModelChange::Reset];
        }
        self.entries
            .iter()
            .filter(|(rev, _)| *rev > revision)
            .map(|(_, change)| change.clone())
            .collect()
    }
}

/// Read access shared by list, table, and tree views and by proxies.
///
/// A flat list or table uses `parent == None` for every row.
pub trait ItemModel {
    fn row_count(&self, parent: Option<ItemKey>) -> usize;
    fn column_count(&self) -> usize {
        1
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey>;
    fn parent(&self, key: ItemKey) -> Option<ItemKey>;
    /// Row of `key` among its siblings, or None if the key is gone.
    fn row(&self, key: ItemKey) -> Option<usize>;
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value;
    fn header(&self, _column: usize, _role: Role) -> Value {
        Value::None
    }
    fn has_children(&self, key: ItemKey) -> bool {
        self.row_count(Some(key)) > 0
    }
    fn editable(&self, _key: ItemKey, _column: usize) -> bool {
        false
    }
    /// Revision of this model's change log.
    fn revision(&self) -> u64;
    fn changes_since(&self, revision: u64) -> Vec<ModelChange>;
    /// Whether more children can be loaded lazily under `parent`.
    fn can_fetch_more(&self, _parent: Option<ItemKey>) -> bool {
        false
    }
}

/// Mutation shared by editable models and the data-widget mapper.
pub trait EditableModel: ItemModel {
    fn set_data(
        &mut self,
        key: ItemKey,
        column: usize,
        role: Role,
        value: Value,
    ) -> Result<(), ModelError>;
    fn fetch_more(&mut self, _parent: Option<ItemKey>) {}
}

/// How a row type presents itself to an owned store.
pub trait ItemData {
    fn data(&self, column: usize, role: Role) -> Value;
    fn set_data(&mut self, _column: usize, _role: Role, _value: Value) -> Result<(), ModelError> {
        Err(ModelError::ReadOnly)
    }
    fn editable(&self, _column: usize) -> bool {
        false
    }
}

impl ItemData for String {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Edit | Role::Sort | Role::Accessibility) => {
                Value::Text(self.clone())
            }
            _ => Value::None,
        }
    }
    fn set_data(&mut self, column: usize, role: Role, value: Value) -> Result<(), ModelError> {
        match (column, role, value) {
            (0, Role::Edit | Role::Display, Value::Text(text)) => {
                *self = text;
                Ok(())
            }
            (0, _, _) => Err(ModelError::InvalidValue),
            _ => Err(ModelError::InvalidColumn),
        }
    }
    fn editable(&self, column: usize) -> bool {
        column == 0
    }
}

struct TreeEntry<T> {
    value: T,
    parent: Option<ItemKey>,
    children: Vec<ItemKey>,
}

/// Owned hierarchical store; a list store is a tree with only top-level rows.
pub struct TreeStore<T> {
    entries: HashMap<ItemKey, TreeEntry<T>>,
    roots: Vec<ItemKey>,
    next_key: u64,
    columns: usize,
    headers: Vec<String>,
    log: ChangeLog,
}

impl<T: ItemData> Default for TreeStore<T> {
    fn default() -> Self {
        Self::new(1)
    }
}

impl<T: ItemData> TreeStore<T> {
    pub fn new(columns: usize) -> Self {
        Self {
            entries: HashMap::new(),
            roots: Vec::new(),
            next_key: 1,
            columns: columns.max(1),
            headers: Vec::new(),
            log: ChangeLog::default(),
        }
    }

    pub fn set_headers(&mut self, headers: impl IntoIterator<Item = impl Into<String>>) {
        self.headers = headers.into_iter().map(Into::into).collect();
        for column in 0..self.columns {
            self.log.record(ModelChange::HeaderChanged { column });
        }
    }

    fn siblings(&self, parent: Option<ItemKey>) -> Option<&Vec<ItemKey>> {
        match parent {
            None => Some(&self.roots),
            Some(key) => self.entries.get(&key).map(|entry| &entry.children),
        }
    }

    fn siblings_mut(&mut self, parent: Option<ItemKey>) -> Option<&mut Vec<ItemKey>> {
        match parent {
            None => Some(&mut self.roots),
            Some(key) => self.entries.get_mut(&key).map(|entry| &mut entry.children),
        }
    }

    pub fn insert(
        &mut self,
        parent: Option<ItemKey>,
        row: usize,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Vec<ItemKey>, ModelError> {
        let len = self.siblings(parent).ok_or(ModelError::MissingItem)?.len();
        if row > len {
            return Err(ModelError::InvalidRow);
        }
        let mut keys = Vec::new();
        for value in values {
            let key = ItemKey(self.next_key);
            self.next_key += 1;
            self.entries.insert(
                key,
                TreeEntry {
                    value,
                    parent,
                    children: Vec::new(),
                },
            );
            keys.push(key);
        }
        if keys.is_empty() {
            return Ok(keys);
        }
        let siblings = self.siblings_mut(parent).expect("parent validated");
        siblings.splice(row..row, keys.iter().copied());
        self.log.record(ModelChange::Inserted {
            parent,
            first_row: row,
            keys: keys.clone(),
        });
        Ok(keys)
    }

    pub fn push(&mut self, parent: Option<ItemKey>, value: T) -> Result<ItemKey, ModelError> {
        let row = self.row_count(parent);
        Ok(self.insert(parent, row, [value])?[0])
    }

    /// Removes items and their descendants.
    pub fn remove(&mut self, keys: &[ItemKey]) -> Result<(), ModelError> {
        if keys.iter().any(|key| !self.entries.contains_key(key)) {
            return Err(ModelError::MissingItem);
        }
        let mut by_parent: Vec<(Option<ItemKey>, Vec<ItemKey>)> = Vec::new();
        for key in keys {
            let Some(entry) = self.entries.get(key) else {
                continue; // already removed as a descendant
            };
            let parent = entry.parent;
            if let Some(siblings) = self.siblings_mut(parent) {
                siblings.retain(|sibling| sibling != key);
            }
            let mut pending = vec![*key];
            while let Some(current) = pending.pop() {
                if let Some(entry) = self.entries.remove(&current) {
                    pending.extend(entry.children);
                }
            }
            match by_parent.iter_mut().find(|(owner, _)| *owner == parent) {
                Some((_, removed)) => removed.push(*key),
                None => by_parent.push((parent, vec![*key])),
            }
        }
        for (parent, keys) in by_parent {
            self.log.record(ModelChange::Removed { parent, keys });
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.roots.clear();
        self.log.record(ModelChange::Reset);
    }

    pub fn get(&self, key: ItemKey) -> Option<&T> {
        self.entries.get(&key).map(|entry| &entry.value)
    }

    /// Mutates an item and reports the given roles as changed.
    pub fn update(
        &mut self,
        key: ItemKey,
        roles: Vec<Role>,
        change: impl FnOnce(&mut T),
    ) -> Result<(), ModelError> {
        let entry = self.entries.get_mut(&key).ok_or(ModelError::MissingItem)?;
        change(&mut entry.value);
        self.log.record(ModelChange::DataChanged { key, roles });
        Ok(())
    }

    /// Moves `key` to `row` under `parent`, keeping its identity.
    pub fn move_item(
        &mut self,
        key: ItemKey,
        parent: Option<ItemKey>,
        row: usize,
    ) -> Result<(), ModelError> {
        let old_parent = self
            .entries
            .get(&key)
            .ok_or(ModelError::MissingItem)?
            .parent;
        let mut ancestor = parent;
        while let Some(current) = ancestor {
            if current == key {
                return Err(ModelError::InvalidRow);
            }
            ancestor = self
                .entries
                .get(&current)
                .ok_or(ModelError::MissingItem)?
                .parent;
        }
        let target_len = self.siblings(parent).ok_or(ModelError::MissingItem)?.len()
            - usize::from(old_parent == parent);
        if row > target_len {
            return Err(ModelError::InvalidRow);
        }
        self.siblings_mut(old_parent)
            .expect("old parent exists")
            .retain(|sibling| *sibling != key);
        self.siblings_mut(parent)
            .expect("parent validated")
            .insert(row, key);
        self.entries.get_mut(&key).expect("validated").parent = parent;
        if old_parent == parent {
            self.log.record(ModelChange::Reordered { parent });
        } else {
            self.log.record(ModelChange::Removed {
                parent: old_parent,
                keys: vec![key],
            });
            self.log.record(ModelChange::Inserted {
                parent,
                first_row: row,
                keys: vec![key],
            });
        }
        Ok(())
    }

    /// Reorders siblings by a key function, preserving identity.
    pub fn sort_children_by<K: Ord>(&mut self, parent: Option<ItemKey>, key: impl Fn(&T) -> K) {
        let Some(children) = self.siblings(parent).cloned() else {
            return;
        };
        let mut sorted = children.clone();
        sorted.sort_by_key(|child| key(&self.entries[child].value));
        if sorted != children {
            *self.siblings_mut(parent).expect("exists") = sorted;
            self.log.record(ModelChange::Reordered { parent });
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T: ItemData> ItemModel for TreeStore<T> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.siblings(parent).map_or(0, Vec::len)
    }
    fn column_count(&self) -> usize {
        self.columns
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.siblings(parent)?.get(row).copied()
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.entries.get(&key)?.parent
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        let parent = self.entries.get(&key)?.parent;
        self.siblings(parent)?.iter().position(|item| *item == key)
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        if column >= self.columns {
            return Value::None;
        }
        self.entries
            .get(&key)
            .map_or(Value::None, |entry| entry.value.data(column, role))
    }
    fn header(&self, column: usize, role: Role) -> Value {
        match role {
            Role::Display | Role::Accessibility => self
                .headers
                .get(column)
                .map_or(Value::None, |text| Value::Text(text.clone())),
            _ => Value::None,
        }
    }
    fn editable(&self, key: ItemKey, column: usize) -> bool {
        self.entries
            .get(&key)
            .is_some_and(|entry| entry.value.editable(column))
    }
    fn revision(&self) -> u64 {
        self.log.revision()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.log.since(revision)
    }
}

impl<T: ItemData> EditableModel for TreeStore<T> {
    fn set_data(
        &mut self,
        key: ItemKey,
        column: usize,
        role: Role,
        value: Value,
    ) -> Result<(), ModelError> {
        if column >= self.columns {
            return Err(ModelError::InvalidColumn);
        }
        let entry = self.entries.get_mut(&key).ok_or(ModelError::MissingItem)?;
        if !entry.value.editable(column) {
            return Err(ModelError::ReadOnly);
        }
        entry.value.set_data(column, role, value)?;
        let roles = if role == Role::Edit {
            vec![Role::Edit, Role::Display]
        } else {
            vec![role]
        };
        self.log.record(ModelChange::DataChanged { key, roles });
        Ok(())
    }
}

/// Flat list of owned rows.
pub type ListStore<T> = TreeStore<T>;
/// Convenience string list (`QStringListModel` equivalent).
pub type StringListModel = TreeStore<String>;

impl StringListModel {
    pub fn from_strings(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut model = Self::new(1);
        let values: Vec<String> = values.into_iter().map(Into::into).collect();
        model.insert(None, 0, values).expect("root insert is valid");
        model
    }
}

/// How a click or key extends the selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMode {
    None,
    Single,
    /// Shift extends from the anchor and Command toggles one item.
    Extended,
    /// Every activation toggles one item.
    Multi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionCommand {
    /// Replace the selection with one item and move the anchor there.
    Replace,
    Toggle,
    /// Extend from the anchor in the model's current order.
    ExtendTo,
    /// Move the current item without changing the selection.
    CurrentOnly,
}

/// Keyed selection shared by views. Because it stores keys, it survives
/// insertion, sorting, filtering, and reorder of the underlying model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    selected: BTreeSet<ItemKey>,
    anchor: Option<ItemKey>,
    current: Option<ItemKey>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<ItemKey> {
        self.current
    }

    pub fn anchor(&self) -> Option<ItemKey> {
        self.anchor
    }

    pub fn is_selected(&self, key: ItemKey) -> bool {
        self.selected.contains(&key)
    }

    pub fn len(&self) -> usize {
        self.selected.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = ItemKey> + '_ {
        self.selected.iter().copied()
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Applies one command using `order` (keys in the view's current visible
    /// order) to resolve ranges. Returns true when selection changed.
    ///
    /// A range finds the anchor with a scan of `order`. A view that keeps
    /// an index from key to row uses [`Selection::apply_indexed`].
    pub fn apply(
        &mut self,
        mode: SelectionMode,
        command: SelectionCommand,
        key: ItemKey,
        order: &[ItemKey],
    ) -> bool {
        self.apply_indexed(mode, command, key, order, |item| {
            order.iter().position(|other| *other == item)
        })
    }

    /// Applies one command as [`Selection::apply`] does, with `position`
    /// giving the row of a key in `order`. A range then costs two lookups
    /// and the rows between them, not a scan of `order`.
    pub fn apply_indexed(
        &mut self,
        mode: SelectionMode,
        command: SelectionCommand,
        key: ItemKey,
        order: &[ItemKey],
        position: impl Fn(ItemKey) -> Option<usize>,
    ) -> bool {
        self.current = Some(key);
        match (mode, command) {
            (SelectionMode::None, _) | (_, SelectionCommand::CurrentOnly) => false,
            (SelectionMode::Single, _) | (SelectionMode::Extended, SelectionCommand::Replace) => {
                self.anchor = Some(key);
                self.replace(std::iter::once(key))
            }
            (SelectionMode::Multi, _) | (SelectionMode::Extended, SelectionCommand::Toggle) => {
                if !self.selected.remove(&key) {
                    self.selected.insert(key);
                }
                self.anchor = Some(key);
                true
            }
            (SelectionMode::Extended, SelectionCommand::ExtendTo) => {
                let anchor = self.anchor.unwrap_or(key);
                let range = position(anchor)
                    .zip(position(key))
                    .and_then(|(start, end)| order.get(start.min(end)..=start.max(end)));
                match range {
                    Some(keys) => {
                        self.anchor = Some(anchor);
                        self.replace(keys.iter().copied())
                    }
                    None => {
                        self.anchor = Some(key);
                        self.replace(std::iter::once(key))
                    }
                }
            }
        }
    }

    /// Replaces the selected keys with `keys`. Returns true when the set
    /// changed.
    fn replace(&mut self, keys: impl Iterator<Item = ItemKey>) -> bool {
        let mut next = BTreeSet::new();
        next.extend(keys);
        let changed = next != self.selected;
        self.selected = next;
        changed
    }

    pub fn select_all(&mut self, order: &[ItemKey]) -> bool {
        let before = self.selected.len();
        self.selected.extend(order.iter().copied());
        before != self.selected.len()
    }

    /// Drops keys that no longer exist. Returns removed keys.
    pub fn retain_existing(&mut self, exists: impl Fn(ItemKey) -> bool) -> Vec<ItemKey> {
        let removed: Vec<ItemKey> = self
            .selected
            .iter()
            .copied()
            .filter(|key| !exists(*key))
            .collect();
        for key in &removed {
            self.selected.remove(key);
        }
        if self.anchor.is_some_and(|key| !exists(key)) {
            self.anchor = None;
        }
        if self.current.is_some_and(|key| !exists(key)) {
            self.current = None;
        }
        removed
    }

    /// Contiguous row ranges of selected keys in `order`.
    pub fn ranges(&self, order: &[ItemKey]) -> Vec<std::ops::Range<usize>> {
        let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
        for (row, key) in order.iter().enumerate() {
            if !self.selected.contains(key) {
                continue;
            }
            match ranges.last_mut() {
                Some(range) if range.end == row => range.end += 1,
                _ => ranges.push(row..row + 1),
            }
        }
        ranges
    }
}

/// Keys of a model's visible rows in depth-first order, descending only
/// into parents for which `expanded` returns true.
pub fn visible_keys(model: &dyn ItemModel, expanded: impl Fn(ItemKey) -> bool) -> Vec<ItemKey> {
    let mut order = Vec::new();
    let mut stack: Vec<(Option<ItemKey>, usize)> = vec![(None, 0)];
    while let Some((parent, row)) = stack.pop() {
        let Some(key) = model.key(parent, row) else {
            continue;
        };
        stack.push((parent, row + 1));
        order.push(key);
        if expanded(key) && model.has_children(key) {
            stack.push((Some(key), 0));
        }
    }
    order
}
