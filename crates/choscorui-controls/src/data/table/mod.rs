//! A data table over an item model: a filter, column visibility, sorting,
//! row selection, and pagination.

mod layout;

use crate::CommandId;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::input::Editor;
use crate::hosted;
use crate::menu::{MenuCheck, MenuItem, MenuResponse};
use crate::overlay::menu_popup::MenuPopup;
use crate::style::PaintContext;
use crate::{CommandState, TextField};
use choscorui_core::{
    EventPhase, EventReply, ItemKey, ItemModel, Key, KeyEvent, Measured, Point, PointerButton,
    PointerEvent, Rect, Role, Selection, SelectionCommand, SelectionMode, SemanticAction,
    SemanticNode, SemanticRole, SortFilterProxy, SortOrder, TextFilter,
};
use layout::Geometry;

/// A column of the grid: the selection checkboxes or a model column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Column {
    Check,
    Data(usize),
}

/// A tab stop inside the table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Filter,
    Columns,
    Grid,
    Previous,
    Next,
}

/// The focused grid cell: a row on the page, or `None` for the header,
/// and an index into the visible columns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Cell {
    row: Option<usize>,
    column: usize,
}

/// The part under the pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Part {
    Columns,
    Header(Column),
    Check(usize),
    Row(usize),
    Previous,
    Next,
}

/// Formats the selection status from the selected and the filtered row
/// counts.
pub type StatusText = Box<dyn Fn(usize, usize) -> String>;

/// A data table. It reads rows from an item model through a sort and
/// filter proxy, and keeps a keyed selection.
///
/// - A header click, or Enter or Space on a header cell, sorts by that
///   column: ascending, descending, then source order.
/// - The first column holds checkboxes. A row checkbox toggles the row,
///   and the header checkbox toggles every row on the page. Each change
///   emits `Changed(Indices(source rows))`.
/// - The grid is one tab stop. Arrow keys move between cells, and Home
///   and End go to the first and the last column.
/// - The optional filter input, the column menu, and the Previous and
///   Next buttons are tab stops of their own.
pub struct DataTable {
    name: String,
    model: Box<dyn ItemModel>,
    proxy: SortFilterProxy,
    /// The filtered rows in display order.
    order: Vec<ItemKey>,
    selection: Selection,
    hidden: Vec<bool>,
    filter: Option<(Editor, Option<usize>)>,
    columns_label: Option<String>,
    menu: MenuPopup,
    labels: [String; 2],
    empty: String,
    status: Option<StatusText>,
    page_size: usize,
    page: usize,
    focus: Stop,
    cell: Cell,
    pressed: Option<Part>,
    hovered: Option<Part>,
    geometry: Geometry,
    enabled: bool,
    rtl: bool,
}

impl DataTable {
    /// A table of `model`. `previous` and `next` label the page buttons.
    pub fn new(
        name: impl Into<String>,
        model: Box<dyn ItemModel>,
        previous: impl Into<String>,
        next: impl Into<String>,
    ) -> Self {
        let columns = model.column_count();
        let mut table = Self {
            name: name.into(),
            model,
            proxy: SortFilterProxy::new(),
            order: Vec::new(),
            selection: Selection::new(),
            hidden: vec![false; columns],
            filter: None,
            columns_label: None,
            menu: MenuPopup::new(Vec::new()),
            labels: [previous.into(), next.into()],
            empty: String::new(),
            status: None,
            page_size: 10,
            page: 0,
            focus: Stop::Grid,
            cell: Cell::default(),
            pressed: None,
            hovered: None,
            geometry: Geometry::default(),
            enabled: true,
            rtl: false,
        };
        table.sync();
        table
    }

    /// Adds a filter input with `placeholder` that matches `column`, or
    /// every column when `None`.
    #[must_use]
    pub fn with_filter(mut self, placeholder: impl Into<String>, column: Option<usize>) -> Self {
        let field = TextField::new(self.name.clone(), "").with_placeholder(placeholder);
        self.filter = Some((Editor::new(field), column));
        self
    }

    /// Adds a menu button with `label` that shows and hides columns.
    #[must_use]
    pub fn with_column_menu(mut self, label: impl Into<String>) -> Self {
        self.columns_label = Some(label.into());
        let items = (0..self.model.column_count())
            .map(|column| {
                MenuItem::action(self.header(column), CommandId(column as u64))
                    .with_check(MenuCheck::Checkbox)
            })
            .collect();
        self.menu = MenuPopup::new(items);
        for column in 0..self.hidden.len() {
            self.sync_menu(column);
        }
        self
    }

    /// Sets the text that shows when no row matches the filter.
    #[must_use]
    pub fn with_empty(mut self, text: impl Into<String>) -> Self {
        self.empty = text.into();
        self
    }

    /// Shows a status line under the table, such as "2 of 5 row(s)
    /// selected.", formatted from the selected and the filtered counts.
    #[must_use]
    pub fn with_status(mut self, status: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.status = Some(Box::new(status));
        self
    }

    /// Shows `rows` rows per page, at least one.
    #[must_use]
    pub fn with_page_size(mut self, rows: usize) -> Self {
        self.page_size = rows.max(1);
        self
    }

    /// Hides `column` until the column menu shows it again.
    #[must_use]
    pub fn with_hidden(mut self, column: usize) -> Self {
        if let Some(hidden) = self.hidden.get_mut(column) {
            *hidden = true;
            self.sync_menu(column);
        }
        self
    }

    /// The source rows of the selected items, in source order.
    pub fn selected_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = self
            .selection
            .keys()
            .filter_map(|key| self.model.row(key))
            .collect();
        rows.sort_unstable();
        rows
    }

    /// The sort column and order.
    pub fn sort(&self) -> Option<(usize, SortOrder)> {
        self.proxy.sort()
    }

    fn header(&self, column: usize) -> String {
        self.model.header(column, Role::Display).to_display_string()
    }

    fn text(&self, key: ItemKey, column: usize) -> String {
        self.model
            .data(key, column, Role::Display)
            .to_display_string()
    }

    fn sync_menu(&mut self, column: usize) {
        let state = CommandState {
            enabled: true,
            checked: !self.hidden[column],
        };
        let _ = self.menu.set_command(CommandId(column as u64), state);
    }

    /// Rebuilds the display order from the proxy.
    fn sync(&mut self) {
        self.proxy.sync(self.model.as_ref());
        let view = self.proxy.view(self.model.as_ref());
        self.order = (0..view.row_count(None))
            .filter_map(|row| view.key(None, row))
            .collect();
        self.page = self.page.min(self.last_page());
    }

    fn last_page(&self) -> usize {
        self.order.len().saturating_sub(1) / self.page_size
    }

    /// The rows on the current page.
    fn page_keys(&self) -> &[ItemKey] {
        let start = (self.page * self.page_size).min(self.order.len());
        let end = (start + self.page_size).min(self.order.len());
        &self.order[start..end]
    }

    fn columns(&self) -> Vec<Column> {
        std::iter::once(Column::Check)
            .chain(
                (0..self.hidden.len())
                    .filter(|column| !self.hidden[*column])
                    .map(Column::Data),
            )
            .collect()
    }

    fn stops(&self) -> Vec<Stop> {
        let mut stops = Vec::new();
        if self.filter.is_some() {
            stops.push(Stop::Filter);
        }
        if self.columns_label.is_some() {
            stops.push(Stop::Columns);
        }
        stops.push(Stop::Grid);
        if self.page > 0 {
            stops.push(Stop::Previous);
        }
        if self.page < self.last_page() {
            stops.push(Stop::Next);
        }
        stops
    }

    /// Whether every row on the page is selected, some are, or none.
    fn page_check(&self) -> Option<bool> {
        let keys = self.page_keys();
        let selected = keys
            .iter()
            .filter(|key| self.selection.is_selected(**key))
            .count();
        match selected {
            0 => Some(false),
            count if count == keys.len() => Some(true),
            _ => None,
        }
    }

    fn emit_selection(&self, cx: &mut ControlContext<'_>) {
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            self.selected_rows(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    fn toggle_row(&mut self, cx: &mut ControlContext<'_>, row: usize) {
        let Some(key) = self.page_keys().get(row).copied() else {
            return;
        };
        let order = self.order.clone();
        self.selection
            .apply(SelectionMode::Multi, SelectionCommand::Toggle, key, &order);
        self.emit_selection(cx);
    }

    fn toggle_page(&mut self, cx: &mut ControlContext<'_>) {
        let keys = self.page_keys().to_vec();
        if self.page_check() == Some(true) {
            for key in keys {
                let order = self.order.clone();
                self.selection
                    .apply(SelectionMode::Multi, SelectionCommand::Toggle, key, &order);
            }
        } else {
            self.selection.select_all(&keys);
        }
        self.emit_selection(cx);
    }

    /// Sorts by `column`: ascending, descending, then source order.
    fn cycle_sort(&mut self, cx: &mut ControlContext<'_>, column: usize) {
        let next = match self.proxy.sort() {
            Some((current, SortOrder::Ascending)) if current == column => {
                Some((column, SortOrder::Descending))
            }
            Some((current, SortOrder::Descending)) if current == column => None,
            _ => Some((column, SortOrder::Ascending)),
        };
        self.proxy.set_sort(next);
        self.sync();
        cx.request_paint();
        cx.request_semantics();
    }

    fn set_page(&mut self, cx: &mut ControlContext<'_>, page: usize) {
        let page = page.min(self.last_page());
        if page == self.page {
            return;
        }
        self.page = page;
        let rows = self.page_keys().len();
        if let Some(row) = self.cell.row
            && row >= rows
        {
            self.cell.row = rows.checked_sub(1);
        }
        // A button that no longer applies hands focus to the grid.
        if !self.stops().contains(&self.focus) {
            self.focus = Stop::Grid;
        }
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    fn activate_cell(&mut self, cx: &mut ControlContext<'_>) {
        let columns = self.columns();
        let column = columns.get(self.cell.column).copied();
        match (self.cell.row, column) {
            (None, Some(Column::Check)) => self.toggle_page(cx),
            (None, Some(Column::Data(column))) => self.cycle_sort(cx, column),
            (Some(row), Some(_)) => self.toggle_row(cx, row),
            _ => {}
        }
    }

    fn activate_stop(&mut self, cx: &mut ControlContext<'_>, keyboard: bool) {
        match self.focus {
            Stop::Filter => {}
            Stop::Columns => self.open_menu(cx, keyboard),
            Stop::Grid => self.activate_cell(cx),
            Stop::Previous => self.set_page(cx, self.page.saturating_sub(1)),
            Stop::Next => self.set_page(cx, self.page + 1),
        }
    }

    fn open_menu(&mut self, cx: &mut ControlContext<'_>, keyboard: bool) {
        let button = self.geometry.columns;
        let width = self.menu_width();
        let x = if self.rtl {
            button.origin.x + width
        } else {
            button.max_x() - width
        };
        self.menu
            .open(Point::new(x, button.max_y() + 4.0), cx.time(), keyboard);
        cx.emit(ControlMessage::OpenChanged(true));
        cx.request_paint();
        cx.request_semantics();
    }

    fn menu_width(&self) -> f64 {
        self.menu
            .panels()
            .first()
            .map_or(0.0, |panel| panel.size.width)
    }

    fn close_menu(&mut self, cx: &mut ControlContext<'_>) {
        if self.menu.is_open() {
            self.menu.close(cx.time());
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
            cx.request_semantics();
        }
    }

    fn menu_response(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) {
        match response {
            MenuResponse::None => {}
            MenuResponse::Activated(CommandId(column)) => {
                let column = column as usize;
                if let Some(hidden) = self.hidden.get_mut(column) {
                    *hidden = !*hidden;
                    self.sync_menu(column);
                    let count = self.columns().len();
                    self.cell.column = self.cell.column.min(count - 1);
                    cx.request_layout();
                }
                self.close_menu(cx);
            }
            MenuResponse::Closed => self.close_menu(cx),
        }
        cx.request_paint();
    }

    /// Routes `event` to the open column menu. `None` when the menu does
    /// not take it.
    fn menu_event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
    ) -> Option<EventReply> {
        if !self.menu.is_open() {
            return None;
        }
        match event {
            ControlEvent::Pointer(pointer) => {
                let released = matches!(pointer, PointerEvent::Up { .. });
                let response = self.menu.pointer(pointer.position(), released)?;
                if released {
                    self.menu_response(cx, response);
                } else {
                    cx.request_paint();
                }
                Some(EventReply::Stop)
            }
            ControlEvent::Key(KeyEvent::Down { key: Key::Tab, .. })
            | ControlEvent::Dismiss
            | ControlEvent::FocusChanged(false) => {
                self.close_menu(cx);
                None
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let response = self.menu.menu.handle_key(*key, &self.menu.commands);
                self.menu_response(cx, response);
                Some(EventReply::Stop)
            }
            _ => None,
        }
    }

    /// Routes `event` to the filter input while it has focus.
    fn filter_event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
    ) -> Option<EventReply> {
        let rect = self.geometry.filter_text;
        let field = self.geometry.filter;
        let focused = self.focus == Stop::Filter;
        let (editor, _) = self.filter.as_mut()?;
        let forward = match event {
            ControlEvent::Pointer(PointerEvent::Down { position, .. }) => field.contains(*position),
            ControlEvent::Pointer(_) | ControlEvent::Text(_) => focused,
            ControlEvent::Key(KeyEvent::Down { key, .. }) => focused && *key != Key::Tab,
            _ => false,
        };
        if !forward {
            return None;
        }
        let reply = editor.event(cx, event, rect);
        if matches!(event, ControlEvent::Pointer(PointerEvent::Down { .. })) {
            self.focus = Stop::Filter;
        }
        self.refilter(cx);
        (reply == EventReply::Stop).then_some(EventReply::Stop)
    }

    /// Applies the filter text when it changed.
    fn refilter(&mut self, cx: &mut ControlContext<'_>) {
        let Some((editor, column)) = &self.filter else {
            return;
        };
        let value = editor.field.value().to_string();
        let current = self
            .proxy
            .filter()
            .map_or("", |filter| filter.pattern.as_str());
        if value == current {
            return;
        }
        let filter = (!value.is_empty()).then(|| TextFilter {
            column: *column,
            ..TextFilter::contains(value)
        });
        self.proxy.set_filter(filter);
        self.page = 0;
        self.sync();
        self.cell.row = None;
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                let Some(part) = self.part_at(*position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(part);
                match part {
                    Part::Columns => self.focus = Stop::Columns,
                    Part::Previous => self.focus = Stop::Previous,
                    Part::Next => self.focus = Stop::Next,
                    Part::Header(column) => {
                        self.focus = Stop::Grid;
                        self.cell = Cell {
                            row: None,
                            column: self.column_index(column),
                        };
                    }
                    Part::Check(row) | Part::Row(row) => {
                        self.focus = Stop::Grid;
                        self.cell.row = Some(row);
                    }
                }
                cx.request_focus();
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Up { position, .. } if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = self.part_at(*position);
                if let Some(part) = self.pressed.take().filter(|part| hit == Some(*part)) {
                    match part {
                        Part::Columns => self.open_menu(cx, false),
                        Part::Header(Column::Check) => self.toggle_page(cx),
                        Part::Header(Column::Data(column)) => self.cycle_sort(cx, column),
                        Part::Check(row) => self.toggle_row(cx, row),
                        Part::Row(_) => {}
                        Part::Previous => self.set_page(cx, self.page.saturating_sub(1)),
                        Part::Next => self.set_page(cx, self.page + 1),
                    }
                }
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Move { position } => {
                let hovered = self.part_at(*position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn column_index(&self, column: Column) -> usize {
        self.columns()
            .iter()
            .position(|other| *other == column)
            .unwrap_or(0)
    }

    fn grid_key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> bool {
        let rows = self.page_keys().len();
        let columns = self.columns().len();
        let (forward, back) = if self.rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        let cell = self.cell;
        self.cell = match key {
            Key::ArrowDown => Cell {
                row: match cell.row {
                    None if rows > 0 => Some(0),
                    Some(row) if row + 1 < rows => Some(row + 1),
                    other => other,
                },
                ..cell
            },
            Key::ArrowUp => Cell {
                row: cell.row.and_then(|row| row.checked_sub(1)),
                ..cell
            },
            key if key == forward => Cell {
                column: (cell.column + 1).min(columns - 1),
                ..cell
            },
            key if key == back => Cell {
                column: cell.column.saturating_sub(1),
                ..cell
            },
            Key::Home => Cell { column: 0, ..cell },
            Key::End => Cell {
                column: columns - 1,
                ..cell
            },
            _ => return false,
        };
        cx.request_paint();
        cx.request_semantics();
        true
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key, repeat: bool) -> EventReply {
        match key {
            Key::Tab => {
                let stops = self.stops();
                let current = stops
                    .iter()
                    .position(|stop| *stop == self.focus)
                    .unwrap_or(0);
                match hosted::tab_stop(current, stops.len(), cx.modifiers().shift) {
                    Some(next) => {
                        self.focus = stops[next];
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            Key::Enter | Key::Space if !repeat => {
                self.activate_stop(cx, true);
                EventReply::Stop
            }
            Key::ArrowDown if self.focus == Stop::Columns => {
                self.open_menu(cx, true);
                EventReply::Stop
            }
            key if self.focus == Stop::Grid && self.grid_key(cx, key) => EventReply::Stop,
            _ => EventReply::Continue,
        }
    }

    /// What the focused part says, for semantics.
    fn focused_text(&self) -> String {
        match self.focus {
            Stop::Filter => self
                .filter
                .as_ref()
                .map(|(editor, _)| editor.field.placeholder().to_string())
                .unwrap_or_default(),
            Stop::Columns => self.columns_label.clone().unwrap_or_default(),
            Stop::Previous => self.labels[0].clone(),
            Stop::Next => self.labels[1].clone(),
            Stop::Grid => {
                let columns = self.columns();
                match (self.cell.row, columns.get(self.cell.column)) {
                    (None, Some(Column::Data(column))) => self.header(*column),
                    (Some(row), Some(_)) => match self.page_keys().get(row) {
                        Some(key) => columns
                            .iter()
                            .filter_map(|column| match column {
                                Column::Data(column) => Some(self.text(*key, *column)),
                                Column::Check => None,
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        None => String::new(),
                    },
                    _ => String::new(),
                }
            }
        }
    }
}

impl CustomControl for DataTable {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured {
            size: self.measure_size(cx),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        self.geometry = self.layout(cx, bounds);
        let content = cx.style(crate::style::StyleQuery::new(
            crate::style::ControlKind::Menu,
            crate::style::Slot::Content,
        ));
        self.menu.arrange(cx, content.min_width);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if let Some(reply) = self.menu_event(cx, event) {
            return reply;
        }
        if let Some(reply) = self.filter_event(cx, event) {
            return reply;
        }
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => self.key(cx, *key, *repeat),
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(true) => {
                if self.pressed.is_none() {
                    let stops = self.stops();
                    self.focus = if cx.modifiers().shift {
                        stops[stops.len() - 1]
                    } else {
                        stops[0]
                    };
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(false) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_table(cx);
    }

    fn overlay(&self) -> Option<Rect> {
        self.menu.frame()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.menu.paint(cx);
    }

    fn animating(&self, time: f64) -> bool {
        self.menu.animating(time)
    }

    fn semantics(&self) -> SemanticNode {
        let mut actions = Vec::new();
        if self.enabled {
            actions.extend([SemanticAction::Activate, SemanticAction::Focus]);
            if self.focus == Stop::Filter {
                actions.push(SemanticAction::SetValue);
            }
            if self.page < self.last_page() {
                actions.push(SemanticAction::Increment);
            }
            if self.page > 0 {
                actions.push(SemanticAction::Decrement);
            }
        }
        let mut node = hosted::node(SemanticRole::Table, &self.name, actions);
        node.value = self
            .status
            .as_ref()
            .map(|status| status(self.selection.len(), self.order.len()));
        node.description = self.focused_text();
        node.state.expanded = self.columns_label.as_ref().map(|_| self.menu.is_open());
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                self.activate_stop(cx, true);
                true
            }
            SemanticAction::Increment if self.page < self.last_page() => {
                self.set_page(cx, self.page + 1);
                true
            }
            SemanticAction::Decrement if self.page > 0 => {
                self.set_page(cx, self.page - 1);
                true
            }
            SemanticAction::SetValue if self.focus == Stop::Filter => {
                let Some((editor, _)) = self.filter.as_mut() else {
                    return false;
                };
                let changed = editor.set_value(cx, value.unwrap_or_default());
                self.refilter(cx);
                changed
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if let Some((editor, _)) = self.filter.as_mut() {
            editor.field.set_enabled_state(enabled);
        }
        if !enabled {
            self.menu.reveal = Default::default();
        }
    }
}
