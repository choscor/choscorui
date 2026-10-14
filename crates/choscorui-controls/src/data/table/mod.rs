//! A data table over an item model: a filter, column visibility, sorting,
//! row selection, and pagination.

mod layout;

use crate::CommandId;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::menu::{MenuCheck, MenuItem, MenuResponse};
use crate::overlay::menu_popup::MenuPopup;
use crate::popup::gap_anchor;
use crate::style::PaintContext;
use crate::{CommandState, EditCommand, TextField};
use choscorui_core::{
    EventPhase, EventReply, InputModality, ItemKey, ItemModel, Key, KeyEvent, Localizer, Measured,
    PointerButton, PointerEvent, Rect, Role, Selection, SelectionCommand, SelectionMode,
    SemanticAction, SemanticCollection, SemanticNode, SemanticParts, SemanticRole, SortFilterProxy,
    SortOrder, TextFilter, TextInputInfo,
};
use layout::Geometry;
use std::collections::HashMap;

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
    /// The index in `order` of each filtered row.
    index_of: HashMap<ItemKey, usize>,
    selection: Selection,
    hidden: Vec<bool>,
    filter: Option<(TextField, Option<usize>)>,
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
    /// The selected rows that the filter shows.
    shown_selected: usize,
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
            index_of: HashMap::new(),
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
            shown_selected: 0,
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
        self.filter = Some((field, column));
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

    /// Shows a status line under the table, such as "2 of 5 rows
    /// selected.", formatted from the count of selected rows that the
    /// filter shows and the count of filtered rows.
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
        // The last visible data column cannot hide.
        let last =
            !self.hidden[column] && self.hidden.iter().filter(|hidden| !**hidden).count() == 1;
        let state = CommandState {
            enabled: !last,
            checked: !self.hidden[column],
        };
        let _ = self.menu.set_command(CommandId(column as u64), state);
    }

    /// Hides or shows `column`. The last visible data column stays, and
    /// hiding the sort column clears the sort.
    fn toggle_column(&mut self, column: usize) -> bool {
        let Some(hidden) = self.hidden.get(column).copied() else {
            return false;
        };
        if !hidden && self.hidden.iter().filter(|hidden| !**hidden).count() == 1 {
            return false;
        }
        self.hidden[column] = !hidden;
        if !hidden
            && self
                .proxy
                .sort()
                .is_some_and(|(sorted, _)| sorted == column)
        {
            self.proxy.set_sort(None);
            self.sync();
        }
        for column in 0..self.hidden.len() {
            self.sync_menu(column);
        }
        let count = self.columns().len();
        self.cell.column = self.cell.column.min(count - 1);
        true
    }

    /// Counts the selected rows that the filter shows. The work is the
    /// selection size, not the filtered rows.
    fn count_selected(&mut self) {
        self.shown_selected = self
            .selection
            .keys()
            .filter(|key| self.index_of.contains_key(key))
            .count();
    }

    /// Rebuilds the display order from the proxy.
    fn sync(&mut self) {
        self.proxy.sync(self.model.as_ref());
        let view = self.proxy.view(self.model.as_ref());
        self.order = (0..view.row_count(None))
            .filter_map(|row| view.key(None, row))
            .collect();
        self.index_of = self
            .order
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect();
        self.page = self.page.min(self.last_page());
        self.count_selected();
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

    fn emit_selection(&mut self, cx: &mut ControlContext<'_>) {
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            self.selected_rows(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    /// Toggles `key`, a row that the filter shows, and moves the anchor
    /// to it.
    fn toggle_key(&mut self, key: ItemKey) {
        let index_of = &self.index_of;
        self.selection.apply_indexed(
            SelectionMode::Multi,
            SelectionCommand::Toggle,
            key,
            &self.order,
            |item| index_of.get(&item).copied(),
        );
        if self.selection.is_selected(key) {
            self.shown_selected += 1;
        } else {
            self.shown_selected -= 1;
        }
    }

    /// Adds the filtered rows `range` to the selection. The work is the
    /// rows of the range, not every filtered row.
    fn select_range(&mut self, range: std::ops::RangeInclusive<usize>) {
        let keys = &self.order[range];
        let added = keys
            .iter()
            .filter(|key| !self.selection.is_selected(**key))
            .count();
        self.selection.select_all(keys);
        self.shown_selected += added;
    }

    /// Toggles `row` of the page. With `extend`, it adds every row from
    /// the last toggled row to `row` and keeps the other selected rows.
    fn toggle_row(&mut self, cx: &mut ControlContext<'_>, row: usize, extend: bool) {
        let Some(key) = self.page_keys().get(row).copied() else {
            return;
        };
        let position = |key| self.index_of.get(&key).copied();
        let range = self
            .selection
            .anchor()
            .filter(|_| extend)
            .and_then(position)
            .zip(position(key));
        match range {
            Some((anchor, end)) => self.select_range(anchor.min(end)..=anchor.max(end)),
            None => self.toggle_key(key),
        }
        self.emit_selection(cx);
    }

    /// Selects every row that the filter shows, on every page.
    fn select_all_rows(&mut self, cx: &mut ControlContext<'_>) {
        if self.selection.select_all(&self.order) {
            self.shown_selected = self.order.len();
            self.emit_selection(cx);
        }
    }

    fn toggle_page(&mut self, cx: &mut ControlContext<'_>) {
        let start = (self.page * self.page_size).min(self.order.len());
        let end = (start + self.page_size).min(self.order.len());
        if start == end {
            return;
        }
        if self.page_check() == Some(true) {
            for row in start..end {
                self.toggle_key(self.order[row]);
            }
        } else {
            self.select_range(start..=end - 1);
        }
        self.emit_selection(cx);
    }

    /// Sorts by `column`: ascending, descending, then source order. A
    /// sort change shows the first page.
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
        self.show_page(0);
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    /// Shows `page` and keeps the focused cell and stop on it.
    fn show_page(&mut self, page: usize) {
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
    }

    fn set_page(&mut self, cx: &mut ControlContext<'_>, page: usize) {
        let page = page.min(self.last_page());
        if page == self.page {
            return;
        }
        self.show_page(page);
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
            (Some(row), Some(_)) => self.toggle_row(cx, row, cx.modifiers().shift),
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

    /// Opens the column menu below its button at `time`.
    fn show_menu(&mut self, time: f64, keyboard: bool) {
        let button = self.geometry.columns;
        let width = self.menu_width();
        // The menu's end edge lines up with the button's end edge.
        let x = if self.rtl {
            button.origin.x
        } else {
            button.max_x() - width
        };
        let anchor = Rect::from_xywh(x, button.origin.y, width, button.size.height);
        self.menu
            .open(gap_anchor(anchor, 4.0, None), time, keyboard);
    }

    fn open_menu(&mut self, cx: &mut ControlContext<'_>, keyboard: bool) {
        self.show_menu(cx.time(), keyboard);
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
                if self.toggle_column(column as usize) {
                    cx.request_layout();
                    cx.request_semantics();
                }
                self.close_menu(cx);
            }
            MenuResponse::Closed => {
                self.dismiss_menu(cx);
            }
        }
        cx.request_paint();
    }

    /// Closes the open menu without a choice, and reports `Dismissed`. The
    /// reply is `Stop` when the menu closed.
    fn dismiss_menu(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let open = self.menu.is_open();
        self.close_menu(cx);
        hosted::dismissed(cx, open)
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
            ControlEvent::Pointer(pointer) if self.menu.wheel(pointer) => {
                cx.request_paint();
                Some(EventReply::Stop)
            }
            ControlEvent::Pointer(pointer) => {
                let released = matches!(pointer, PointerEvent::Up { .. });
                let Some(response) = self.menu.pointer(pointer.position(), released) else {
                    // A press on the table outside the menu only closes it.
                    return matches!(pointer, PointerEvent::Down { .. })
                        .then(|| self.dismiss_menu(cx));
                };
                if released {
                    self.menu_response(cx, response);
                } else {
                    cx.request_paint();
                }
                Some(EventReply::Stop)
            }
            ControlEvent::Dismiss => Some(self.dismiss_menu(cx)),
            ControlEvent::Key(KeyEvent::Down { key: Key::Tab, .. })
            | ControlEvent::FocusChanged(false) => {
                self.dismiss_menu(cx);
                None
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) if !cx.modifiers().is_chord() => {
                let response = self.menu.key(*key, cx.time());
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
            ControlEvent::Pointer(_)
            | ControlEvent::Text(_)
            | ControlEvent::Compose(_)
            | ControlEvent::Edit(_) => focused,
            ControlEvent::Key(KeyEvent::Down { key, .. }) => focused && *key != Key::Tab,
            _ => false,
        };
        if !forward {
            return None;
        }
        let reply = editor.hosted_event(cx, event, rect);
        if matches!(event, ControlEvent::Pointer(PointerEvent::Down { .. })) {
            self.focus = Stop::Filter;
        }
        self.refilter(cx);
        (reply == EventReply::Stop).then_some(EventReply::Stop)
    }

    /// Applies the filter text when it changed. Returns true when it
    /// changed.
    fn apply_filter(&mut self) -> bool {
        let Some((editor, column)) = &self.filter else {
            return false;
        };
        let value = editor.value().to_string();
        let current = self
            .proxy
            .filter()
            .map_or("", |filter| filter.pattern.as_str());
        if value == current {
            return false;
        }
        let filter = (!value.is_empty()).then(|| TextFilter {
            column: *column,
            ..TextFilter::contains(value)
        });
        self.proxy.set_filter(filter);
        self.page = 0;
        self.sync();
        self.cell.row = None;
        true
    }

    fn refilter(&mut self, cx: &mut ControlContext<'_>) {
        if self.apply_filter() {
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
        }
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
                        Part::Check(row) => self.toggle_row(cx, row, cx.modifiers().shift),
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
        let cell = self.cell;
        self.cell = match key.in_reading_order(self.rtl) {
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
            Key::ArrowRight => Cell {
                column: (cell.column + 1).min(columns - 1),
                ..cell
            },
            Key::ArrowLeft => Cell {
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
            // Page Up and Page Down turn the pages and keep the cell.
            Key::PageUp | Key::PageDown if self.focus == Stop::Grid => {
                let page = if key == Key::PageUp {
                    self.page.saturating_sub(1)
                } else {
                    self.page + 1
                };
                self.set_page(cx, page);
                EventReply::Stop
            }
            Key::Character('a') if self.focus == Stop::Grid && cx.modifiers().command => {
                self.select_all_rows(cx);
                EventReply::Stop
            }
            key if self.focus == Stop::Grid && self.grid_key(cx, key) => EventReply::Stop,
            _ => EventReply::Continue,
        }
    }

    /// What the focused part says, for semantics.
    /// The filter field, the columns button with its open menu, a `Row`
    /// part for the header and for each row on the page, and the page
    /// buttons. A header row has a `ColumnHeader` part for each data
    /// column, with the sort order as the description. A row is selected
    /// with the selection, and has a `Checkbox` part and a `Cell` part for
    /// each data column. The focused stop or cell is the focused part.
    fn table_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let g = &self.geometry;
        let focus = |parts: &mut SemanticParts, focused: bool| {
            if focused {
                parts.set_focused(Some(parts.len() - 1));
            }
        };
        if let Some((editor, _)) = &self.filter {
            let part = parts.push(
                None,
                SemanticRole::SearchField,
                editor.placeholder(),
                g.filter,
            );
            part.node.value = Some(editor.value().to_owned());
            focus(parts, self.focus == Stop::Filter);
        }
        if let Some(label) = &self.columns_label {
            let part = hosted::pressable(
                parts,
                None,
                SemanticRole::PopUpButton,
                label,
                g.columns,
                true,
            );
            part.node.state.expanded = Some(self.menu.is_open());
            focus(parts, self.focus == Stop::Columns);
            self.menu.semantic_parts(Some(parts.len() - 1), parts);
        }
        let columns = self.columns();
        let data = columns
            .iter()
            .filter(|column| matches!(column, Column::Data(_)))
            .count();
        parts.set_collection(SemanticCollection {
            item_count: self.order.len(),
            column_count: Some(data),
        });
        let grid = |row: Option<usize>, column: usize| {
            self.focus == Stop::Grid && self.cell == Cell { row, column }
        };
        parts.push(None, SemanticRole::Row, &self.name, g.header);
        let header = Some(parts.len() - 1);
        for (index, column) in columns.iter().enumerate() {
            let rect = self.cell_rect(index, g.header);
            let Column::Data(column) = *column else {
                // The header check box selects every row.
                let role = SemanticRole::Checkbox;
                let part = hosted::pressable(parts, header, role, &self.name, rect, true);
                let all = !self.order.is_empty() && self.shown_selected == self.order.len();
                part.node.state.checked = Some(all);
                focus(parts, grid(None, index));
                continue;
            };
            let name = self.header(column);
            let part =
                hosted::pressable(parts, header, SemanticRole::ColumnHeader, &name, rect, true);
            part.node.description = match self.sort() {
                Some((sorted, SortOrder::Ascending)) if sorted == column => {
                    strings.message("choscorui.column.ascending", &[])
                }
                Some((sorted, SortOrder::Descending)) if sorted == column => {
                    strings.message("choscorui.column.descending", &[])
                }
                _ => String::new(),
            };
            focus(parts, grid(None, index));
        }
        let first = self.page * self.page_size;
        for (row, (key, rect)) in self.page_keys().iter().zip(&g.rows).enumerate() {
            let selected = self.selection.is_selected(*key);
            let cells: Vec<String> = columns
                .iter()
                .filter_map(|column| match column {
                    Column::Data(column) => Some(self.text(*key, *column)),
                    Column::Check => None,
                })
                .collect();
            let part = parts.push(None, SemanticRole::Row, &cells.join(", "), *rect);
            part.node.state.selected = Some(selected);
            part.item(first + row);
            let parent = Some(parts.len() - 1);
            let mut texts = cells.iter();
            for (index, column) in columns.iter().enumerate() {
                let rect = self.cell_rect(index, *rect);
                let part = match column {
                    Column::Check => {
                        // The check box takes the name of the row.
                        let name = cells.first().map_or("", String::as_str);
                        let role = SemanticRole::Checkbox;
                        let part = hosted::pressable(parts, parent, role, name, rect, true);
                        part.node.state.checked = Some(selected);
                        part
                    }
                    Column::Data(_) => {
                        let text = texts.next().map_or("", String::as_str);
                        hosted::pressable(parts, parent, SemanticRole::Cell, text, rect, true)
                    }
                };
                part.node.state.selected = Some(selected);
                focus(parts, grid(Some(row), index));
            }
        }
        for (stop, label, rect, enabled) in [
            (Stop::Previous, &self.labels[0], g.previous, self.page > 0),
            (
                Stop::Next,
                &self.labels[1],
                g.next,
                self.page < self.last_page(),
            ),
        ] {
            hosted::pressable(parts, None, SemanticRole::Button, label, rect, enabled);
            focus(parts, self.focus == stop);
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
            ControlEvent::Edit(EditCommand::SelectAll) if self.focus == Stop::Grid => {
                self.select_all_rows(cx);
                EventReply::Stop
            }
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(true) => {
                // A press keeps the inner focus; the press itself moves it
                // to the part under the pointer.
                if cx.input_modality() == InputModality::Keyboard {
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

    fn text_input(&self) -> Option<TextInputInfo> {
        if self.focus != Stop::Filter {
            return None;
        }
        self.filter.as_ref()?.0.hosted_text_input()
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
            .map(|status| status(self.shown_selected, self.order.len()));
        node.state.expanded = self.columns_label.as_ref().map(|_| self.menu.is_open());
        node.state.disabled = !self.enabled;
        node
    }

    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        self.table_parts(strings, parts);
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
                let changed = editor.hosted_set_value(cx, value.unwrap_or_default());
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
            editor.set_enabled_state(enabled);
        }
        if !enabled {
            self.menu.reveal = Default::default();
        }
    }

    /// Takes `Indices`, the selected source rows, or `Text` for the
    /// filter input. Rejects a row past the model.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Indices(rows) => {
                let keys: Option<Vec<_>> =
                    rows.iter().map(|row| self.model.key(None, *row)).collect();
                let Some(keys) = keys else {
                    return false;
                };
                self.selection.clear();
                self.selection.select_all(&keys);
                self.count_selected();
                true
            }
            ControlValue::Text(_) => {
                let Some((editor, _)) = self.filter.as_mut() else {
                    return false;
                };
                editor.take_text(value);
                self.apply_filter();
                true
            }
            _ => false,
        }
    }

    /// Opens or closes the column menu.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if self.columns_label.is_none() || (open && !self.enabled) {
            return false;
        }
        if !open {
            self.menu.close(time);
        } else if !self.menu.is_open() {
            self.show_menu(time, true);
        }
        true
    }
}
