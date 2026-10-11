//! Model-backed views: the virtualized item view, table headers, cell
//! editing, form mapping, column browsing, undo history, and the canvas.

use crate::page::{
    Align, Collection, Column, Env, Page, PageState, Push, node, paint_field, pressed, sem, split,
    type_into,
};
use choscorui::controls::{
    CanvasInput, CanvasView, CellEdit, ColumnBrowser, DataMapper, DefaultEditorFactory,
    HeaderColumn, HeaderInput, HeaderView, ItemModifiers, ItemView, ItemViewInput,
    ItemViewResponse, ShortcutModifiers, TextItemDelegate, ValidationState, ViewLayout,
    VisualState, undo_history, undo_steps_to,
};
use choscorui::core::{
    Color, ColorRole, DisplayList, ItemData, ItemKey, ItemModel, ItemShape, ItemStyle, Key,
    KeyEvent, MatchMode, ModelError, NodeId, Point, PointerButton, PointerEvent, Rect, Role, Scene,
    SceneItem, SceneItemId, SemanticAction, SemanticCollection, SemanticNode, SemanticRole, Size,
    SortFilterProxy, StringListModel, TextFilter, Tree, TreeStore, UndoStack, Value, WheelUnit,
};
use std::cell::{Cell, RefCell};

const ROW: f64 = 24.0;

fn item_modifiers(modifiers: ShortcutModifiers) -> ItemModifiers {
    ItemModifiers {
        shift: modifiers.shift,
        command: modifiers.command,
    }
}

fn wheel_points(delta: Point, unit: WheelUnit) -> Point {
    match unit {
        WheelUnit::Lines => Point::new(delta.x * ROW, delta.y * ROW),
        WheelUnit::Points => delta,
    }
}

/// An [`ItemView`] bound to one tree node, with the routing every list on
/// these pages shares.
struct ListBox {
    node: NodeId,
    view: ItemView,
    rect: Rect,
}

impl ListBox {
    fn new(tree: &mut Tree, parent: NodeId, name: &str, role: SemanticRole) -> Self {
        Self {
            node: node(tree, parent, true),
            view: ItemView::new(name, role, ROW),
            rect: Rect::default(),
        }
    }

    fn arrange(&mut self, tree: &mut Tree, rect: Rect) {
        self.rect = rect;
        let _ = tree.set_bounds(self.node, rect);
        let _ = self.view.arrange(tree, self.node, rect);
    }

    fn input(
        &mut self,
        tree: &mut Tree,
        model: &dyn ItemModel,
        input: ItemViewInput,
    ) -> ItemViewResponse {
        let response = self
            .view
            .handle(tree, self.node, model, input)
            .unwrap_or_default();
        if response.scrolled || response.expansion_changed {
            let _ = self.view.arrange(tree, self.node, self.rect);
        }
        response
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        model: &dyn ItemModel,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        clicks: u32,
    ) -> Option<ItemViewResponse> {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if self.rect.contains(position) => Some(self.input(
                tree,
                model,
                ItemViewInput::Press {
                    position,
                    modifiers: item_modifiers(modifiers),
                    clicks,
                },
            )),
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.rect.contains(position) => Some(self.input(
                tree,
                model,
                ItemViewInput::Wheel {
                    delta_y: wheel_points(delta, unit).y,
                },
            )),
            _ => None,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        model: &dyn ItemModel,
        key: Key,
        modifiers: ShortcutModifiers,
    ) -> Option<ItemViewResponse> {
        (tree.focus() == Some(self.node)).then(|| {
            self.input(
                tree,
                model,
                ItemViewInput::Key {
                    key,
                    modifiers: item_modifiers(modifiers),
                },
            )
        })
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        model: &dyn ItemModel,
        node: NodeId,
        action: SemanticAction,
    ) -> bool {
        if node == self.node && action == SemanticAction::Focus {
            return tree.set_focus(Some(self.node)).is_ok();
        }
        match self.view.realized().iter().find(|item| item.node == node) {
            Some(item) => {
                let key = item.key;
                self.input(tree, model, ItemViewInput::Semantic { key, action });
                true
            }
            None => false,
        }
    }

    fn semantics(&self, model: &dyn ItemModel) -> Vec<(NodeId, SemanticNode)> {
        std::iter::once((self.node, self.view.semantics()))
            .chain(
                self.view
                    .realized()
                    .iter()
                    .map(|item| (item.node, self.view.item_semantics(model, item))),
            )
            .collect()
    }

    fn collection(&self, model: &dyn ItemModel) -> Collection {
        Collection {
            node: self.node,
            info: SemanticCollection {
                item_count: self.view.len(),
                column_count: (model.column_count() > 1).then_some(model.column_count()),
            },
            items: self
                .view
                .realized()
                .iter()
                .map(|item| (item.node, item.index))
                .collect(),
        }
    }

    fn paint(
        &self,
        tree: &Tree,
        list: &mut DisplayList,
        model: &dyn ItemModel,
        env: &Env<'_>,
        columns: &[f64],
    ) {
        env.panel(list, self.rect);
        let state = VisualState::NONE
            .set(VisualState::FOCUSED, tree.focus() == Some(self.node))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(self.node));
        let delegate = if columns.is_empty() {
            TextItemDelegate::default()
        } else {
            TextItemDelegate::table()
        };
        self.view.paint(
            &mut env.cx(list, self.rect),
            model,
            &delegate,
            columns,
            state,
        );
    }
}

/// A search field: a focusable node and its text.
struct Search {
    node: NodeId,
    text: String,
}

impl Search {
    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>, placeholder: &str) {
        paint_field(
            env,
            list,
            tree.bounds(self.node).unwrap_or_default(),
            &self.text,
            placeholder,
            tree.focus() == Some(self.node),
            &ValidationState::Valid,
            None,
        );
    }
}

// ---------------------------------------------------------------------------
// Item view

pub const RECORDS: usize = 100_000;
const CITIES: [&str; 6] = ["Hà Nội", "Huế", "Đà Lạt", "Hội An", "Sa Pa", "Tel Aviv"];

/// 100,000 records behind a folded (case- and diacritic-insensitive)
/// search, in list or grid layout. Only visible rows are realized.
pub struct ItemViewPage {
    search: Search,
    layout_toggle: Push,
    count: NodeId,
    list: ListBox,
    model: StringListModel,
    proxy: SortFilterProxy,
    grid: bool,
}

pub fn item_view(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let search = Search {
        node: node(tree, parent, true),
        text: String::new(),
    };
    let layout_toggle = Push::new(tree, parent, "Grid layout", env);
    let count = node(tree, parent, false);
    let mut list = ListBox::new(tree, parent, "Records", SemanticRole::List);
    let model = StringListModel::from_strings((0..RECORDS).map(|i| {
        format!(
            "Record {i:06} · {} · {}.{:02} ₫",
            CITIES[i % CITIES.len()],
            (i * 37) % 10_000,
            i % 100
        )
    }));
    let mut proxy = SortFilterProxy::new();
    proxy.sync(&model);
    list.view
        .set_selection_mode(choscorui::core::SelectionMode::Extended);
    list.view.sync(&proxy.view(&model));
    Box::new(ItemViewPage {
        search,
        layout_toggle,
        count,
        list,
        model,
        proxy,
        grid: false,
    })
}

impl ItemViewPage {
    /// Filters the records; matching ignores case and diacritics.
    pub fn search(&mut self, tree: &mut Tree, text: &str) {
        self.search.text = text.to_owned();
        self.apply_search(tree);
    }

    fn apply_search(&mut self, tree: &mut Tree) {
        let text = self.search.text.trim();
        self.proxy
            .set_filter((!text.is_empty()).then(|| TextFilter {
                column: None,
                pattern: text.to_owned(),
                mode: MatchMode::Contains,
            }));
        self.proxy.sync(&self.model);
        self.list.view.sync(&self.proxy.view(&self.model));
        let rect = self.list.rect;
        self.list.arrange(tree, rect);
    }

    /// Records matching the search.
    pub fn matches(&self) -> usize {
        self.list.view.len()
    }

    /// Rows that currently have tree nodes.
    pub fn realized(&self) -> usize {
        self.list.view.realized().len()
    }

    /// Focuses the list and presses Page Down.
    #[cfg(target_os = "macos")]
    pub fn page_down(&mut self, tree: &mut Tree) {
        let _ = tree.set_focus(Some(self.list.node));
        let view = self.proxy.view(&self.model);
        self.list
            .key(tree, &view, Key::PageDown, ShortcutModifiers::default());
    }

    fn set_grid(&mut self, tree: &mut Tree, grid: bool) {
        self.grid = grid;
        self.layout_toggle.label = if grid { "List layout" } else { "Grid layout" }.into();
        self.list.view.set_layout(if grid {
            ViewLayout::Grid { item_width: 300.0 }
        } else {
            ViewLayout::List
        });
        let rect = self.list.rect;
        self.list.arrange(tree, rect);
    }
}

impl Page for ItemViewPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(32.0, &[300.0, 120.0, 0.0]);
        let _ = tree.set_bounds(self.search.node, cells[0]);
        let _ = tree.set_bounds(self.layout_toggle.node, cells[1]);
        let _ = tree.set_bounds(self.count, cells[2]);
        let rest = column.rest();
        self.list.arrange(tree, rest);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.search
            .paint(tree, list, env, "Search (try “hoi an” or “tel”)");
        self.layout_toggle.paint(tree, list, env);
        env.caption(
            list,
            tree.bounds(self.count).unwrap_or_default(),
            &format!(
                "{} of {RECORDS} records · {} rows realized",
                self.matches(),
                self.realized()
            ),
        );
        let view = self.proxy.view(&self.model);
        self.list.paint(tree, list, &view, env, &[]);
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let view = self.proxy.view(&self.model);
        let mut nodes = self.list.semantics(&view);
        let mut search = sem(
            SemanticRole::TextField,
            "Search records",
            Some(self.search.text.clone()),
        );
        search.actions = vec![SemanticAction::Focus, SemanticAction::SetValue];
        nodes.push((self.search.node, search));
        nodes.push(self.layout_toggle.semantics(tree));
        nodes
    }

    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        vec![self.list.collection(&self.proxy.view(&self.model))]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.layout_toggle.pointer(tree, event) {
            self.set_grid(tree, !self.grid);
            return true;
        }
        if let PointerEvent::Down { position, .. } = event
            && tree
                .bounds(self.search.node)
                .is_some_and(|b| b.contains(position))
        {
            return tree.set_focus(Some(self.search.node)).is_ok();
        }
        let view = self.proxy.view(&self.model);
        self.list
            .pointer(tree, &view, event, modifiers, 1)
            .is_some()
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.layout_toggle.key(tree, event) {
            self.set_grid(tree, !self.grid);
            return true;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        if tree.focus() == Some(self.search.node) {
            return match key {
                Key::Backspace | Key::Delete => {
                    let changed = self.search.text.pop().is_some();
                    self.apply_search(tree);
                    changed
                }
                Key::Escape => {
                    self.search(tree, "");
                    true
                }
                Key::ArrowDown | Key::Enter => tree.set_focus(Some(self.list.node)).is_ok(),
                _ => false,
            };
        }
        if key == Key::Tab {
            return false;
        }
        let view = self.proxy.view(&self.model);
        self.list.key(tree, &view, key, modifiers).is_some()
    }

    fn text(&mut self, tree: &mut Tree, text: &str, env: &Env<'_>) -> bool {
        if tree.focus() == Some(self.search.node) {
            let changed = type_into(&mut self.search.text, text);
            self.apply_search(tree);
            return changed;
        }
        if tree.focus() != Some(self.list.node) {
            return false;
        }
        let view = self.proxy.view(&self.model);
        let mut handled = false;
        for character in text.chars().filter(|c| !c.is_control()) {
            self.list.input(
                tree,
                &view,
                ItemViewInput::Type {
                    character,
                    at: env.now,
                },
            );
            handled = true;
        }
        handled
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if node == self.search.node {
            return match action {
                SemanticAction::SetValue => {
                    self.search(tree, value.unwrap_or_default());
                    true
                }
                SemanticAction::Focus => tree.set_focus(Some(node)).is_ok(),
                _ => false,
            };
        }
        if self.layout_toggle.action(tree, node, action) {
            self.set_grid(tree, !self.grid);
            return true;
        }
        let view = self.proxy.view(&self.model);
        self.list.action(tree, &view, node, action)
    }
}

// ---------------------------------------------------------------------------
// Products: the typed table shared by the header, cell-edit, and mapper pages

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub name: String,
    pub quantity: i64,
    pub price: f64,
    pub in_stock: bool,
}

impl ItemData for Product {
    fn data(&self, column: usize, role: Role) -> Value {
        let display = matches!(role, Role::Display | Role::Accessibility);
        match column {
            0 if display || matches!(role, Role::Edit | Role::Sort) => {
                Value::Text(self.name.clone())
            }
            1 if display => Value::Text(self.quantity.to_string()),
            1 if matches!(role, Role::Edit | Role::Sort) => Value::Int(self.quantity),
            2 if display => Value::Text(format!("{:.2}", self.price)),
            2 if matches!(role, Role::Edit | Role::Sort) => Value::Float(self.price),
            3 if display => Value::Text(if self.in_stock { "Yes" } else { "No" }.into()),
            3 if matches!(role, Role::Edit | Role::Sort) => Value::Bool(self.in_stock),
            _ => Value::None,
        }
    }

    fn set_data(&mut self, column: usize, _role: Role, value: Value) -> Result<(), ModelError> {
        match (column, value) {
            (0, Value::Text(name)) if !name.trim().is_empty() => self.name = name,
            (1, Value::Int(quantity)) if quantity >= 0 => self.quantity = quantity,
            (2, Value::Float(price)) if price >= 0.0 => self.price = price,
            (3, Value::Bool(in_stock)) => self.in_stock = in_stock,
            (0..=3, _) => return Err(ModelError::InvalidValue),
            _ => return Err(ModelError::InvalidColumn),
        }
        Ok(())
    }

    fn editable(&self, column: usize) -> bool {
        column < 4
    }
}

const PRODUCT_NAMES: [&str; 12] = [
    "Cà phê sữa đá",
    "Bánh mì",
    "Phở bò",
    "Trà sen",
    "Chè ba màu",
    "Bún chả",
    "Gỏi cuốn",
    "Cơm tấm",
    "Nước mía",
    "Bánh xèo",
    "Xôi gấc",
    "Hủ tiếu",
];

fn products(count: usize) -> TreeStore<Product> {
    let mut store = TreeStore::new(4);
    store.set_headers(["Name", "Quantity", "Price", "In stock"]);
    let rows = (0..count).map(|i| Product {
        name: if i < PRODUCT_NAMES.len() {
            PRODUCT_NAMES[i].to_owned()
        } else {
            format!(
                "{} #{}",
                PRODUCT_NAMES[i % PRODUCT_NAMES.len()],
                i / PRODUCT_NAMES.len() + 1
            )
        },
        quantity: ((i * 7) % 50) as i64,
        price: 1.5 + (i * 13 % 40) as f64 * 0.75,
        in_stock: i % 4 != 3,
    });
    store.insert(None, 0, rows).expect("root insert is valid");
    store
}

fn product_header() -> HeaderView {
    HeaderView::new(
        ["Name", "Quantity", "Price", "In stock"]
            .iter()
            .zip([200.0, 100.0, 100.0, 90.0])
            .map(|(title, width)| HeaderColumn {
                title: (*title).to_owned(),
                width,
                min_width: 50.0,
            })
            .collect(),
    )
}

/// A header node and its view, with pointer resizing and sorting.
struct Header {
    node: NodeId,
    view: HeaderView,
    dragging: bool,
}

impl Header {
    fn new(tree: &mut Tree, parent: NodeId) -> Self {
        Self {
            node: node(tree, parent, true),
            view: product_header(),
            dragging: false,
        }
    }

    fn arrange(&mut self, tree: &mut Tree, rect: Rect) {
        let _ = tree.set_bounds(self.node, rect);
        self.view.set_origin_x(rect.origin.x);
    }

    /// Returns whether the sort changed (`Some(true)`), something else
    /// changed (`Some(false)`), or the event was not for the header.
    fn pointer(&mut self, tree: &mut Tree, event: PointerEvent) -> Option<bool> {
        let rect = tree.bounds(self.node).unwrap_or_default();
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if rect.contains(position) => {
                let _ = tree.set_focus(Some(self.node));
                let _ = tree.set_pointer_capture(Some(self.node));
                self.dragging = true;
                Some(
                    self.view
                        .handle(HeaderInput::Press { x: position.x })
                        .sort_changed,
                )
            }
            PointerEvent::Move { position } if self.dragging => {
                self.view.handle(HeaderInput::Drag { x: position.x });
                Some(false)
            }
            PointerEvent::Up { .. } if self.dragging => {
                self.dragging = false;
                let _ = tree.set_pointer_capture(None);
                self.view.handle(HeaderInput::Release);
                Some(false)
            }
            _ => None,
        }
    }

    fn key(&mut self, tree: &Tree, key: Key) -> Option<bool> {
        (tree.focus() == Some(self.node))
            .then(|| self.view.handle(HeaderInput::Key(key)).sort_changed)
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let rect = tree.bounds(self.node).unwrap_or_default();
        self.view
            .paint(&mut env.cx(list, rect), tree.focus_visible(self.node));
    }

    fn semantics(&self) -> Vec<(NodeId, SemanticNode)> {
        // One node stands for the header; its value names the focused
        // column and sort so VoiceOver reads changes as the user moves.
        let focused = self.view.focused_column();
        let column = self.view.column_semantics(focused);
        let mut node = sem(
            SemanticRole::Group,
            "Column headers",
            column.as_ref().map(|column| match &column.value {
                Some(order) => format!("{}, sorted {order}", column.name),
                None => column.name.clone(),
            }),
        );
        node.description = "Left and Right move; Space sorts; + and - resize".into();
        node.actions = vec![SemanticAction::Focus];
        vec![(self.node, node)]
    }
}

/// A sortable, resizable table header over 60 products.
pub struct HeaderViewPage {
    header: Header,
    list: ListBox,
    model: TreeStore<Product>,
    proxy: SortFilterProxy,
}

pub fn header_view(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let header = Header::new(tree, parent);
    let mut list = ListBox::new(tree, parent, "Products", SemanticRole::Table);
    let model = products(60);
    let mut proxy = SortFilterProxy::new();
    proxy.sync(&model);
    list.view.sync(&proxy.view(&model));
    Box::new(HeaderViewPage {
        header,
        list,
        model,
        proxy,
    })
}

impl HeaderViewPage {
    fn resort(&mut self, tree: &mut Tree) {
        self.proxy.set_sort(self.header.view.sort());
        self.proxy.sync(&self.model);
        self.list.view.sync(&self.proxy.view(&self.model));
        let rect = self.list.rect;
        self.list.arrange(tree, rect);
    }
}

fn table_width(header: &HeaderView) -> f64 {
    header.widths().iter().sum()
}

impl Page for HeaderViewPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let width = (table_width(&self.header.view) + 40.0).min(bounds.size.width);
        let mut column = Column::new(bounds, env);
        column.gap = 0.0;
        self.header.arrange(tree, column.cells(28.0, &[width])[0]);
        let rest = column.rest();
        let rect = split(rest, &[width], 0.0, env.rtl())[0];
        self.list.arrange(tree, rect);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let view = self.proxy.view(&self.model);
        self.list
            .paint(tree, list, &view, env, &self.header.view.widths());
        self.header.paint(tree, list, env);
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = self.header.semantics();
        nodes.extend(self.list.semantics(&self.proxy.view(&self.model)));
        nodes
    }

    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        vec![self.list.collection(&self.proxy.view(&self.model))]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match self.header.pointer(tree, event) {
            Some(true) => {
                self.resort(tree);
                true
            }
            Some(false) => true,
            None => {
                let view = self.proxy.view(&self.model);
                self.list
                    .pointer(tree, &view, event, modifiers, 1)
                    .is_some()
            }
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        if key == Key::Tab {
            return false;
        }
        match self.header.key(tree, key) {
            Some(true) => {
                self.resort(tree);
                true
            }
            Some(false) => true,
            None => {
                let view = self.proxy.view(&self.model);
                self.list.key(tree, &view, key, modifiers).is_some()
            }
        }
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if node == self.header.node {
            return action == SemanticAction::Focus && tree.set_focus(Some(node)).is_ok();
        }
        let view = self.proxy.view(&self.model);
        self.list.action(tree, &view, node, action)
    }
}

// ---------------------------------------------------------------------------
// Cell editing

/// A product table edited in place: Left and Right (or a click) choose
/// the column, Return edits, Return commits, Escape cancels.
pub struct CellEditPage {
    header: Header,
    list: ListBox,
    model: TreeStore<Product>,
    column: usize,
    edit: Option<CellEdit>,
    error: Option<String>,
}

pub fn cell_edit(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let header = Header::new(tree, parent);
    let mut list = ListBox::new(tree, parent, "Editable products", SemanticRole::Table);
    let model = products(30);
    list.view.sync(&model);
    Box::new(CellEditPage {
        header,
        list,
        model,
        column: 1,
        edit: None,
        error: None,
    })
}

impl CellEditPage {
    /// Starts editing the current row's column; returns whether an editor
    /// opened.
    pub fn begin(&mut self, env: &Env<'_>) -> bool {
        let Some(key) = self.list.view.current() else {
            return false;
        };
        self.edit = CellEdit::begin(
            &self.model,
            &DefaultEditorFactory,
            key,
            self.column,
            env.locale.clone(),
        );
        self.error = None;
        self.edit.is_some()
    }

    fn commit(&mut self) -> Result<(), String> {
        let Some(edit) = &self.edit else {
            return Ok(());
        };
        match edit.commit(&mut self.model) {
            Ok(()) => {
                self.edit = None;
                self.error = None;
                Ok(())
            }
            Err(message) => {
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    fn cell_rect(&self, key: ItemKey, column: usize) -> Option<Rect> {
        let item = self
            .list
            .view
            .realized()
            .iter()
            .find(|item| item.key == key)?;
        let widths = self.header.view.widths();
        let x: f64 = widths[..column].iter().sum();
        Some(Rect::new(
            Point::new(item.bounds.origin.x + x, item.bounds.origin.y),
            Size::new(widths[column], item.bounds.size.height),
        ))
    }
}

impl Page for CellEditPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let width = (table_width(&self.header.view) + 40.0).min(bounds.size.width);
        let mut column = Column::new(bounds, env);
        column.gap = 0.0;
        self.header.arrange(tree, column.cells(28.0, &[width])[0]);
        let mut rest = column.rest();
        rest.size.height = (rest.size.height - 40.0).max(0.0);
        let rect = split(rest, &[width], 0.0, env.rtl())[0];
        self.list.arrange(tree, rect);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.list
            .paint(tree, list, &self.model, env, &self.header.view.widths());
        self.header.paint(tree, list, env);
        if let Some(key) = self.list.view.current()
            && let Some(rect) = self.cell_rect(key, self.column)
            && tree.focus() == Some(self.list.node)
        {
            let _ = list.stroke_rounded_rect(
                rect.inset(1.0),
                2.0,
                env.tokens.color(ColorRole::Primary),
                1.5,
            );
        }
        let below = Rect::new(
            Point::new(self.list.rect.origin.x, self.list.rect.max_y() + 8.0),
            Size::new(self.list.rect.size.width, 24.0),
        );
        if let Some(edit) = &self.edit {
            if let Some(rect) = self.cell_rect(edit.key, edit.column) {
                let validation = match &self.error {
                    Some(message) => ValidationState::Invalid(message.clone()),
                    None => edit.validation(),
                };
                paint_field(
                    env,
                    list,
                    rect,
                    &edit.text,
                    "",
                    true,
                    &validation,
                    Some(below),
                );
            }
        } else {
            env.caption(
                list,
                below,
                &format!(
                    "Editing column “{}” · Return edits, Left/Right change column",
                    self.header.view.columns()[self.column].title
                ),
            );
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = self.header.semantics();
        nodes.extend(self.list.semantics(&self.model));
        if let Some(edit) = &self.edit
            && let Some((_, semantic)) = nodes.iter_mut().find(|(id, _)| *id == self.list.node)
        {
            semantic.value = Some(format!(
                "Editing {}: {}",
                self.header.view.columns()[edit.column].title,
                edit.text
            ));
            semantic.state.invalid = edit.validation() != ValidationState::Valid;
        }
        nodes
    }

    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        vec![self.list.collection(&self.model)]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.header.pointer(tree, event).is_some() {
            return true;
        }
        if let PointerEvent::Down { position, .. } = event
            && self.list.rect.contains(position)
        {
            let x = position.x - self.list.rect.origin.x;
            let mut edge = 0.0;
            for (index, width) in self.header.view.widths().iter().enumerate() {
                edge += width;
                if x < edge {
                    self.column = index;
                    break;
                }
            }
        }
        let Some(response) = self.list.pointer(tree, &self.model, event, modifiers, 1) else {
            return false;
        };
        if response.activated.is_some() {
            self.begin(env);
        } else if response.current_changed {
            self.edit = None;
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        if key == Key::Tab {
            return false;
        }
        if let Some(edit) = &mut self.edit {
            match key {
                Key::Enter => {
                    let _ = self.commit();
                }
                Key::Escape => {
                    self.edit = None;
                    self.error = None;
                }
                Key::Backspace | Key::Delete => {
                    let mut text = edit.text.clone();
                    text.pop();
                    edit.set_text(text);
                    self.error = None;
                }
                _ => {}
            }
            return true;
        }
        if self.header.key(tree, key).is_some() {
            return true;
        }
        if tree.focus() != Some(self.list.node) {
            return false;
        }
        let forward = !env.rtl();
        match key {
            Key::ArrowRight | Key::ArrowLeft => {
                let next = (key == Key::ArrowRight) == forward;
                self.column = if next {
                    (self.column + 1).min(3)
                } else {
                    self.column.saturating_sub(1)
                };
                true
            }
            Key::Enter => {
                self.begin(env);
                true
            }
            _ => self.list.key(tree, &self.model, key, modifiers).is_some(),
        }
    }

    fn text(&mut self, _tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let Some(edit) = &mut self.edit else {
            return false;
        };
        let mut value = edit.text.clone();
        let changed = type_into(&mut value, text);
        edit.set_text(value);
        self.error = None;
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        self.list.action(tree, &self.model, node, action)
    }
}

// ---------------------------------------------------------------------------
// Data mapper

const MAPPED: [(usize, &str); 3] = [(0, "Name"), (1, "Quantity"), (2, "Price")];

/// A product list with a form bound to the selected row. Edits stay
/// pending per row until Submit writes the valid ones.
pub struct DataMapperPage {
    list: ListBox,
    model: TreeStore<Product>,
    mapper: DataMapper,
    fields: Vec<(NodeId, NodeId)>,
    submit: Push,
    errors: Vec<(usize, String)>,
}

pub fn data_mapper(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut list = ListBox::new(tree, parent, "Products", SemanticRole::List);
    let model = products(20);
    list.view.sync(&model);
    let mut mapper = DataMapper::new(MAPPED.iter().map(|(column, _)| *column).collect());
    mapper.set_current(model.key(None, 0));
    let fields = MAPPED
        .iter()
        .map(|_| (node(tree, parent, false), node(tree, parent, true)))
        .collect();
    let submit = Push::new(tree, parent, "Submit", env);
    Box::new(DataMapperPage {
        list,
        model,
        mapper,
        fields,
        submit,
        errors: Vec::new(),
    })
}

fn parse_mapped<'a>(env: &'a Env<'a>) -> impl Fn(usize, &str) -> Result<Value, String> + 'a {
    move |column, text| match column {
        1 => env
            .locale
            .parse_integer(text.trim())
            .map(Value::Int)
            .map_err(|_| "Enter a whole number".into()),
        2 => env
            .locale
            .parse_decimal(text.trim())
            .map(Value::Float)
            .map_err(|_| "Enter a price".into()),
        _ if text.trim().is_empty() => Err("A name is required".into()),
        _ => Ok(Value::Text(text.to_owned())),
    }
}

impl DataMapperPage {
    fn focused_field(&self, tree: &Tree) -> Option<usize> {
        self.fields
            .iter()
            .position(|(_, id)| tree.focus() == Some(*id))
    }

    fn value(&self, index: usize) -> String {
        let column = MAPPED[index].0;
        self.mapper
            .values(&self.model)
            .into_iter()
            .find(|(c, _)| *c == column)
            .map(|(_, text)| text)
            .unwrap_or_default()
    }

    /// Replaces one field's pending text, for scripted checks.
    pub fn edit(&mut self, index: usize, text: &str) {
        self.mapper.edit(MAPPED[index].0, text);
    }

    /// Writes pending fields; returns the invalid ones.
    pub fn submit(&mut self, env: &Env<'_>) -> Vec<(usize, String)> {
        self.errors = match self.mapper.submit(&mut self.model, parse_mapped(env)) {
            Ok(()) => Vec::new(),
            Err(errors) => errors,
        };
        self.errors.clone()
    }

    fn follow_selection(&mut self) {
        if self.mapper.set_current(self.list.view.current()) {
            self.errors.clear();
        }
    }
}

impl Page for DataMapperPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let halves = split(bounds, &[240.0, 0.0], 24.0, env.rtl());
        self.list.arrange(tree, halves[0]);
        let mut column = Column::new(halves[1], env);
        for (caption, id) in &self.fields {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(*id, column.cells(32.0, &[260.0])[0]);
            column.space(18.0);
        }
        let _ = tree.set_bounds(self.submit.node, column.cells(30.0, &[110.0])[0]);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.list.paint(tree, list, &self.model, env, &[]);
        for (index, ((caption, id), (column, label))) in self.fields.iter().zip(MAPPED).enumerate()
        {
            let pending = self
                .mapper
                .current()
                .is_some_and(|key| self.mapper.has_pending(key));
            let mut title = label.to_owned();
            if pending {
                title.push_str(" · unsaved changes");
            }
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), &title);
            let rect = tree.bounds(*id).unwrap_or_default();
            let validation = match self.errors.iter().find(|(c, _)| *c == column) {
                Some((_, message)) => ValidationState::Invalid(message.clone()),
                None => ValidationState::Valid,
            };
            let message = Rect::new(
                Point::new(rect.origin.x, rect.max_y() + 2.0),
                Size::new(rect.size.width + 200.0, 16.0),
            );
            paint_field(
                env,
                list,
                rect,
                &self.value(index),
                "",
                tree.focus() == Some(*id),
                &validation,
                matches!(validation, ValidationState::Invalid(_)).then_some(message),
            );
        }
        self.submit.paint(tree, list, env);
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = self.list.semantics(&self.model);
        for (index, ((_, id), (column, label))) in self.fields.iter().zip(MAPPED).enumerate() {
            let validation = match self.errors.iter().find(|(c, _)| *c == column) {
                Some((_, message)) => ValidationState::Invalid(message.clone()),
                None => ValidationState::Valid,
            };
            nodes.push((
                *id,
                crate::page::field_semantics(label, &self.value(index), &validation),
            ));
        }
        nodes.push(self.submit.semantics(tree));
        nodes
    }

    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        vec![self.list.collection(&self.model)]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.submit.pointer(tree, event) {
            self.submit(env);
            return true;
        }
        if let PointerEvent::Down { position, .. } = event
            && let Some((_, id)) = self
                .fields
                .iter()
                .find(|(_, id)| tree.bounds(*id).is_some_and(|b| b.contains(position)))
        {
            return tree.set_focus(Some(*id)).is_ok();
        }
        let handled = self
            .list
            .pointer(tree, &self.model, event, modifiers, 1)
            .is_some();
        self.follow_selection();
        handled
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.submit.key(tree, event) {
            self.submit(env);
            return true;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        if key == Key::Tab {
            return false;
        }
        if let Some(index) = self.focused_field(tree) {
            return match key {
                Key::Backspace | Key::Delete => {
                    let mut text = self.value(index);
                    text.pop();
                    self.edit(index, &text);
                    true
                }
                Key::Enter => {
                    self.submit(env);
                    true
                }
                _ => false,
            };
        }
        let handled = self.list.key(tree, &self.model, key, modifiers).is_some();
        self.follow_selection();
        handled
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let Some(index) = self.focused_field(tree) else {
            return false;
        };
        let mut value = self.value(index);
        let changed = type_into(&mut value, text);
        self.edit(index, &value);
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        if self.submit.action(tree, node, action) {
            self.submit(env);
            return true;
        }
        if let Some(index) = self.fields.iter().position(|(_, id)| *id == node) {
            return match action {
                SemanticAction::SetValue => {
                    self.edit(index, value.unwrap_or_default());
                    true
                }
                SemanticAction::Focus => tree.set_focus(Some(node)).is_ok(),
                _ => false,
            };
        }
        let handled = self.list.action(tree, &self.model, node, action);
        self.follow_selection();
        handled
    }
}

// ---------------------------------------------------------------------------
// Column browser

/// A country and its cities.
type Country = (&'static str, &'static [&'static str]);

const PLACES: &[(&str, &[Country])] = &[
    (
        "Asia",
        &[
            (
                "Việt Nam",
                &["Hà Nội", "Huế", "Đà Nẵng", "Thành phố Hồ Chí Minh"],
            ),
            ("Japan", &["Tokyo", "Kyoto", "Osaka"]),
            ("Israel", &["ירושלים", "תל אביב", "חיפה"]),
        ],
    ),
    (
        "Europe",
        &[
            ("France", &["Paris", "Lyon"]),
            ("Deutschland", &["Berlin", "München", "Köln"]),
        ],
    ),
    (
        "Americas",
        &[
            ("Canada", &["Montréal", "Toronto"]),
            ("Brasil", &["São Paulo", "Rio de Janeiro"]),
        ],
    ),
];

const BROWSER_COLUMN: f64 = 190.0;

/// Places browsed column by column: Right opens, Left goes back.
pub struct ColumnBrowserPage {
    container: NodeId,
    model: TreeStore<String>,
    browser: ColumnBrowser,
    rows: Vec<(ItemKey, NodeId)>,
}

pub fn column_browser(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let container = node(tree, parent, true);
    let mut model = TreeStore::<String>::new(1);
    let mut rows = Vec::new();
    for (continent, countries) in PLACES {
        let continent_key = model
            .push(None, (*continent).to_owned())
            .expect("valid parent");
        rows.push((continent_key, node(tree, container, false)));
        for (country, cities) in *countries {
            let country_key = model
                .push(Some(continent_key), (*country).to_owned())
                .expect("valid parent");
            rows.push((country_key, node(tree, container, false)));
            for city in *cities {
                let city_key = model
                    .push(Some(country_key), (*city).to_owned())
                    .expect("valid parent");
                rows.push((city_key, node(tree, container, false)));
            }
        }
    }
    let mut browser = ColumnBrowser::new();
    if let Some(first) = model.key(None, 0) {
        browser.select(0, first);
    }
    Box::new(ColumnBrowserPage {
        container,
        model,
        browser,
        rows,
    })
}

impl ColumnBrowserPage {
    /// Where each visible row goes: (key, column, row rect).
    fn placed(&self, tree: &Tree, rtl: bool) -> Vec<(ItemKey, usize, Rect)> {
        let bounds = tree.bounds(self.container).unwrap_or_default();
        let columns = self.browser.columns(&self.model);
        let widths = vec![BROWSER_COLUMN; columns.len()];
        let cells = split(bounds, &widths, 1.0, rtl);
        let mut placed = Vec::new();
        for (index, (parent, cell)) in columns.iter().zip(cells).enumerate() {
            for row in 0..self.model.row_count(*parent) {
                if let Some(key) = self.model.key(*parent, row) {
                    placed.push((
                        key,
                        index,
                        Rect::new(
                            Point::new(cell.origin.x, cell.origin.y + row as f64 * ROW),
                            Size::new(cell.size.width, ROW),
                        ),
                    ));
                }
            }
        }
        placed
    }

    fn refresh(&self, tree: &mut Tree, rtl: bool) {
        let placed = self.placed(tree, rtl);
        for (key, id) in &self.rows {
            match placed.iter().find(|(k, _, _)| k == key) {
                Some((_, _, rect)) => {
                    let _ = tree.set_visible(*id, true);
                    let _ = tree.set_bounds(*id, *rect);
                }
                None => {
                    let _ = tree.set_visible(*id, false);
                }
            }
        }
    }

    fn name(&self, key: ItemKey) -> String {
        self.model.data(key, 0, Role::Display).to_display_string()
    }

    /// The selected path, for scripted checks.
    pub fn path(&self) -> Vec<String> {
        self.browser
            .path()
            .iter()
            .map(|key| self.name(*key))
            .collect()
    }
}

impl Page for ColumnBrowserPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let rect = Rect::new(
            bounds.origin,
            Size::new(bounds.size.width, (ROW * 8.0).min(bounds.size.height)),
        );
        let _ = tree.set_bounds(self.container, rect);
        self.refresh(tree, env.rtl());
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let bounds = tree.bounds(self.container).unwrap_or_default();
        let focused = tree.focus() == Some(self.container);
        let columns = self.browser.columns(&self.model).len();
        for cell in split(bounds, &vec![BROWSER_COLUMN; columns], 1.0, env.rtl()) {
            env.panel(list, cell);
        }
        for (key, column, rect) in self.placed(tree, env.rtl()) {
            let selected = self.browser.path().get(column) == Some(&key);
            let mut color = env.tokens.color(ColorRole::Foreground);
            if selected {
                let active = focused && column == self.browser.focused_column();
                let fill = if active {
                    env.tokens.color(ColorRole::Primary)
                } else {
                    env.mix(0.2)
                };
                list.fill_rect(rect, fill);
                if active {
                    color = env.on_accent();
                }
            }
            let mut label = self.name(key);
            if self.model.has_children(key) {
                label.push_str(if env.rtl() { "  ‹" } else { "  ›" });
            }
            env.text(
                list,
                rect.inset(6.0),
                &label,
                env.body(),
                color,
                Align::Leading,
            );
        }
        if focused {
            env.focus_ring(list, bounds);
        }
        let below = Rect::new(
            Point::new(bounds.origin.x, bounds.max_y() + 8.0),
            Size::new(bounds.size.width, 20.0),
        );
        env.caption(list, below, &self.path().join(" › "));
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut container = sem(SemanticRole::Group, "Places", Some(self.path().join(", ")));
        container.actions = vec![SemanticAction::Focus];
        let mut nodes = vec![(self.container, container)];
        for (key, id) in &self.rows {
            let mut row = sem(SemanticRole::ListItem, self.name(*key), None);
            row.state.selected = Some(self.browser.path().contains(key));
            row.state.hidden = tree.visible(*id) == Some(false);
            row.actions = vec![SemanticAction::Activate];
            nodes.push((*id, row));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let Some((key, column, _)) = self
            .placed(tree, env.rtl())
            .into_iter()
            .find(|(_, _, rect)| rect.contains(position))
        else {
            return false;
        };
        let _ = tree.set_focus(Some(self.container));
        self.browser.select(column, key);
        self.refresh(tree, env.rtl());
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if tree.focus() != Some(self.container) {
            return false;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        let key = match (key, env.rtl()) {
            (Key::ArrowLeft, true) => Key::ArrowRight,
            (Key::ArrowRight, true) => Key::ArrowLeft,
            (key, _) => key,
        };
        let changed = self.browser.handle_key(&self.model, key);
        self.refresh(tree, env.rtl());
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        if action != SemanticAction::Activate {
            return false;
        }
        let Some((key, column, _)) =
            self.rows
                .iter()
                .find(|(_, id)| *id == node)
                .and_then(|(key, _)| {
                    self.placed(tree, env.rtl())
                        .into_iter()
                        .find(|(k, _, _)| k == key)
                })
        else {
            return false;
        };
        self.browser.select(column, key);
        self.refresh(tree, env.rtl());
        true
    }
}

// ---------------------------------------------------------------------------
// Undo history

const HISTORY_LIMIT: usize = 12;

/// A counter edited by buttons, with an undo history list: choosing a row
/// undoes or redoes to that state.
pub struct UndoHistoryPage {
    value: i64,
    stack: UndoStack<i64>,
    buttons: Vec<(Push, &'static str)>,
    undo: Push,
    redo: Push,
    rows: Vec<NodeId>,
    readout: NodeId,
}

pub fn undo_history_page(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let readout = node(tree, parent, false);
    let buttons = [
        ("Add 1", "add1"),
        ("Add 10", "add10"),
        ("Double", "double"),
        ("Reset", "reset"),
    ]
    .into_iter()
    .map(|(label, id)| (Push::new(tree, parent, label, env), id))
    .collect();
    let undo = Push::new(tree, parent, "Undo", env);
    let redo = Push::new(tree, parent, "Redo", env);
    let rows = (0..=HISTORY_LIMIT)
        .map(|_| node(tree, parent, false))
        .collect();
    Box::new(UndoHistoryPage {
        value: 0,
        stack: UndoStack::new(HISTORY_LIMIT),
        buttons,
        undo,
        redo,
        rows,
        readout,
    })
}

impl UndoHistoryPage {
    fn apply(&mut self, id: &str) {
        let (label, delta) = match id {
            "add1" => ("Add 1", 1),
            "add10" => ("Add 10", 10),
            "double" => ("Double", self.value),
            _ => ("Reset", -self.value),
        };
        self.value += delta;
        self.stack.push(label, delta);
    }

    fn undo(&mut self) -> bool {
        match self.stack.undo() {
            Some(group) => {
                self.value -= group.entries.iter().sum::<i64>();
                true
            }
            None => false,
        }
    }

    fn redo(&mut self) -> bool {
        match self.stack.redo() {
            Some(group) => {
                self.value += group.entries.iter().sum::<i64>();
                true
            }
            None => false,
        }
    }

    /// Undoes or redoes until history row `row` is current.
    pub fn go_to(&mut self, row: usize) -> bool {
        let steps = undo_steps_to(&self.stack, row);
        for _ in 0..steps.unsigned_abs() {
            if steps < 0 {
                self.undo();
            } else {
                self.redo();
            }
        }
        steps != 0
    }

    fn sync_rows(&self, tree: &mut Tree) {
        let count = undo_history(&self.stack, "").len();
        for (index, id) in self.rows.iter().enumerate() {
            let _ = tree.set_visible(*id, index < count);
        }
    }

    fn press(&mut self, index: Option<usize>, undo: bool, redo: bool, tree: &mut Tree) -> bool {
        let changed = match index {
            Some(index) => {
                let id = self.buttons[index].1;
                self.apply(id);
                true
            }
            None if undo => self.undo(),
            None if redo => self.redo(),
            None => false,
        };
        self.sync_rows(tree);
        changed
    }
}

impl Page for UndoHistoryPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.readout, column.row(28.0));
        let cells = column.cells(30.0, &[80.0, 80.0, 80.0, 80.0, 0.0]);
        for ((push, _), cell) in self.buttons.iter().zip(&cells) {
            let _ = tree.set_bounds(push.node, *cell);
        }
        let cells = column.cells(30.0, &[80.0, 80.0]);
        let _ = tree.set_bounds(self.undo.node, cells[0]);
        let _ = tree.set_bounds(self.redo.node, cells[1]);
        column.gap = 0.0;
        for id in &self.rows {
            let _ = tree.set_bounds(*id, column.cells(ROW, &[260.0])[0]);
        }
        self.sync_rows(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        env.label(
            list,
            tree.bounds(self.readout).unwrap_or_default(),
            &format!("Value: {}", self.value),
        );
        for (push, _) in &self.buttons {
            push.paint(tree, list, env);
        }
        self.undo.paint(tree, list, env);
        self.redo.paint(tree, list, env);
        let history = undo_history(&self.stack, "Start (0)");
        for (row, id) in history.iter().zip(&self.rows) {
            let rect = tree.bounds(*id).unwrap_or_default();
            if row.current {
                list.fill_rect(rect, env.mix(0.18));
            }
            let color = if row.redoable {
                env.tokens.color(ColorRole::Foreground).with_alpha(0.45)
            } else {
                env.tokens.color(ColorRole::Foreground)
            };
            let label = if row.current {
                format!("● {}", row.label)
            } else {
                format!("   {}", row.label)
            };
            env.text(
                list,
                rect.inset(4.0),
                &label,
                env.body(),
                color,
                Align::Leading,
            );
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![(
            self.readout,
            sem(
                SemanticRole::StaticText,
                format!("Value: {}", self.value),
                None,
            ),
        )];
        nodes.extend(self.buttons.iter().map(|(push, _)| push.semantics(tree)));
        nodes.push(self.undo.semantics(tree));
        nodes.push(self.redo.semantics(tree));
        let history = undo_history(&self.stack, "Start (0)");
        for (index, id) in self.rows.iter().enumerate() {
            let mut row = match history.get(index) {
                Some(row) => {
                    let mut node = sem(SemanticRole::ListItem, row.label.clone(), None);
                    node.state.selected = Some(row.current);
                    node.description = if row.redoable {
                        "Redoable".into()
                    } else {
                        String::new()
                    };
                    node
                }
                None => sem(SemanticRole::ListItem, "Unused history slot", None),
            };
            row.state.hidden = index >= history.len();
            row.actions = vec![SemanticAction::Activate];
            nodes.push((*id, row));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let index = self
            .buttons
            .iter_mut()
            .position(|(push, _)| push.pointer(tree, event));
        let undo = self.undo.pointer(tree, event);
        let redo = self.redo.pointer(tree, event);
        if index.is_some() || undo || redo {
            return self.press(index, undo, redo, tree);
        }
        if let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
            && let Some(row) = self.rows.iter().position(|id| {
                tree.visible(*id) == Some(true)
                    && tree.bounds(*id).is_some_and(|b| b.contains(position))
            })
        {
            let changed = self.go_to(row);
            self.sync_rows(tree);
            return changed;
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if modifiers.command && pressed(event) == Some(Key::Character('z')) {
            let (undo, redo) = (!modifiers.shift, modifiers.shift);
            return self.press(None, undo, redo, tree);
        }
        let index = self
            .buttons
            .iter_mut()
            .position(|(push, _)| push.key(tree, event));
        let undo = self.undo.key(tree, event);
        let redo = self.redo.key(tree, event);
        (index.is_some() || undo || redo) && self.press(index, undo, redo, tree)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let index = self
            .buttons
            .iter_mut()
            .position(|(push, _)| push.action(tree, node, action));
        let undo = self.undo.action(tree, node, action);
        let redo = self.redo.action(tree, node, action);
        if index.is_some() || undo || redo {
            return self.press(index, undo, redo, tree);
        }
        match self.rows.iter().position(|id| *id == node) {
            Some(row) if action == SemanticAction::Activate => {
                let changed = self.go_to(row);
                self.sync_rows(tree);
                changed
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Canvas

pub const CANVAS_ITEMS: usize = 20_000;
const CANVAS_COLUMNS: usize = 200;
const CANVAS_SPACING: f64 = 40.0;
/// Most items given semantic nodes at once; the rest are virtualized.
const CANVAS_REALIZED: usize = 400;

fn canvas_extent() -> Rect {
    Rect::new(
        Point::new(0.0, 0.0),
        Size::new(
            CANVAS_COLUMNS as f64 * CANVAS_SPACING,
            (CANVAS_ITEMS / CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
        ),
    )
}

/// 20,000 shapes with zoom, hit testing, rubber-band selection, culling,
/// and virtualized semantics. The zoom level persists.
pub struct CanvasPage {
    canvas_node: NodeId,
    zoom_in: Push,
    zoom_out: Push,
    fit: Push,
    readout: NodeId,
    scene: RefCell<Scene>,
    canvas: CanvasView,
    realized: Vec<(NodeId, SceneItemId, usize)>,
    painted: Cell<usize>,
    dragging: bool,
    restored: Option<f64>,
}

pub fn canvas(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let zoom_in = Push::new(tree, parent, "Zoom in", env);
    let zoom_out = Push::new(tree, parent, "Zoom out", env);
    let fit = Push::new(tree, parent, "Fit", env);
    let readout = node(tree, parent, false);
    let canvas_node = node(tree, parent, true);
    let mut scene = Scene::new();
    let palette = [
        Color::rgba8(10, 132, 255, 255),
        Color::rgba8(48, 209, 88, 255),
        Color::rgba8(255, 159, 10, 255),
        Color::rgba8(191, 90, 242, 255),
    ];
    for i in 0..CANVAS_ITEMS {
        let origin = Point::new(
            (i % CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
            (i / CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
        );
        let rect = Rect::new(origin, Size::new(28.0, 28.0));
        let shape = if i % 3 == 0 {
            ItemShape::Ellipse(rect)
        } else {
            ItemShape::Rect(rect)
        };
        let _ = scene.add(
            SceneItem::new(shape, ItemStyle::filled(palette[i % palette.len()]))
                .named(format!("Node {i}"), SemanticRole::Button),
        );
    }
    let canvas = CanvasView::new("Canvas", Rect::default());
    Box::new(CanvasPage {
        canvas_node,
        zoom_in,
        zoom_out,
        fit,
        readout,
        scene: RefCell::new(scene),
        canvas,
        realized: Vec::new(),
        painted: Cell::new(0),
        dragging: false,
        restored: Some(env.state.canvas_zoom),
    })
}

impl CanvasPage {
    #[cfg(target_os = "macos")]
    pub fn zoom(&self) -> f64 {
        self.canvas.zoom()
    }

    /// Zooms about the viewport center.
    pub fn set_zoom(&mut self, tree: &mut Tree, zoom: f64) {
        let center = self.canvas.viewport().center();
        self.canvas.zoom_at(center, zoom);
        self.realize(tree);
    }

    /// The name of the item under a view point.
    #[cfg(target_os = "macos")]
    pub fn hit(&self, at: Point) -> Option<String> {
        let mut scene = self.scene.borrow_mut();
        let id = scene.hit_test(self.canvas.view_to_scene(at))?;
        scene.item(id).map(|item| item.name.clone())
    }

    /// The view position of the center of item `index`.
    #[cfg(target_os = "macos")]
    pub fn item_center(&self, index: usize) -> Point {
        let scene_point = Point::new(
            (index % CANVAS_COLUMNS) as f64 * CANVAS_SPACING + 14.0,
            (index / CANVAS_COLUMNS) as f64 * CANVAS_SPACING + 14.0,
        );
        self.canvas.scene_to_view(scene_point)
    }

    /// Drags a rubber band between two view points; returns how many
    /// items are selected afterwards.
    #[cfg(target_os = "macos")]
    pub fn rubber_band(&mut self, tree: &mut Tree, from: Point, to: Point) -> usize {
        let scene = self.scene.get_mut();
        self.canvas.handle(
            scene,
            CanvasInput::Press {
                at: from,
                extend: false,
            },
        );
        self.canvas.handle(scene, CanvasInput::Drag { at: to });
        self.canvas.handle(scene, CanvasInput::Release { at: to });
        self.realize(tree);
        self.canvas.selection().len()
    }

    /// Items painted by the last frame (after culling).
    #[cfg(target_os = "macos")]
    pub fn painted(&self) -> usize {
        self.painted.get()
    }

    /// Items that currently have semantic nodes.
    #[cfg(target_os = "macos")]
    pub fn realized(&self) -> usize {
        self.realized.len()
    }

    /// Gives the visible items (up to a bound) tree nodes so assistive
    /// technologies can reach them.
    fn realize(&mut self, tree: &mut Tree) {
        for (id, _, _) in self.realized.drain(..) {
            let _ = tree.detach(id);
        }
        let items = self.canvas.item_semantics(self.scene.get_mut());
        for (item, index, _, bounds) in items.into_iter().take(CANVAS_REALIZED) {
            if let Ok(id) = tree.attach(self.canvas_node, bounds) {
                self.realized.push((id, item, index));
            }
        }
    }

    fn handle(&mut self, tree: &mut Tree, input: CanvasInput) -> bool {
        let response = self.canvas.handle(self.scene.get_mut(), input);
        if !response.delete_requested.is_empty() {
            let scene = self.scene.get_mut();
            for id in &response.delete_requested {
                scene.remove(*id);
            }
            self.canvas.prune(scene);
        }
        self.realize(tree);
        response.repaint || !response.delete_requested.is_empty()
    }

    fn run(&mut self, tree: &mut Tree, button: usize) -> bool {
        match button {
            0 => self.set_zoom(tree, self.canvas.zoom() * 1.25),
            1 => self.set_zoom(tree, self.canvas.zoom() / 1.25),
            _ => {
                self.canvas.fit(canvas_extent(), 8.0);
                self.realize(tree);
            }
        }
        true
    }
}

impl Page for CanvasPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[90.0, 90.0, 70.0, 0.0]);
        let _ = tree.set_bounds(self.zoom_in.node, cells[0]);
        let _ = tree.set_bounds(self.zoom_out.node, cells[1]);
        let _ = tree.set_bounds(self.fit.node, cells[2]);
        let _ = tree.set_bounds(self.readout, cells[3]);
        let rect = column.rest();
        let _ = tree.set_bounds(self.canvas_node, rect);
        self.canvas.set_viewport(rect);
        if let Some(zoom) = self.restored.take()
            && zoom != self.canvas.zoom()
        {
            self.canvas.zoom_at(rect.origin, zoom);
        }
        self.realize(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.zoom_in.paint(tree, list, env);
        self.zoom_out.paint(tree, list, env);
        self.fit.paint(tree, list, env);
        let rect = tree.bounds(self.canvas_node).unwrap_or_default();
        env.panel(list, rect);
        let painted = self
            .canvas
            .paint(&mut env.cx(list, rect), &mut self.scene.borrow_mut())
            .unwrap_or(0);
        self.painted.set(painted);
        if tree.focus() == Some(self.canvas_node) {
            env.focus_ring(list, rect);
        }
        env.caption(
            list,
            tree.bounds(self.readout).unwrap_or_default(),
            &format!(
                "Zoom {}% · {} selected · {painted} of {} painted",
                (self.canvas.zoom() * 100.0).round(),
                self.canvas.selection().len(),
                self.scene.borrow().len(),
            ),
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut scene = self.scene.borrow_mut();
        let (container, _) = self.canvas.semantics(&mut scene);
        let items = self.canvas.item_semantics(&mut scene);
        let mut nodes = vec![
            self.zoom_in.semantics(tree),
            self.zoom_out.semantics(tree),
            self.fit.semantics(tree),
            (self.canvas_node, container),
        ];
        for (id, item, _) in &self.realized {
            if let Some((_, _, node, _)) = items.iter().find(|(other, _, _, _)| other == item) {
                nodes.push((*id, node.clone()));
            }
        }
        nodes
    }

    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        let (_, info) = self.canvas.semantics(&mut self.scene.borrow_mut());
        vec![Collection {
            node: self.canvas_node,
            info,
            items: self
                .realized
                .iter()
                .map(|(id, _, index)| (*id, *index))
                .collect(),
        }]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let hit = [&mut self.zoom_in, &mut self.zoom_out, &mut self.fit]
            .into_iter()
            .position(|push| push.pointer(tree, event));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        let rect = tree.bounds(self.canvas_node).unwrap_or_default();
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if rect.contains(position) => {
                let _ = tree.set_focus(Some(self.canvas_node));
                let _ = tree.set_pointer_capture(Some(self.canvas_node));
                self.dragging = true;
                self.handle(
                    tree,
                    CanvasInput::Press {
                        at: position,
                        extend: modifiers.shift || modifiers.command,
                    },
                )
            }
            PointerEvent::Move { position } if self.dragging => {
                self.handle(tree, CanvasInput::Drag { at: position })
            }
            PointerEvent::Up { position, .. } if self.dragging => {
                self.dragging = false;
                let _ = tree.set_pointer_capture(None);
                self.handle(tree, CanvasInput::Release { at: position })
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if rect.contains(position) => {
                let delta = wheel_points(delta, unit);
                self.handle(
                    tree,
                    CanvasInput::Wheel {
                        dx: delta.x,
                        dy: delta.y,
                        at: position,
                        zoom: modifiers.command,
                    },
                )
            }
            _ => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let hit = [&mut self.zoom_in, &mut self.zoom_out, &mut self.fit]
            .into_iter()
            .position(|push| push.key(tree, event));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        if tree.focus() != Some(self.canvas_node) {
            return false;
        }
        match pressed(event) {
            Some(Key::Tab) | None => false,
            Some(key) => {
                self.handle(
                    tree,
                    CanvasInput::Key {
                        key,
                        shift: modifiers.shift,
                    },
                );
                true
            }
        }
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let hit = [&mut self.zoom_in, &mut self.zoom_out, &mut self.fit]
            .into_iter()
            .position(|push| push.action(tree, node, action));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        if node == self.canvas_node {
            return action == SemanticAction::Focus && tree.set_focus(Some(node)).is_ok();
        }
        let Some((_, item, _)) = self.realized.iter().find(|(id, _, _)| *id == node).copied()
        else {
            return false;
        };
        let performed = self.canvas.perform(self.scene.get_mut(), item, action);
        self.realize(tree);
        performed
    }

    fn persist(&self, state: &mut PageState) {
        state.canvas_zoom = self.canvas.zoom();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    #[test]
    fn cell_edit_rejects_text_in_a_number_column_and_commits_a_number() {
        let mut h = Harness::new("cell-edit");
        let table = h.node("", Some(SemanticRole::Table));
        let _ = h.tree.set_focus(Some(table));
        h.press(Key::ArrowDown);
        let env = crate::pages::tests::env(&h.tokens, &h.locale, true, &h.state, &h.cache);
        let page = h.page.as_any_mut().downcast_mut::<CellEditPage>().unwrap();
        assert!(page.begin(&env), "a current row opens the quantity editor");
        page.edit.as_mut().unwrap().set_text("many");
        assert!(page.commit().is_err());
        page.edit.as_mut().unwrap().set_text("12");
        assert_eq!(page.commit(), Ok(()));
    }

    #[test]
    fn undo_history_jumps_back_and_forward_through_the_stack() {
        let mut h = Harness::new("undo-history");
        let add_ten = h.node("Add 10", None);
        h.act(add_ten, SemanticAction::Activate);
        h.act(add_ten, SemanticAction::Activate);
        let page = h.get::<UndoHistoryPage>();
        assert_eq!(page.value, 20);
        assert!(page.go_to(0), "row 0 is the empty history");
        assert_eq!(page.value, 0);
        assert!(!page.go_to(0), "already there");
        assert!(page.go_to(2));
        assert_eq!(page.value, 20);
    }
}
