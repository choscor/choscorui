//! Virtualized list, tree, table, and grid views over an [`ItemModel`].
//!
//! Only rows inside the viewport (plus a small overscan) own retained-tree
//! nodes; scrolling recycles them, so node count is bounded by the
//! viewport, not the dataset. Selection, current item, and expansion are
//! keyed by [`ItemKey`] and survive sorting, filtering, and incremental
//! model updates.

use choscorui_core::{
    Color, DisplayList, ItemKey, ItemModel, Key, NodeId, Point, Rect, Role, Selection,
    SelectionCommand, SelectionMode, SemanticAction, SemanticChange, SemanticCollection,
    SemanticError, SemanticNode, SemanticRole, SemanticState, SemanticTree, Size, Tree, TreeError,
    fold_for_search, visible_keys,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How long typed characters accumulate into one type-ahead search.
pub const TYPE_AHEAD_TIMEOUT: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewLayout {
    /// One item per row (list, tree, or table).
    List,
    /// Icon view: items flow left to right in cells of `item_width` by the
    /// row height.
    Grid { item_width: f64 },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ItemModifiers {
    pub shift: bool,
    pub command: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemViewInput {
    Key {
        key: Key,
        modifiers: ItemModifiers,
    },
    /// Primary-button press; `clicks` is the native click count.
    Press {
        position: Point,
        modifiers: ItemModifiers,
        clicks: u32,
    },
    /// Secondary-button press or the context-menu key.
    ContextMenu {
        position: Point,
    },
    Wheel {
        delta_y: f64,
    },
    /// Printable character for type-ahead at monotonic time `at`.
    Type {
        character: char,
        at: Duration,
    },
    Semantic {
        key: ItemKey,
        action: SemanticAction,
    },
}

/// What happened while handling one input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemViewResponse {
    pub selection_changed: bool,
    pub current_changed: bool,
    pub activated: Option<ItemKey>,
    pub context_menu: Option<ItemKey>,
    pub expansion_changed: bool,
    pub scrolled: bool,
    /// Parents whose children should be fetched (`EditableModel::fetch_more`).
    pub fetch: Vec<Option<ItemKey>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RealizedItem {
    pub key: ItemKey,
    /// Position in the flattened visible order.
    pub index: usize,
    pub depth: usize,
    pub node: NodeId,
    pub bounds: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemState {
    pub selected: bool,
    pub current: bool,
    pub focused: bool,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

/// Paints one item or cell. Implementations never mutate the view.
pub trait ItemDelegate {
    fn paint_cell(
        &self,
        list: &mut DisplayList,
        rect: Rect,
        model: &dyn ItemModel,
        key: ItemKey,
        column: usize,
        state: ItemState,
    );
}

pub struct ItemView {
    name: String,
    role: SemanticRole,
    layout: ViewLayout,
    row_height: f64,
    indent: f64,
    overscan: usize,
    viewport: Rect,
    offset: f64,
    order: Vec<ItemKey>,
    depth: HashMap<ItemKey, usize>,
    index_of: HashMap<ItemKey, usize>,
    expanded: HashSet<ItemKey>,
    selection: Selection,
    mode: SelectionMode,
    revision: Option<u64>,
    dirty: bool,
    realized: Vec<RealizedItem>,
    type_ahead: String,
    type_ahead_at: Option<Duration>,
}

impl ItemView {
    pub fn new(name: impl Into<String>, role: SemanticRole, row_height: f64) -> Self {
        Self {
            name: name.into(),
            role,
            layout: ViewLayout::List,
            row_height: row_height.max(1.0),
            indent: 16.0,
            overscan: 2,
            viewport: Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0)),
            offset: 0.0,
            order: Vec::new(),
            depth: HashMap::new(),
            index_of: HashMap::new(),
            expanded: HashSet::new(),
            selection: Selection::new(),
            mode: SelectionMode::Extended,
            revision: None,
            dirty: true,
            realized: Vec::new(),
            type_ahead: String::new(),
            type_ahead_at: None,
        }
    }

    pub fn set_layout(&mut self, layout: ViewLayout) {
        self.layout = layout;
    }
    pub fn set_selection_mode(&mut self, mode: SelectionMode) {
        self.mode = mode;
    }
    pub fn set_indent(&mut self, indent: f64) {
        self.indent = indent.max(0.0);
    }
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn current(&self) -> Option<ItemKey> {
        self.selection.current()
    }
    pub fn order(&self) -> &[ItemKey] {
        &self.order
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn realized(&self) -> &[RealizedItem] {
        &self.realized
    }
    pub fn is_expanded(&self, key: ItemKey) -> bool {
        self.expanded.contains(&key)
    }
    pub fn expanded_keys(&self) -> impl Iterator<Item = ItemKey> + '_ {
        self.expanded.iter().copied()
    }
    pub fn row_height(&self) -> f64 {
        self.row_height
    }

    /// Rebuilds the visible order when the model's revision changed. Keys
    /// that disappeared are dropped from selection and expansion.
    pub fn sync(&mut self, model: &dyn ItemModel) -> bool {
        let revision = model.revision();
        if !self.dirty && self.revision == Some(revision) {
            return false;
        }
        self.revision = Some(revision);
        self.dirty = false;
        self.expanded.retain(|key| model.row(*key).is_some());
        let expanded = &self.expanded;
        self.order = visible_keys(model, |key| expanded.contains(&key));
        self.index_of = self
            .order
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect();
        self.depth.clear();
        for key in &self.order {
            let depth = model
                .parent(*key)
                .and_then(|parent| self.depth.get(&parent))
                .map_or(0, |depth| depth + 1);
            self.depth.insert(*key, depth);
        }
        let index_of = &self.index_of;
        self.selection
            .retain_existing(|key| index_of.contains_key(&key));
        self.clamp_offset();
        true
    }

    fn columns_per_row(&self) -> usize {
        match self.layout {
            ViewLayout::List => 1,
            ViewLayout::Grid { item_width } => {
                ((self.viewport.size.width / item_width.max(1.0)).floor() as usize).max(1)
            }
        }
    }

    fn line_count(&self) -> usize {
        self.order.len().div_ceil(self.columns_per_row())
    }

    pub fn content_height(&self) -> f64 {
        self.line_count() as f64 * self.row_height
    }

    fn max_offset(&self) -> f64 {
        (self.content_height() - self.viewport.size.height).max(0.0)
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }

    /// Rows that fit in the viewport.
    pub fn page_lines(&self) -> usize {
        ((self.viewport.size.height / self.row_height).floor() as usize).max(1)
    }

    /// Rectangle of `index` in tree coordinates.
    pub fn item_rect(&self, index: usize) -> Rect {
        let columns = self.columns_per_row();
        let line = index / columns;
        let column = index % columns;
        let width = match self.layout {
            ViewLayout::List => self.viewport.size.width,
            ViewLayout::Grid { item_width } => item_width,
        };
        Rect::new(
            Point::new(
                self.viewport.origin.x + column as f64 * width,
                self.viewport.origin.y + line as f64 * self.row_height - self.offset,
            ),
            Size::new(width, self.row_height),
        )
    }

    pub fn index_at(&self, point: Point) -> Option<usize> {
        if !self.viewport.contains(point) {
            return None;
        }
        let line = ((point.y - self.viewport.origin.y + self.offset) / self.row_height) as usize;
        let column = match self.layout {
            ViewLayout::List => 0,
            ViewLayout::Grid { item_width } => {
                let column = ((point.x - self.viewport.origin.x) / item_width) as usize;
                if column >= self.columns_per_row() {
                    return None;
                }
                column
            }
        };
        let index = line * self.columns_per_row() + column;
        (index < self.order.len()).then_some(index)
    }

    pub fn key_at(&self, point: Point) -> Option<ItemKey> {
        self.index_at(point).map(|index| self.order[index])
    }

    pub fn index_of(&self, key: ItemKey) -> Option<usize> {
        self.index_of.get(&key).copied()
    }

    /// Lays the view out in `viewport` (tree coordinates) and realizes the
    /// visible rows as children of `id`.
    pub fn arrange(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        viewport: Rect,
    ) -> Result<(), TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        self.viewport = viewport;
        self.clamp_offset();
        self.realize(tree, id)
    }

    fn realize(&mut self, tree: &mut Tree, id: NodeId) -> Result<(), TreeError> {
        let columns = self.columns_per_row();
        let first_line = (self.offset / self.row_height).floor() as usize;
        let last_line =
            ((self.offset + self.viewport.size.height) / self.row_height).ceil() as usize;
        let first = first_line.saturating_sub(self.overscan) * columns;
        let end = ((last_line + self.overscan) * columns).min(self.order.len());
        let wanted: Vec<usize> = (first..end.max(first)).collect();
        let mut by_key: HashMap<ItemKey, NodeId> = self
            .realized
            .iter()
            .map(|item| (item.key, item.node))
            .collect();
        let wanted_keys: HashSet<ItemKey> = wanted.iter().map(|index| self.order[*index]).collect();
        let mut free: Vec<NodeId> = Vec::new();
        by_key.retain(|key, node| {
            if wanted_keys.contains(key) && tree.contains(*node) {
                true
            } else {
                if tree.contains(*node) {
                    free.push(*node);
                }
                false
            }
        });
        let mut realized = Vec::with_capacity(wanted.len());
        for index in wanted {
            let key = self.order[index];
            let bounds = self.item_rect(index);
            let node = match by_key.get(&key) {
                Some(node) => *node,
                None => match free.pop() {
                    Some(node) => node,
                    None => tree.attach(id, bounds)?,
                },
            };
            tree.set_bounds(node, bounds)?;
            realized.push(RealizedItem {
                key,
                index,
                depth: self.depth.get(&key).copied().unwrap_or(0),
                node,
                bounds,
            });
        }
        for node in free {
            tree.detach(node)?;
        }
        self.realized = realized;
        Ok(())
    }

    /// Scrolls so `index` is fully visible. Returns true when it moved.
    pub fn scroll_to_index(&mut self, index: usize) -> bool {
        let line = index / self.columns_per_row();
        let top = line as f64 * self.row_height;
        let bottom = top + self.row_height;
        let before = self.offset;
        if top < self.offset {
            self.offset = top;
        } else if bottom > self.offset + self.viewport.size.height {
            self.offset = bottom - self.viewport.size.height;
        }
        self.clamp_offset();
        self.offset != before
    }

    pub fn scroll_by(&mut self, delta: f64) -> bool {
        let before = self.offset;
        self.offset += delta;
        self.clamp_offset();
        self.offset != before
    }

    pub fn set_expanded(&mut self, model: &dyn ItemModel, key: ItemKey, expanded: bool) -> bool {
        if !model.has_children(key) && !model.can_fetch_more(Some(key)) {
            return false;
        }
        let changed = if expanded {
            self.expanded.insert(key)
        } else {
            self.expanded.remove(&key)
        };
        if changed {
            self.dirty = true;
            self.sync(model);
        }
        changed
    }

    /// Restores expansion (for example from saved view state).
    pub fn restore_expanded(&mut self, keys: impl IntoIterator<Item = ItemKey>) {
        self.expanded = keys.into_iter().collect();
        self.dirty = true;
    }

    /// Handles one input. Call [`arrange`](Self::arrange) afterwards when
    /// `scrolled` or `expansion_changed` is set.
    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        model: &dyn ItemModel,
        input: ItemViewInput,
    ) -> Result<ItemViewResponse, TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        self.sync(model);
        let mut response = ItemViewResponse::default();
        match input {
            ItemViewInput::Key { key, modifiers } => {
                self.handle_key(model, key, modifiers, &mut response);
            }
            ItemViewInput::Press {
                position,
                modifiers,
                clicks,
            } => {
                tree.set_focus(Some(id))?;
                if let Some(index) = self.index_at(position) {
                    let key = self.order[index];
                    let disclosure = self.layout == ViewLayout::List
                        && model.has_children(key)
                        && position.x
                            < self.viewport.origin.x
                                + (self.depth.get(&key).copied().unwrap_or(0) + 1) as f64
                                    * self.indent;
                    if disclosure && clicks == 1 {
                        let expand = !self.expanded.contains(&key);
                        self.toggle_expansion(model, key, expand, &mut response);
                    } else {
                        let command = if modifiers.command {
                            SelectionCommand::Toggle
                        } else if modifiers.shift {
                            SelectionCommand::ExtendTo
                        } else {
                            SelectionCommand::Replace
                        };
                        self.select(key, command, &mut response);
                        if clicks >= 2 {
                            response.activated = Some(key);
                        }
                    }
                }
            }
            ItemViewInput::ContextMenu { position } => {
                tree.set_focus(Some(id))?;
                if let Some(key) = self.key_at(position) {
                    if !self.selection.is_selected(key) {
                        self.select(key, SelectionCommand::Replace, &mut response);
                    }
                    response.context_menu = Some(key);
                }
            }
            ItemViewInput::Wheel { delta_y } => {
                response.scrolled = self.scroll_by(delta_y);
                self.request_tail_fetch(model, &mut response);
            }
            ItemViewInput::Type { character, at } => {
                self.type_ahead_search(model, character, at, &mut response);
            }
            ItemViewInput::Semantic { key, action } => {
                if self.index_of(key).is_none() {
                    return Ok(response);
                }
                match action {
                    SemanticAction::Activate => {
                        self.select(key, SelectionCommand::Replace, &mut response);
                        response.activated = Some(key);
                    }
                    SemanticAction::Focus => {
                        tree.set_focus(Some(id))?;
                        self.select(key, SelectionCommand::CurrentOnly, &mut response);
                    }
                    SemanticAction::Expand => {
                        self.toggle_expansion(model, key, true, &mut response)
                    }
                    SemanticAction::Collapse => {
                        self.toggle_expansion(model, key, false, &mut response)
                    }
                    SemanticAction::ShowMenu => response.context_menu = Some(key),
                    _ => {}
                }
            }
        }
        Ok(response)
    }

    fn select(&mut self, key: ItemKey, command: SelectionCommand, response: &mut ItemViewResponse) {
        let before = self.selection.current();
        response.selection_changed |= self.selection.apply(self.mode, command, key, &self.order);
        response.current_changed |= before != Some(key);
        if let Some(index) = self.index_of(key) {
            response.scrolled |= self.scroll_to_index(index);
        }
    }

    fn toggle_expansion(
        &mut self,
        model: &dyn ItemModel,
        key: ItemKey,
        expand: bool,
        response: &mut ItemViewResponse,
    ) {
        if expand && model.can_fetch_more(Some(key)) {
            response.fetch.push(Some(key));
        }
        if self.set_expanded(model, key, expand) {
            response.expansion_changed = true;
        }
    }

    fn request_tail_fetch(&self, model: &dyn ItemModel, response: &mut ItemViewResponse) {
        let near_end = self.offset + self.viewport.size.height
            >= self.content_height() - self.row_height * self.page_lines() as f64;
        if near_end && model.can_fetch_more(None) && !response.fetch.contains(&None) {
            response.fetch.push(None);
        }
    }

    fn handle_key(
        &mut self,
        model: &dyn ItemModel,
        key: Key,
        modifiers: ItemModifiers,
        response: &mut ItemViewResponse,
    ) {
        if self.order.is_empty() {
            self.request_tail_fetch(model, response);
            return;
        }
        let last = self.order.len() - 1;
        let current = self.selection.current().and_then(|key| self.index_of(key));
        let columns = self.columns_per_row();
        let page = self.page_lines() * columns;
        let target = match (key, current) {
            (Key::Character('a') | Key::Character('A'), _) if modifiers.command => {
                if self.mode != SelectionMode::Single {
                    response.selection_changed |= self.selection.select_all(&self.order);
                }
                return;
            }
            (Key::Enter, Some(index)) => {
                response.activated = Some(self.order[index]);
                return;
            }
            (Key::Space, Some(index)) => {
                let command = if modifiers.command || self.mode == SelectionMode::Multi {
                    SelectionCommand::Toggle
                } else {
                    SelectionCommand::Replace
                };
                self.select(self.order[index], command, response);
                return;
            }
            (Key::ArrowLeft | Key::ArrowRight, Some(index)) if self.layout == ViewLayout::List => {
                let item = self.order[index];
                let expand = key == Key::ArrowRight;
                let expandable = model.has_children(item) || model.can_fetch_more(Some(item));
                if expandable && expand != self.expanded.contains(&item) {
                    self.toggle_expansion(model, item, expand, response);
                    return;
                }
                if expand && expandable {
                    // Already expanded: move to the first child.
                    Some((index + 1).min(last))
                } else if !expand {
                    model.parent(item).and_then(|parent| self.index_of(parent))
                } else {
                    None
                }
            }
            (_, None) => match key {
                Key::ArrowDown | Key::ArrowRight | Key::Home | Key::PageDown => Some(0),
                Key::ArrowUp | Key::ArrowLeft | Key::End | Key::PageUp => Some(last),
                _ => None,
            },
            (Key::ArrowDown, Some(index)) => Some((index + columns).min(last)),
            (Key::ArrowUp, Some(index)) => Some(index.saturating_sub(columns)),
            (Key::ArrowRight, Some(index)) => Some((index + 1).min(last)),
            (Key::ArrowLeft, Some(index)) => Some(index.saturating_sub(1)),
            (Key::PageDown, Some(index)) => Some((index + page).min(last)),
            (Key::PageUp, Some(index)) => Some(index.saturating_sub(page)),
            (Key::Home, Some(_)) => Some(0),
            (Key::End, Some(_)) => Some(last),
            _ => None,
        };
        if let Some(index) = target {
            let command = if modifiers.shift && self.mode != SelectionMode::Single {
                SelectionCommand::ExtendTo
            } else if modifiers.command && self.mode != SelectionMode::Single {
                SelectionCommand::CurrentOnly
            } else {
                SelectionCommand::Replace
            };
            self.select(self.order[index], command, response);
            if index + page >= self.order.len() {
                self.request_tail_fetch_forced(model, response);
            }
        }
    }

    fn request_tail_fetch_forced(&self, model: &dyn ItemModel, response: &mut ItemViewResponse) {
        if model.can_fetch_more(None) && !response.fetch.contains(&None) {
            response.fetch.push(None);
        }
    }

    fn type_ahead_search(
        &mut self,
        model: &dyn ItemModel,
        character: char,
        at: Duration,
        response: &mut ItemViewResponse,
    ) {
        if character.is_control() || self.order.is_empty() {
            return;
        }
        if self
            .type_ahead_at
            .is_none_or(|last| at.saturating_sub(last) > TYPE_AHEAD_TIMEOUT)
        {
            self.type_ahead.clear();
        }
        self.type_ahead_at = Some(at);
        self.type_ahead.push(character);
        let needle = fold_for_search(&self.type_ahead);
        let start = self
            .selection
            .current()
            .and_then(|key| self.index_of(key))
            .map_or(0, |index| {
                // A fresh search starts after the current item; a growing
                // prefix keeps matching the current item.
                if self.type_ahead.chars().count() == 1 {
                    index + 1
                } else {
                    index
                }
            });
        let count = self.order.len();
        for step in 0..count {
            let index = (start + step) % count;
            let key = self.order[index];
            let text = model.data(key, 0, Role::Display).to_display_string();
            if fold_for_search(&text).starts_with(&needle) {
                self.select(key, SelectionCommand::Replace, response);
                return;
            }
        }
    }

    /// Semantic node for the view container.
    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: self.role,
            name: self.name.clone(),
            description: String::new(),
            value: None,
            state: SemanticState {
                selected: None,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![SemanticAction::Focus],
            text: None,
        }
    }

    /// Semantic node for one realized item.
    pub fn item_semantics(&self, model: &dyn ItemModel, item: &RealizedItem) -> SemanticNode {
        let has_children = model.has_children(item.key) || model.can_fetch_more(Some(item.key));
        let role = match self.role {
            SemanticRole::Tree => SemanticRole::TreeItem,
            SemanticRole::Table => SemanticRole::Row,
            _ => SemanticRole::ListItem,
        };
        let mut actions = vec![SemanticAction::Activate, SemanticAction::Focus];
        if has_children {
            actions.push(if self.expanded.contains(&item.key) {
                SemanticAction::Collapse
            } else {
                SemanticAction::Expand
            });
        }
        actions.push(SemanticAction::ShowMenu);
        let accessible = model.data(item.key, 0, Role::Accessibility);
        let name = if accessible.is_none() {
            model.data(item.key, 0, Role::Display).to_display_string()
        } else {
            accessible.to_display_string()
        };
        let value = (1..model.column_count())
            .map(|column| {
                model
                    .data(item.key, column, Role::Display)
                    .to_display_string()
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        SemanticNode {
            role,
            name,
            description: String::new(),
            value: (!value.is_empty()).then_some(value),
            state: SemanticState {
                selected: Some(self.selection.is_selected(item.key)),
                expanded: has_children.then_some(self.expanded.contains(&item.key)),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions,
            text: None,
        }
    }

    /// Publishes the container and its realized items into `semantics`,
    /// declaring the full item count and each realized item's index.
    pub fn sync_semantics(
        &self,
        semantics: &mut SemanticTree,
        tree: &Tree,
        id: NodeId,
        model: &dyn ItemModel,
    ) -> Result<Vec<SemanticChange>, SemanticError> {
        let mut changes = vec![semantics.set(id, self.semantics(), tree)?];
        changes.push(semantics.set_collection(
            id,
            Some(SemanticCollection {
                item_count: self.order.len(),
                column_count: (model.column_count() > 1).then_some(model.column_count()),
            }),
        )?);
        for item in &self.realized {
            changes.push(semantics.set(item.node, self.item_semantics(model, item), tree)?);
            changes.push(semantics.set_item_index(item.node, Some(item.index))?);
        }
        semantics.prune(tree);
        changes.retain(|change| *change != SemanticChange::None);
        Ok(changes)
    }

    /// Paints realized items clipped to the viewport. `columns` are table
    /// column widths; empty means one column spanning the item.
    pub fn paint(
        &self,
        list: &mut DisplayList,
        model: &dyn ItemModel,
        delegate: &dyn ItemDelegate,
        columns: &[f64],
        focused: bool,
    ) {
        list.push_clip(self.viewport);
        for item in &self.realized {
            let has_children = model.has_children(item.key);
            let state = ItemState {
                selected: self.selection.is_selected(item.key),
                current: self.selection.current() == Some(item.key),
                focused,
                depth: item.depth,
                has_children,
                expanded: self.expanded.contains(&item.key),
            };
            if columns.is_empty() {
                delegate.paint_cell(list, item.bounds, model, item.key, 0, state);
            } else {
                let mut x = item.bounds.origin.x;
                for (column, width) in columns.iter().enumerate() {
                    let rect = Rect::new(
                        Point::new(x, item.bounds.origin.y),
                        Size::new(*width, item.bounds.size.height),
                    );
                    delegate.paint_cell(list, rect, model, item.key, column, state);
                    x += width;
                }
            }
        }
        let _ = list.pop_clip();
    }
}

/// Default text delegate: selection fill, focus outline on the current
/// item, tree disclosure triangle, and single-line text.
#[derive(Clone, Debug)]
pub struct TextItemDelegate {
    pub font_family: String,
    pub font_size: f64,
    pub text: Color,
    pub selected_text: Color,
    pub selection: Color,
    pub inactive_selection: Color,
    pub focus: Color,
    pub indent: f64,
}

impl Default for TextItemDelegate {
    fn default() -> Self {
        Self {
            font_family: "Helvetica".into(),
            font_size: 14.0,
            text: Color::rgba8(30, 30, 35, 255),
            selected_text: Color::rgba8(255, 255, 255, 255),
            selection: Color::rgba8(40, 110, 220, 255),
            inactive_selection: Color::rgba8(200, 200, 205, 255),
            focus: Color::rgba8(40, 110, 220, 255),
            indent: 16.0,
        }
    }
}

impl ItemDelegate for TextItemDelegate {
    fn paint_cell(
        &self,
        list: &mut DisplayList,
        rect: Rect,
        model: &dyn ItemModel,
        key: ItemKey,
        column: usize,
        state: ItemState,
    ) {
        if state.selected {
            list.fill_rect(
                rect,
                if state.focused {
                    self.selection
                } else {
                    self.inactive_selection
                },
            );
        }
        let mut x = rect.origin.x + 4.0;
        if column == 0 {
            x += state.depth as f64 * self.indent;
            if state.has_children {
                let mid = rect.origin.y + rect.size.height / 2.0;
                let mut path = choscorui_core::Path::new();
                if state.expanded {
                    path.move_to(Point::new(x, mid - 3.0))
                        .line_to(Point::new(x + 8.0, mid - 3.0))
                        .line_to(Point::new(x + 4.0, mid + 3.0))
                        .close();
                } else {
                    path.move_to(Point::new(x + 1.0, mid - 4.0))
                        .line_to(Point::new(x + 7.0, mid))
                        .line_to(Point::new(x + 1.0, mid + 4.0))
                        .close();
                }
                let _ = list.fill_path(
                    path,
                    choscorui_core::Paint::Solid(self.text),
                    choscorui_core::FillRule::NonZero,
                );
            }
            x += self.indent;
        }
        let text = model.data(key, column, Role::Display).to_display_string();
        if !text.is_empty() {
            let baseline = rect.origin.y + rect.size.height / 2.0 + self.font_size * 0.35;
            let color = if state.selected && state.focused {
                self.selected_text
            } else {
                self.text
            };
            list.push_clip(rect);
            let _ = list.draw_text(
                Point::new(x, baseline),
                &text,
                &self.font_family,
                self.font_size,
                color,
            );
            let _ = list.pop_clip();
        }
        if state.current && state.focused && column == 0 {
            let _ = list.stroke_rect(rect, self.focus, 1.0);
        }
    }
}
