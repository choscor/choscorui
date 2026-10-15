//! Virtualized list, tree, table, and grid views over an [`ItemModel`].
//!
//! Only rows inside the viewport (plus a small overscan) own retained-tree
//! nodes; scrolling recycles them, so node count is bounded by the
//! viewport, not the dataset. Selection, current item, and expansion are
//! keyed by [`ItemKey`] and survive sorting, filtering, and incremental
//! model updates.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::Parts;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TextAlign, TextLine, VisualState, content_rect,
    draw_text,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, ItemKey, ItemModel, Key, KeyEvent, LayoutDirection, Localizer,
    Measured, NodeId, Point, PointerButton, PointerEvent, Rect, Role, Selection, SelectionCommand,
    SelectionMode, SemanticAction, SemanticChange, SemanticCollection, SemanticError, SemanticNode,
    SemanticParts, SemanticRole, SemanticState, SemanticTree, Size, Tree, TreeError, WheelUnit,
    visible_keys,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

/// How long typed characters accumulate into one type-ahead search.
pub const TYPE_AHEAD_TIMEOUT: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewLayout {
    /// One item per row (list, tree, or table).
    List,
    /// Icon view: items flow in reading order in cells of `item_width` by
    /// the row height.
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemState {
    pub selected: bool,
    pub current: bool,
    pub focused: bool,
    /// The view has keyboard focus: the current item shows the ring.
    pub focus_visible: bool,
    pub depth: usize,
    /// The indent of one tree level, from [`ItemView::set_indent`].
    pub indent: f64,
    pub has_children: bool,
    pub expanded: bool,
    /// The hover and press of the row, and the disabled flag of the view.
    pub pointer: VisualState,
}

/// Paints one item or cell in the context's bounds. Implementations never
/// mutate the view.
pub trait ItemDelegate {
    fn paint_cell(
        &self,
        cx: &mut PaintContext<'_>,
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
    type_ahead: crate::hosted::TypeAhead,
    /// True when no row has children, so rows reserve no disclosure gap.
    flat: bool,
    /// Scroll the current item into view at the next arrange, after a
    /// change of the rows or the layout.
    reveal: bool,
    /// The model, delegate, and column widths of a hosted view.
    source: Option<ItemSource>,
    /// Whether a hosted view takes its row height from the `ListView` row
    /// recipe in each arrange.
    styled_rows: bool,
    /// The text under a hosted table, after its footer rows.
    caption: String,
    /// The cells of each footer row of a hosted table, one per column.
    footer: Vec<Vec<String>>,
    /// The overlay scroll bar of a hosted view.
    bar: crate::OverlayScrollBar,
    /// Whether rows and grid cells start at the right.
    rtl: bool,
    /// Hover and press of the rows in the viewport of a hosted view.
    tracked: Parts<ItemKey>,
}

/// The height of a `ListView` row: the minimum height of the row recipe,
/// or its text line and vertical padding when they are taller.
pub(crate) fn list_row_height(cx: &MeasureContext<'_>) -> f64 {
    let row = cx.style(StyleQuery::new(ControlKind::ListView, Slot::Row));
    row.min_height
        .max(row.text.line_height + row.padding.vertical())
        .max(1.0)
}

/// What a view mounted in a [`crate::ControlHost`] reads its rows from.
struct ItemSource {
    model: Box<dyn ItemModel>,
    delegate: Box<dyn ItemDelegate>,
    columns: Vec<f64>,
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
            type_ahead: crate::hosted::TypeAhead::default(),
            flat: true,
            reveal: false,
            source: None,
            styled_rows: false,
            caption: String::new(),
            footer: Vec::new(),
            bar: crate::OverlayScrollBar::new(crate::ScrollOrientation::Vertical),
            tracked: Parts::default(),
            rtl: false,
        }
    }

    /// Runs grid cells and tree indents from the right in a right-to-left
    /// layout, and swaps ArrowLeft and ArrowRight. A hosted view takes the
    /// direction from `arrange`.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.rtl = direction == LayoutDirection::Rtl;
    }

    /// The distance of `x` from the start edge of the viewport.
    fn inset(&self, x: f64) -> f64 {
        if self.rtl {
            self.viewport.max_x() - x
        } else {
            x - self.viewport.origin.x
        }
    }

    /// Adds a caption that a hosted view paints under its rows and its
    /// footer, with the `Caption` slot of the delegate's recipe. The
    /// caption describes the view for assistive technology.
    #[must_use]
    pub fn with_caption(mut self, caption: impl Into<String>) -> Self {
        self.caption = caption.into();
        self
    }

    /// Adds footer rows, for example totals, that a hosted view paints
    /// under its rows with the `Footer` slot of the `Table` recipe. Each
    /// row has one cell for each column. The footer rows do not scroll and
    /// take no selection.
    #[must_use]
    pub fn with_footer(mut self, rows: Vec<Vec<String>>) -> Self {
        self.footer = rows;
        self
    }

    /// Makes a hosted view take its row height from the `ListView` row
    /// recipe when it measures and arranges, in place of the height given
    /// to [`new`](Self::new).
    #[must_use]
    pub fn with_styled_rows(mut self) -> Self {
        self.styled_rows = true;
        self
    }

    /// Gives the view its own `model` and `delegate`, so it can mount in a
    /// [`crate::ControlHost`] as a single node. A hosted view realizes no
    /// child nodes: it paints the rows in the viewport itself.
    #[must_use]
    pub fn with_model(
        mut self,
        model: Box<dyn ItemModel>,
        delegate: Box<dyn ItemDelegate>,
    ) -> Self {
        self.dirty = true;
        self.source = Some(ItemSource {
            model,
            delegate,
            columns: Vec::new(),
        });
        self
    }

    /// The model given to [`with_model`](Self::with_model).
    pub fn model(&self) -> Option<&dyn ItemModel> {
        self.source.as_ref().map(|source| source.model.as_ref())
    }

    /// Sets the table column widths that a hosted view paints; empty
    /// means one column across the row. Returns false without a model.
    pub fn set_column_widths(&mut self, widths: Vec<f64>) -> bool {
        match &mut self.source {
            Some(source) => {
                source.columns = widths;
                true
            }
            None => false,
        }
    }

    /// Sets the list or grid layout. The view keeps its scroll offset and
    /// shows the current item at the next arrange.
    pub fn set_layout(&mut self, layout: ViewLayout) {
        if self.layout != layout {
            self.layout = layout;
            self.reveal = true;
        }
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
    /// that disappeared are dropped from selection and expansion. The view
    /// keeps its scroll offset, and when the rows change it shows the
    /// current item at the next arrange.
    pub fn sync(&mut self, model: &dyn ItemModel) -> bool {
        let revision = model.revision();
        if !self.dirty && self.revision == Some(revision) {
            return false;
        }
        self.revision = Some(revision);
        self.dirty = false;
        self.expanded.retain(|key| model.row(*key).is_some());
        let expanded = &self.expanded;
        let order = visible_keys(model, |key| expanded.contains(&key));
        self.reveal |= order != self.order;
        self.order = order;
        self.flat = self
            .order
            .iter()
            .all(|key| !model.has_children(*key) && !model.can_fetch_more(Some(*key)));
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
        let start = column as f64 * width;
        let x = if self.rtl {
            self.viewport.max_x() - start - width
        } else {
            self.viewport.origin.x + start
        };
        Rect::new(
            Point::new(
                x,
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
                let column = (self.inset(point.x) / item_width) as usize;
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
        self.reveal_current();
        self.realize(tree, id)
    }

    /// Scrolls the current item into view after a change of the rows or
    /// the layout.
    fn reveal_current(&mut self) {
        if std::mem::take(&mut self.reveal)
            && let Some(index) = self.current().and_then(|key| self.index_of(key))
        {
            self.scroll_to_index(index);
        }
    }

    /// Makes the first row in view current when no item is current, so a
    /// view that gets focus shows where the keys start.
    fn show_current(&mut self) -> bool {
        if self.current().is_some() || self.order.is_empty() {
            return false;
        }
        let line = (self.offset / self.row_height).ceil() as usize;
        let index = (line * self.columns_per_row()).min(self.order.len() - 1);
        let key = self.order[index];
        self.selection
            .apply(self.mode, SelectionCommand::CurrentOnly, key, &self.order);
        true
    }

    /// The indices of the rows in the viewport, plus the overscan.
    fn visible_range(&self) -> Range<usize> {
        let columns = self.columns_per_row();
        let first_line = (self.offset / self.row_height).floor() as usize;
        let last_line =
            ((self.offset + self.viewport.size.height) / self.row_height).ceil() as usize;
        let first = first_line.saturating_sub(self.overscan) * columns;
        let end = ((last_line + self.overscan) * columns).min(self.order.len());
        first..end.max(first)
    }

    fn realize(&mut self, tree: &mut Tree, id: NodeId) -> Result<(), TreeError> {
        let wanted: Vec<usize> = self.visible_range().collect();
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
                if self.index_at(position).is_none() && !self.selection.is_empty() {
                    // A click on empty space clears the selection.
                    self.selection.clear();
                    response.selection_changed = true;
                }
                if let Some(index) = self.index_at(position) {
                    let key = self.order[index];
                    let disclosure = self.layout == ViewLayout::List
                        && model.has_children(key)
                        && self.inset(position.x)
                            < (self.depth.get(&key).copied().unwrap_or(0) + 1) as f64 * self.indent;
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
        let index_of = &self.index_of;
        response.selection_changed |=
            self.selection
                .apply_indexed(self.mode, command, key, &self.order, |item| {
                    index_of.get(&item).copied()
                });
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
        let key = key.in_reading_order(self.rtl);
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
            // With no selection, the first vertical arrow selects the
            // current item that focus showed.
            (Key::ArrowDown | Key::ArrowUp, Some(index)) if self.selection.is_empty() => {
                Some(index)
            }
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
        let current = self.selection.current().and_then(|key| self.index_of(key));
        let order = &self.order;
        let found = self.type_ahead.find(
            character.encode_utf8(&mut [0; 4]),
            at.as_secs_f64(),
            current,
            order.len(),
            |index| {
                model
                    .data(order[index], 0, Role::Display)
                    .to_display_string()
            },
        );
        if let Some(index) = found {
            self.select(self.order[index], SelectionCommand::Replace, response);
        }
    }

    /// Semantic node for the view container.
    pub fn semantics(&self) -> SemanticNode {
        // The footer rows are not items, so the description reads them
        // after the caption.
        let footer = self.footer.iter().map(|row| {
            row.iter()
                .filter(|cell| !cell.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        });
        // Each part ends as a sentence, unless it already has a full stop.
        let mut description = String::new();
        for part in std::iter::once(self.caption.clone())
            .chain(footer)
            .filter(|part| !part.is_empty())
        {
            if !description.is_empty() {
                if !description.ends_with(['.', '!', '?']) {
                    description.push('.');
                }
                description.push(' ');
            }
            description.push_str(&part);
        }
        SemanticNode {
            role: self.role,
            name: self.name.clone(),
            description,
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
        self.key_semantics(model, item.key)
    }

    /// Semantic node for the item with `key`.
    fn key_semantics(&self, model: &dyn ItemModel, key: ItemKey) -> SemanticNode {
        let has_children = model.has_children(key) || model.can_fetch_more(Some(key));
        let role = match self.role {
            SemanticRole::Tree => SemanticRole::TreeItem,
            SemanticRole::Table => SemanticRole::Row,
            _ => SemanticRole::ListItem,
        };
        let mut actions = vec![SemanticAction::Activate, SemanticAction::Focus];
        if has_children {
            actions.push(if self.expanded.contains(&key) {
                SemanticAction::Collapse
            } else {
                SemanticAction::Expand
            });
        }
        actions.push(SemanticAction::ShowMenu);
        let accessible = model.data(key, 0, Role::Accessibility);
        let name = if accessible.is_none() {
            model.data(key, 0, Role::Display).to_display_string()
        } else {
            accessible.to_display_string()
        };
        let value = (1..model.column_count())
            .map(|column| model.data(key, column, Role::Display).to_display_string())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        SemanticNode {
            role,
            name,
            description: String::new(),
            value: (!value.is_empty()).then_some(value),
            state: SemanticState {
                selected: Some(self.selection.is_selected(key)),
                expanded: has_children.then_some(self.expanded.contains(&key)),
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
    /// column widths; empty means one column spanning the item. `state`
    /// carries the view's focus flags.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        model: &dyn ItemModel,
        delegate: &dyn ItemDelegate,
        columns: &[f64],
        state: VisualState,
    ) {
        let rows = self
            .realized
            .iter()
            .map(|item| (item.key, item.depth, item.bounds));
        self.paint_rows(cx, model, delegate, columns, state, rows);
    }

    /// Paints `rows` (key, depth, and bounds) clipped to the viewport.
    fn paint_rows(
        &self,
        cx: &mut PaintContext<'_>,
        model: &dyn ItemModel,
        delegate: &dyn ItemDelegate,
        columns: &[f64],
        state: VisualState,
        rows: impl Iterator<Item = (ItemKey, usize, Rect)>,
    ) {
        cx.list.push_clip(self.viewport);
        for (key, depth, bounds) in rows {
            let item_state = ItemState {
                selected: self.selection.is_selected(key),
                current: self.selection.current() == Some(key),
                focused: state.focused(),
                focus_visible: state.focus_visible(),
                depth,
                // Rows of a flat model reserve no disclosure gap.
                indent: if self.flat { 0.0 } else { self.indent },
                has_children: model.has_children(key),
                expanded: self.expanded.contains(&key),
                pointer: self.tracked.state(key, state),
            };
            if columns.is_empty() {
                delegate.paint_cell(&mut cx.at(bounds), model, key, 0, item_state);
            } else {
                let mut x = bounds.origin.x;
                for (column, width) in columns.iter().enumerate() {
                    let rect = Rect::new(
                        Point::new(x, bounds.origin.y),
                        Size::new(*width, bounds.size.height),
                    );
                    delegate.paint_cell(&mut cx.at(rect), model, key, column, item_state);
                    x += width;
                }
            }
        }
        let _ = cx.list.pop_clip();
    }
}

/// Default text delegate with the list or table look: the row fill for
/// hover and selection, the focus ring on the current item, a tree
/// disclosure chevron, and single-line text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextItemDelegate {
    /// `ListView` for lists and trees, `Table` for table rows.
    pub kind: ControlKind,
}

impl Default for TextItemDelegate {
    fn default() -> Self {
        Self {
            kind: ControlKind::ListView,
        }
    }
}

impl TextItemDelegate {
    /// The delegate with the table row look.
    pub fn table() -> Self {
        Self {
            kind: ControlKind::Table,
        }
    }
}

impl ItemDelegate for TextItemDelegate {
    fn paint_cell(
        &self,
        cx: &mut PaintContext<'_>,
        model: &dyn ItemModel,
        key: ItemKey,
        column: usize,
        state: ItemState,
    ) {
        let rect = cx.bounds;
        let flags = state
            .pointer
            .set(VisualState::SELECTED, state.selected)
            .set(VisualState::HIGHLIGHTED, state.selected && state.focused);
        let query = |slot| StyleQuery::new(self.kind, slot).state(flags);
        let (row, cell, indicator) = (
            cx.style(query(Slot::Row)),
            cx.style(query(Slot::Cell)),
            cx.style(query(Slot::Indicator)),
        );
        cx.paint_box(rect, &row);
        let mut text = content_rect(rect, &cell, cx.rtl);
        if column == 0 {
            let indent = state.depth as f64 * state.indent;
            let mut start = |width: f64| {
                let width = width.min(text.size.width);
                let x = if cx.rtl {
                    text.max_x() - width
                } else {
                    text.origin.x
                };
                if !cx.rtl {
                    text.origin.x += width;
                }
                text.size.width -= width;
                Rect::from_xywh(x, text.origin.y, width, text.size.height)
            };
            start(indent);
            let disclosure = start(state.indent);
            if state.has_children {
                let icon = match (state.expanded, cx.rtl) {
                    (true, _) => Icon::ChevronDown,
                    (false, false) => Icon::ChevronRight,
                    (false, true) => Icon::ChevronLeft,
                };
                cx.icon(disclosure, icon, &indicator);
            }
        }
        let label = model.data(key, column, Role::Display).to_display_string();
        if !label.is_empty() {
            cx.list.push_clip(rect);
            let line = TextLine::new(&label, &cell, cx.rtl).color(row.foreground);
            draw_text(cx.list, text, &line, cx.text);
            let _ = cx.list.pop_clip();
        }
        if state.current && state.focus_visible && column == 0 {
            let focus = StyleQuery::new(self.kind, Slot::Root).state(VisualState::FOCUS_VISIBLE);
            let ring = cx.style(focus);
            cx.slot(rect, &ring, |_, _| {});
        }
    }
}

/// The keys a focused hosted view consumes.
fn view_key(key: Key, modifiers: ItemModifiers) -> bool {
    match key {
        Key::ArrowUp
        | Key::ArrowDown
        | Key::ArrowLeft
        | Key::ArrowRight
        | Key::PageUp
        | Key::PageDown
        | Key::Home
        | Key::End
        | Key::Enter
        | Key::Space => true,
        Key::Character('a' | 'A') => modifiers.command,
        _ => false,
    }
}

impl ItemView {
    /// Runs `input` against the hosted model and reports the response as
    /// messages. Returns the response, or `None` without a model.
    fn hosted_input(
        &mut self,
        cx: &mut ControlContext<'_>,
        input: ItemViewInput,
    ) -> Option<ItemViewResponse> {
        let source = self.source.take()?;
        let node = cx.node();
        let response = self.handle(cx.tree_mut(), node, source.model.as_ref(), input);
        self.source = Some(source);
        let response = response.ok()?;
        if response.selection_changed {
            cx.emit(ControlMessage::Changed(ControlValue::Indices(
                self.selected_indices(),
            )));
        }
        if let Some(index) = response.activated.and_then(|key| self.index_of(key)) {
            cx.emit(ControlMessage::ItemActivated(index));
        }
        if response.selection_changed || response.current_changed || response.expansion_changed {
            cx.request_semantics();
        }
        cx.request_paint();
        Some(response)
    }

    /// Rebuilds the visible order from the hosted model.
    fn sync_source(&mut self) {
        if let Some(source) = self.source.take() {
            self.sync(source.model.as_ref());
            self.source = Some(source);
        }
    }

    /// The selected items' positions in the visible order, ascending.
    fn selected_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .selection
            .keys()
            .filter_map(|key| self.index_of(key))
            .collect();
        indices.sort_unstable();
        indices
    }
}

impl ItemView {
    /// The `Caption` style of the delegate's recipe: `Table` for a table,
    /// `ListView` otherwise.
    fn caption_query(&self) -> StyleQuery {
        let kind = if self.role == SemanticRole::Table {
            ControlKind::Table
        } else {
            ControlKind::ListView
        };
        StyleQuery::new(kind, Slot::Caption)
    }

    fn footer_height(&self) -> f64 {
        self.footer.len() as f64 * self.row_height
    }

    /// The caption line and the space above and below it, or zero without
    /// a caption.
    fn caption_height(&self, cx: &MeasureContext<'_>) -> f64 {
        if self.caption.is_empty() {
            return 0.0;
        }
        let style = cx.style(self.caption_query());
        style.text.line_height + style.padding.vertical()
    }

    /// Paints the footer rows right under the last row, or at the bottom
    /// of the viewport when the rows fill it, then the caption.
    fn paint_footer(&self, cx: &mut PaintContext<'_>, columns: &[f64]) {
        let view = self.viewport;
        let mut top = view.origin.y + self.content_height().min(view.size.height);
        if !self.footer.is_empty() {
            let query = |slot| StyleQuery::new(ControlKind::Table, slot);
            let (footer, cell) = (cx.style(query(Slot::Footer)), cx.style(query(Slot::Cell)));
            let area = Rect::from_xywh(view.origin.x, top, view.size.width, self.footer_height());
            cx.paint_box(area, &footer);
            for row in &self.footer {
                let widths = if columns.is_empty() {
                    vec![view.size.width]
                } else {
                    columns.to_vec()
                };
                let mut x = view.origin.x;
                for (text, width) in row.iter().zip(widths) {
                    let rect = Rect::from_xywh(x, top, width, self.row_height);
                    let line = TextLine::new(text, &footer, cx.rtl);
                    cx.list.push_clip(rect);
                    draw_text(cx.list, content_rect(rect, &cell, cx.rtl), &line, cx.text);
                    let _ = cx.list.pop_clip();
                    x += width;
                }
                top += self.row_height;
            }
        }
        if !self.caption.is_empty() {
            let style = cx.style(self.caption_query());
            let height = style.text.line_height + style.padding.vertical();
            let rect = Rect::from_xywh(view.origin.x, top, view.size.width, height);
            let inner = content_rect(rect, &style, cx.rtl);
            cx.label(inner, &self.caption, &style, TextAlign::Center);
        }
    }
}

impl CustomControl for ItemView {
    /// A view takes the offered size; unbounded, it is as wide as a medium
    /// container and ten rows high.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        if self.styled_rows {
            self.row_height = list_row_height(cx);
        }
        let max = cx.constraints.max;
        let width = if max.width.is_finite() {
            max.width
        } else {
            cx.tokens.spacing * 120.0
        };
        let height = if max.height.is_finite() {
            max.height
        } else {
            self.row_height * 10.0 + self.footer_height() + self.caption_height(cx)
        };
        Measured::new(Size::new(width, height))
    }

    /// The rows take the bounds above the footer rows and the caption.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        if self.styled_rows {
            self.row_height = list_row_height(cx);
        }
        self.sync_source();
        self.rtl = cx.rtl();
        let below = self.footer_height() + self.caption_height(cx);
        let mut viewport = bounds;
        viewport.size.height = (bounds.size.height - below).max(0.0);
        self.viewport = viewport;
        self.clamp_offset();
        self.reveal_current();
        self.bar.arrange(cx, viewport);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let shortcut = cx.modifiers();
        let modifiers = ItemModifiers {
            shift: shortcut.shift,
            command: shortcut.command,
        };
        // The scroll bar takes the pointer before the rows under it.
        if let ControlEvent::Pointer(pointer) = event {
            self.bar.set_scroll(self.content_height(), self.offset);
            if let Some(scrolled) = self.bar.hosted_pointer(cx, pointer) {
                if scrolled {
                    self.offset = self.bar.offset();
                }
                return EventReply::Stop;
            }
        }
        let mut tracked = self.tracked;
        tracked.track(cx, event, |point| self.key_at(point));
        self.tracked = tracked;
        match event {
            ControlEvent::Pointer(PointerEvent::Down { position, button })
                if self.viewport.contains(*position) =>
            {
                let input = match button {
                    PointerButton::Primary => ItemViewInput::Press {
                        position: *position,
                        modifiers,
                        clicks: cx.click_count(),
                    },
                    _ => ItemViewInput::ContextMenu {
                        position: *position,
                    },
                };
                self.hosted_input(cx, input);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Wheel { delta, unit, .. }) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => self.row_height,
                };
                if delta.y == 0.0 {
                    return EventReply::Continue;
                }
                let input = ItemViewInput::Wheel {
                    delta_y: delta.y * scale,
                };
                match self.hosted_input(cx, input) {
                    Some(response) if response.scrolled => {
                        // Other rows move under the pointer.
                        self.tracked.hovered = None;
                        EventReply::Stop
                    }
                    _ => EventReply::Continue,
                }
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) if view_key(*key, modifiers) => {
                let input = ItemViewInput::Key {
                    key: *key,
                    modifiers,
                };
                self.hosted_input(cx, input);
                EventReply::Stop
            }
            ControlEvent::Text(text) => {
                let at = Duration::from_secs_f64(cx.time().max(0.0));
                for character in text.chars() {
                    self.hosted_input(cx, ItemViewInput::Type { character, at });
                }
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                if *focused && self.show_current() {
                    cx.request_semantics();
                }
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Hover(false) | ControlEvent::Dismiss => {
                if self.bar.release() {
                    cx.request_paint();
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints only the rows in the viewport, plus the overscan, and the
    /// overlay scroll bar above them.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let Some(source) = &self.source else {
            return;
        };
        if self.order.is_empty() {
            // An empty view says that no row matches.
            let style = cx.style(self.caption_query());
            let text = cx.strings.message("choscorui.list.no_results", &[]);
            cx.label(self.viewport, &text, &style, TextAlign::Center);
        }
        let rows = self.visible_range().map(|index| {
            let key = self.order[index];
            let depth = self.depth.get(&key).copied().unwrap_or(0);
            (key, depth, self.item_rect(index))
        });
        let state = cx.state;
        self.paint_rows(
            cx,
            source.model.as_ref(),
            source.delegate.as_ref(),
            &source.columns,
            state,
            rows,
        );
        self.paint_footer(cx, &source.columns);
        let mut bar = self.bar.clone();
        bar.set_scroll(self.content_height(), self.offset);
        bar.paint(cx);
    }

    fn semantics(&self) -> SemanticNode {
        ItemView::semantics(self)
    }

    /// A part for each row in view, with its absolute index, and the row
    /// count as the collection. The current row is the focused part.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        let Some(source) = &self.source else {
            return;
        };
        let model = source.model.as_ref();
        parts.set_collection(SemanticCollection {
            item_count: self.order.len(),
            column_count: (model.column_count() > 1).then_some(model.column_count()),
        });
        let current = self.current();
        for index in self.visible_range() {
            let key = self.order[index];
            let part = parts.push(None, SemanticRole::ListItem, "", self.item_rect(index));
            part.node = self.key_semantics(model, key);
            part.item(index);
            if current == Some(key) {
                parts.set_focused(Some(parts.len() - 1));
            }
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action != SemanticAction::Focus {
            return false;
        }
        cx.request_focus();
        true
    }

    /// Takes `Indices`: the selected positions in the visible order.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(indices) = value else {
            return false;
        };
        self.sync_source();
        let fits = match self.mode {
            SelectionMode::None => indices.is_empty(),
            SelectionMode::Single => indices.len() <= 1,
            SelectionMode::Extended | SelectionMode::Multi => true,
        };
        if !fits || indices.iter().any(|index| *index >= self.order.len()) {
            return false;
        }
        // The same selection again, as a host pushes it with each new
        // configuration, keeps the current row.
        let mut keys = std::collections::BTreeSet::new();
        keys.extend(indices.iter().map(|index| self.order[*index]));
        if keys.len() == self.selection.len()
            && keys.iter().all(|key| self.selection.is_selected(*key))
        {
            return true;
        }
        self.selection.clear();
        for index in indices {
            let key = self.order[*index];
            if !self.selection.is_selected(key) {
                self.selection.apply(
                    SelectionMode::Multi,
                    SelectionCommand::Toggle,
                    key,
                    &self.order,
                );
            }
        }
        true
    }
}
