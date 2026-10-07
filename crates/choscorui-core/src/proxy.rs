//! Model adapters: sort/filter, identity mapping, concatenation, and a
//! borrowed-slice model. Adapters keep the source's [`ItemKey`]s (or a
//! documented reversible mapping), so a selection made through one adapter
//! still names the same items after re-sorting, filtering, or composing.

use crate::{BasicCollator, ChangeLog, Collator, ItemKey, ItemModel, ModelChange, Role, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchMode {
    Contains,
    Prefix,
    Exact,
}

/// Case- and diacritic-insensitive text filter ("viet" matches "Việt").
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextFilter {
    /// Column to search, or every column when `None`.
    pub column: Option<usize>,
    pub pattern: String,
    pub mode: MatchMode,
}

impl TextFilter {
    pub fn contains(pattern: impl Into<String>) -> Self {
        Self {
            column: None,
            pattern: pattern.into(),
            mode: MatchMode::Contains,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        let needle = crate::locale::fold_for_search(&self.pattern);
        let hay = crate::locale::fold_for_search(text);
        match self.mode {
            MatchMode::Contains => hay.contains(&needle),
            MatchMode::Prefix => hay.starts_with(&needle),
            MatchMode::Exact => hay == needle,
        }
    }

    fn accepts(&self, source: &dyn ItemModel, key: ItemKey) -> bool {
        if self.pattern.is_empty() {
            return true;
        }
        let columns = match self.column {
            Some(column) => column..column + 1,
            None => 0..source.column_count(),
        };
        columns.into_iter().any(|column| {
            self.matches(&source.data(key, column, Role::Display).to_display_string())
        })
    }
}

type Predicate = Box<dyn Fn(&dyn ItemModel, ItemKey) -> bool>;

/// Sorted and filtered view of a source model.
///
/// Call [`sync`](Self::sync) after the source changes (or each frame); it
/// skips work when nothing relevant changed and otherwise rebuilds the
/// mapping, recording keyed `Inserted`/`Removed`/`Reordered` changes in the
/// proxy's own log. Use [`view`](Self::view) to read through the proxy.
pub struct SortFilterProxy {
    sort: Option<(usize, SortOrder)>,
    filter: Option<TextFilter>,
    predicate: Option<Predicate>,
    collator: Box<dyn Collator>,
    /// Keep ancestors of matching descendants (tree filtering).
    recursive: bool,
    rows: HashMap<Option<ItemKey>, Vec<ItemKey>>,
    positions: HashMap<ItemKey, usize>,
    parents: HashMap<ItemKey, Option<ItemKey>>,
    source_revision: Option<u64>,
    dirty: bool,
    log: ChangeLog,
}

impl Default for SortFilterProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl SortFilterProxy {
    pub fn new() -> Self {
        Self {
            sort: None,
            filter: None,
            predicate: None,
            collator: Box::new(BasicCollator),
            recursive: true,
            rows: HashMap::new(),
            positions: HashMap::new(),
            parents: HashMap::new(),
            source_revision: None,
            dirty: true,
            log: ChangeLog::default(),
        }
    }

    /// Sorts by the [`Role::Sort`] value of `column` (text uses the
    /// collator). `None` keeps source order. Sorting is stable.
    pub fn set_sort(&mut self, sort: Option<(usize, SortOrder)>) {
        if self.sort != sort {
            self.sort = sort;
            self.dirty = true;
        }
    }
    pub fn sort(&self) -> Option<(usize, SortOrder)> {
        self.sort
    }
    pub fn set_filter(&mut self, filter: Option<TextFilter>) {
        if self.filter != filter {
            self.filter = filter;
            self.dirty = true;
        }
    }
    pub fn filter(&self) -> Option<&TextFilter> {
        self.filter.as_ref()
    }
    pub fn set_predicate(&mut self, predicate: Option<Predicate>) {
        self.predicate = predicate;
        self.dirty = true;
    }
    pub fn set_collator(&mut self, collator: Box<dyn Collator>) {
        self.collator = collator;
        self.dirty = true;
    }
    pub fn set_recursive(&mut self, recursive: bool) {
        if self.recursive != recursive {
            self.recursive = recursive;
            self.dirty = true;
        }
    }

    /// Brings the mapping up to date. Returns true when the proxy changed.
    pub fn sync(&mut self, source: &dyn ItemModel) -> bool {
        let revision = source.revision();
        if let Some(seen) = self.source_revision
            && !self.dirty
        {
            if seen == revision {
                return false;
            }
            let changes = source.changes_since(seen);
            let sort_column = self.sort.map(|(column, _)| column);
            let relevant = changes.iter().any(|change| match change {
                ModelChange::DataChanged { roles, .. } => {
                    (self.filter.is_some() || self.predicate.is_some() || sort_column.is_some())
                        && roles
                            .iter()
                            .any(|role| matches!(role, Role::Display | Role::Edit | Role::Sort))
                }
                ModelChange::HeaderChanged { .. } => false,
                _ => true,
            });
            if !relevant {
                self.source_revision = Some(revision);
                for change in changes {
                    if let ModelChange::DataChanged { key, .. } = &change
                        && self.positions.contains_key(key)
                    {
                        self.log.record(change);
                    } else if matches!(change, ModelChange::HeaderChanged { .. }) {
                        self.log.record(change);
                    }
                }
                return true;
            }
        }
        self.rebuild(source);
        self.source_revision = Some(revision);
        self.dirty = false;
        true
    }

    fn accepts(&self, source: &dyn ItemModel, key: ItemKey) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|filter| filter.accepts(source, key))
            && self
                .predicate
                .as_ref()
                .is_none_or(|predicate| predicate(source, key))
    }

    fn rebuild(&mut self, source: &dyn ItemModel) {
        let mut rows: HashMap<Option<ItemKey>, Vec<ItemKey>> = HashMap::new();
        self.collect(source, None, &mut rows);
        let mut positions = HashMap::new();
        let mut parents = HashMap::new();
        for (parent, children) in &rows {
            for (row, key) in children.iter().enumerate() {
                positions.insert(*key, row);
                parents.insert(*key, *parent);
            }
        }
        let initial = self.source_revision.is_none();
        let old = std::mem::replace(&mut self.rows, rows);
        self.positions = positions;
        self.parents = parents;
        if initial {
            self.log.record(ModelChange::Reset);
            return;
        }
        let mut parent_keys: Vec<_> = old.keys().chain(self.rows.keys()).copied().collect();
        parent_keys.sort();
        parent_keys.dedup();
        for parent in parent_keys {
            let empty = Vec::new();
            let before = old.get(&parent).unwrap_or(&empty);
            let after = self.rows.get(&parent).unwrap_or(&empty);
            record_diff(&mut self.log, parent, before, after);
        }
    }

    /// Collects accepted children of `parent`; returns whether any item in
    /// this subtree was accepted.
    fn collect(
        &self,
        source: &dyn ItemModel,
        parent: Option<ItemKey>,
        rows: &mut HashMap<Option<ItemKey>, Vec<ItemKey>>,
    ) -> bool {
        let count = source.row_count(parent);
        let mut kept = Vec::with_capacity(count);
        for row in 0..count {
            let Some(key) = source.key(parent, row) else {
                continue;
            };
            let descendant = source.has_children(key) && self.collect(source, Some(key), rows);
            if self.accepts(source, key) || (self.recursive && descendant) {
                kept.push(key);
            } else {
                rows.remove(&Some(key));
            }
        }
        if let Some((column, order)) = self.sort {
            let values: HashMap<ItemKey, Value> = kept
                .iter()
                .map(|key| (*key, sort_value(source, *key, column)))
                .collect();
            kept.sort_by(|a, b| {
                let ordering = match (&values[a], &values[b]) {
                    (Value::Text(x), Value::Text(y)) => self.collator.compare(x, y),
                    (x, y) => x.compare(y),
                };
                match order {
                    SortOrder::Ascending => ordering,
                    SortOrder::Descending => ordering.reverse(),
                }
            });
        }
        let any = !kept.is_empty();
        if any || parent.is_none() {
            rows.insert(parent, kept);
        }
        any
    }

    pub fn view<'a>(&'a self, source: &'a dyn ItemModel) -> ProxyView<'a> {
        ProxyView {
            proxy: self,
            source,
        }
    }

    /// Visible row count under `parent` without borrowing the source.
    pub fn len(&self, parent: Option<ItemKey>) -> usize {
        self.rows.get(&parent).map_or(0, Vec::len)
    }
    pub fn is_empty(&self) -> bool {
        self.len(None) == 0
    }
    pub fn contains(&self, key: ItemKey) -> bool {
        self.positions.contains_key(&key)
    }
}

fn sort_value(source: &dyn ItemModel, key: ItemKey, column: usize) -> Value {
    for role in [Role::Sort, Role::Edit, Role::Display] {
        let value = source.data(key, column, role);
        if !value.is_none() {
            return value;
        }
    }
    Value::None
}

fn record_diff(
    log: &mut ChangeLog,
    parent: Option<ItemKey>,
    before: &[ItemKey],
    after: &[ItemKey],
) {
    let after_set: HashSet<_> = after.iter().copied().collect();
    let before_set: HashSet<_> = before.iter().copied().collect();
    let removed: Vec<_> = before
        .iter()
        .copied()
        .filter(|key| !after_set.contains(key))
        .collect();
    if !removed.is_empty() {
        log.record(ModelChange::Removed {
            parent,
            keys: removed,
        });
    }
    let mut row = 0;
    while row < after.len() {
        if before_set.contains(&after[row]) {
            row += 1;
            continue;
        }
        let first_row = row;
        while row < after.len() && !before_set.contains(&after[row]) {
            row += 1;
        }
        log.record(ModelChange::Inserted {
            parent,
            first_row,
            keys: after[first_row..row].to_vec(),
        });
    }
    let kept_before: Vec<_> = before
        .iter()
        .filter(|key| after_set.contains(key))
        .collect();
    let kept_after: Vec<_> = after
        .iter()
        .filter(|key| before_set.contains(key))
        .collect();
    if kept_before != kept_after {
        log.record(ModelChange::Reordered { parent });
    }
}

/// Read access through a [`SortFilterProxy`].
#[derive(Clone, Copy)]
pub struct ProxyView<'a> {
    proxy: &'a SortFilterProxy,
    source: &'a dyn ItemModel,
}

impl ItemModel for ProxyView<'_> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.proxy.len(parent)
    }
    fn column_count(&self) -> usize {
        self.source.column_count()
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.proxy.rows.get(&parent)?.get(row).copied()
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.proxy.parents.get(&key).copied().flatten()
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        self.proxy.positions.get(&key).copied()
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        if self.proxy.contains(key) {
            self.source.data(key, column, role)
        } else {
            Value::None
        }
    }
    fn header(&self, column: usize, role: Role) -> Value {
        self.source.header(column, role)
    }
    fn editable(&self, key: ItemKey, column: usize) -> bool {
        self.proxy.contains(key) && self.source.editable(key, column)
    }
    fn revision(&self) -> u64 {
        self.proxy.log.revision()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.proxy.log.since(revision)
    }
    fn can_fetch_more(&self, parent: Option<ItemKey>) -> bool {
        self.source.can_fetch_more(parent)
    }
}

/// Identity adapter that rewrites data on the way out (formatting, extra
/// roles) while passing structure and keys through unchanged.
pub struct MappedModel<'a, F> {
    source: &'a dyn ItemModel,
    map: F,
}

impl<'a, F: Fn(ItemKey, usize, Role, Value) -> Value> MappedModel<'a, F> {
    pub fn new(source: &'a dyn ItemModel, map: F) -> Self {
        Self { source, map }
    }
}

impl<F: Fn(ItemKey, usize, Role, Value) -> Value> ItemModel for MappedModel<'_, F> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.source.row_count(parent)
    }
    fn column_count(&self) -> usize {
        self.source.column_count()
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.source.key(parent, row)
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.source.parent(key)
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        self.source.row(key)
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        (self.map)(key, column, role, self.source.data(key, column, role))
    }
    fn header(&self, column: usize, role: Role) -> Value {
        self.source.header(column, role)
    }
    fn editable(&self, key: ItemKey, column: usize) -> bool {
        self.source.editable(key, column)
    }
    fn revision(&self) -> u64 {
        self.source.revision()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.source.changes_since(revision)
    }
    fn can_fetch_more(&self, parent: Option<ItemKey>) -> bool {
        self.source.can_fetch_more(parent)
    }
}

const CONCAT_SHIFT: u32 = 48;

/// Flat concatenation of several list/table models. Keys are mapped
/// reversibly as `source_index << 48 | source_key` (source keys must stay
/// below 2^48). Any source change is reported as one `Reset`; selections
/// survive because keys are stable.
pub struct ConcatModel<'a> {
    sources: Vec<&'a dyn ItemModel>,
}

impl<'a> ConcatModel<'a> {
    pub fn new(sources: Vec<&'a dyn ItemModel>) -> Self {
        assert!(sources.len() < 1 << 16, "too many concatenated sources");
        Self { sources }
    }

    pub fn map_key(source: usize, key: ItemKey) -> ItemKey {
        debug_assert!(key.0 < 1 << CONCAT_SHIFT);
        ItemKey(((source as u64) << CONCAT_SHIFT) | key.0)
    }

    pub fn source_key(key: ItemKey) -> (usize, ItemKey) {
        (
            (key.0 >> CONCAT_SHIFT) as usize,
            ItemKey(key.0 & ((1 << CONCAT_SHIFT) - 1)),
        )
    }

    fn locate(&self, row: usize) -> Option<(usize, usize)> {
        let mut offset = row;
        for (index, source) in self.sources.iter().enumerate() {
            let count = source.row_count(None);
            if offset < count {
                return Some((index, offset));
            }
            offset -= count;
        }
        None
    }
}

impl ItemModel for ConcatModel<'_> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        if parent.is_some() {
            return 0;
        }
        self.sources
            .iter()
            .map(|source| source.row_count(None))
            .sum()
    }
    fn column_count(&self) -> usize {
        self.sources
            .iter()
            .map(|source| source.column_count())
            .max()
            .unwrap_or(1)
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        if parent.is_some() {
            return None;
        }
        let (index, local) = self.locate(row)?;
        self.sources[index]
            .key(None, local)
            .map(|key| Self::map_key(index, key))
    }
    fn parent(&self, _: ItemKey) -> Option<ItemKey> {
        None
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        let (index, local) = Self::source_key(key);
        let row = self.sources.get(index)?.row(local)?;
        Some(
            self.sources[..index]
                .iter()
                .map(|source| source.row_count(None))
                .sum::<usize>()
                + row,
        )
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        let (index, local) = Self::source_key(key);
        self.sources
            .get(index)
            .map_or(Value::None, |source| source.data(local, column, role))
    }
    fn header(&self, column: usize, role: Role) -> Value {
        self.sources
            .first()
            .map_or(Value::None, |source| source.header(column, role))
    }
    fn editable(&self, key: ItemKey, column: usize) -> bool {
        let (index, local) = Self::source_key(key);
        self.sources
            .get(index)
            .is_some_and(|source| source.editable(local, column))
    }
    fn revision(&self) -> u64 {
        self.sources.iter().map(|source| source.revision()).sum()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        if revision < self.revision() {
            vec![ModelChange::Reset]
        } else {
            Vec::new()
        }
    }
}

/// A borrowed slice presented as a flat model without copying rows. Row
/// `i` has key `i`, so keys are stable only while the slice is not
/// reordered; bump the revision with [`SliceModel::with_revision`] when the
/// caller mutates the data.
pub struct SliceModel<'a, T, F> {
    items: &'a [T],
    columns: usize,
    data: F,
    revision: u64,
}

impl<'a, T, F: Fn(&T, usize, Role) -> Value> SliceModel<'a, T, F> {
    pub fn new(items: &'a [T], columns: usize, data: F) -> Self {
        Self {
            items,
            columns: columns.max(1),
            data,
            revision: 0,
        }
    }
    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }
}

impl<T, F: Fn(&T, usize, Role) -> Value> ItemModel for SliceModel<'_, T, F> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        if parent.is_some() {
            0
        } else {
            self.items.len()
        }
    }
    fn column_count(&self) -> usize {
        self.columns
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        (parent.is_none() && row < self.items.len()).then_some(ItemKey(row as u64))
    }
    fn parent(&self, _: ItemKey) -> Option<ItemKey> {
        None
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        let row = usize::try_from(key.0).ok()?;
        (row < self.items.len()).then_some(row)
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        match self.row(key) {
            Some(row) if column < self.columns => (self.data)(&self.items[row], column, role),
            _ => Value::None,
        }
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        if revision < self.revision {
            vec![ModelChange::Reset]
        } else {
            Vec::new()
        }
    }
}
