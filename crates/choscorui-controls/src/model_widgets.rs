//! Companions to [`crate::ItemView`]: a sortable and resizable header,
//! per-cell editors chosen by a factory, a data-widget mapper, a column
//! (Miller) browser, and undo-history rows.

use crate::ValidationState;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::Routed;
use crate::item_view::list_row_height;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    CivilDate, EditableModel, EventReply, Icon, ItemKey, ItemModel, Key, KeyEvent, Locale,
    Measured, ModelError, Point, PointerButton, PointerEvent, Rect, Role, SemanticAction,
    SemanticNode, SemanticRole, SemanticState, Size, SortOrder, UndoStack, Value,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct HeaderColumn {
    pub title: String,
    pub width: f64,
    pub min_width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeaderInput {
    Press { x: f64 },
    Drag { x: f64 },
    Release,
    Key(Key),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeaderResponse {
    pub sort_changed: bool,
    pub resized: Option<usize>,
}

/// Table header: click sorts (ascending, then descending), dragging a
/// column edge resizes, and the keyboard moves between columns (Left and
/// Right), sorts (Space or Return), and resizes (`+` and `-`).
#[derive(Clone, Debug)]
pub struct HeaderView {
    columns: Vec<HeaderColumn>,
    sort: Option<(usize, SortOrder)>,
    focused: usize,
    resizing: Option<(usize, f64, f64)>,
    origin_x: f64,
}

/// Distance from a column edge that starts a resize.
const RESIZE_SLOP: f64 = 4.0;
const KEYBOARD_RESIZE_STEP: f64 = 10.0;

impl HeaderView {
    pub fn new(columns: Vec<HeaderColumn>) -> Self {
        Self {
            columns,
            sort: None,
            focused: 0,
            resizing: None,
            origin_x: 0.0,
        }
    }
    pub fn columns(&self) -> &[HeaderColumn] {
        &self.columns
    }
    pub fn widths(&self) -> Vec<f64> {
        self.columns.iter().map(|column| column.width).collect()
    }
    pub fn sort(&self) -> Option<(usize, SortOrder)> {
        self.sort
    }
    pub fn focused_column(&self) -> usize {
        self.focused
    }
    pub fn set_origin_x(&mut self, x: f64) {
        self.origin_x = x;
    }
    /// Restores saved widths (extra or missing entries are ignored).
    pub fn restore_widths(&mut self, widths: &[f64]) {
        for (column, width) in self.columns.iter_mut().zip(widths) {
            if width.is_finite() {
                column.width = width.max(column.min_width);
            }
        }
    }

    fn edge_at(&self, x: f64) -> Option<usize> {
        let mut right = self.origin_x;
        for (index, column) in self.columns.iter().enumerate() {
            right += column.width;
            if (x - right).abs() <= RESIZE_SLOP {
                return Some(index);
            }
        }
        None
    }

    fn column_at(&self, x: f64) -> Option<usize> {
        let mut left = self.origin_x;
        for (index, column) in self.columns.iter().enumerate() {
            if x >= left && x < left + column.width {
                return Some(index);
            }
            left += column.width;
        }
        None
    }

    fn toggle_sort(&mut self, column: usize) {
        self.sort = Some(match self.sort {
            Some((current, SortOrder::Ascending)) if current == column => {
                (column, SortOrder::Descending)
            }
            _ => (column, SortOrder::Ascending),
        });
    }

    fn resize(&mut self, column: usize, width: f64) -> bool {
        let Some(entry) = self.columns.get_mut(column) else {
            return false;
        };
        let width = width.max(entry.min_width);
        let changed = width != entry.width;
        entry.width = width;
        changed
    }

    pub fn handle(&mut self, input: HeaderInput) -> HeaderResponse {
        let mut response = HeaderResponse::default();
        match input {
            HeaderInput::Press { x } => {
                if let Some(edge) = self.edge_at(x) {
                    self.resizing = Some((edge, x, self.columns[edge].width));
                } else if let Some(column) = self.column_at(x) {
                    self.focused = column;
                    self.toggle_sort(column);
                    response.sort_changed = true;
                }
            }
            HeaderInput::Drag { x } => {
                if let Some((column, start, width)) = self.resizing
                    && self.resize(column, width + x - start)
                {
                    response.resized = Some(column);
                }
            }
            HeaderInput::Release => self.resizing = None,
            HeaderInput::Key(key) => match key {
                Key::ArrowLeft => self.focused = self.focused.saturating_sub(1),
                Key::ArrowRight => {
                    self.focused = (self.focused + 1).min(self.columns.len().saturating_sub(1))
                }
                Key::Space | Key::Enter if !self.columns.is_empty() => {
                    self.toggle_sort(self.focused);
                    response.sort_changed = true;
                }
                Key::Character('+') | Key::Character('=') => {
                    let width = self.columns.get(self.focused).map(|column| column.width);
                    if let Some(width) = width
                        && self.resize(self.focused, width + KEYBOARD_RESIZE_STEP)
                    {
                        response.resized = Some(self.focused);
                    }
                }
                Key::Character('-') => {
                    let width = self.columns.get(self.focused).map(|column| column.width);
                    if let Some(width) = width
                        && self.resize(self.focused, width - KEYBOARD_RESIZE_STEP)
                    {
                        response.resized = Some(self.focused);
                    }
                }
                _ => {}
            },
        }
        response
    }

    pub fn column_semantics(&self, column: usize) -> Option<SemanticNode> {
        let entry = self.columns.get(column)?;
        let value = match self.sort {
            Some((sorted, SortOrder::Ascending)) if sorted == column => Some("ascending".into()),
            Some((sorted, SortOrder::Descending)) if sorted == column => Some("descending".into()),
            _ => None,
        };
        Some(SemanticNode {
            role: SemanticRole::ColumnHeader,
            name: entry.title.clone(),
            description: format!("{} points wide", entry.width.round()),
            value,
            state: SemanticState {
                selected: Some(column == self.focused),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate, SemanticAction::Focus],
            text: None,
        })
    }

    /// Paints the header row in the context's bounds with the `Table`
    /// recipe. `focus_visible` rings the focused column.
    pub fn paint(&self, cx: &mut PaintContext<'_>, focus_visible: bool) {
        let bounds = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Table, slot);
        let header = cx.style(query(Slot::Header));
        cx.slot(bounds, &header, |_, _| {});
        let indicator = cx.style(query(Slot::Indicator));
        let mut x = bounds.origin.x;
        for (index, column) in self.columns.iter().enumerate() {
            let rect = Rect::new(
                Point::new(x, bounds.origin.y),
                Size::new(column.width, bounds.size.height),
            );
            let state = VisualState::NONE
                .set(VisualState::FOCUSED, index == self.focused)
                .set(
                    VisualState::FOCUS_VISIBLE,
                    focus_visible && index == self.focused,
                );
            let cell = cx.style(query(Slot::HeaderCell).state(state));
            cx.list.push_clip(rect);
            cx.slot(rect, &cell, |cx, inner| {
                let mut text = inner;
                if let Some((sorted, order)) = self.sort
                    && sorted == index
                {
                    let icon = if order == SortOrder::Ascending {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    };
                    let width = indicator.icon_size.min(inner.size.width);
                    let icon_x = if cx.rtl {
                        inner.origin.x
                    } else {
                        inner.max_x() - width
                    };
                    let icon_rect =
                        Rect::from_xywh(icon_x, inner.origin.y, width, inner.size.height);
                    cx.icon(icon_rect, icon, &indicator);
                    text.size.width = (text.size.width - width - cell.gap).max(0.0);
                    if cx.rtl {
                        text.origin.x = inner.max_x() - text.size.width;
                    }
                }
                cx.label(text, &column.title, &cell, TextAlign::Start);
            });
            let _ = cx.list.pop_clip();
            // The resize divider at the column's end.
            let divider = Rect::from_xywh(
                x + column.width - header.border_width,
                bounds.origin.y + cell.padding.block_start,
                header.border_width,
                (bounds.size.height - cell.padding.vertical()).max(0.0),
            );
            cx.list.fill_rect(divider, header.border);
            if state.focus_visible() {
                let ring = cx.style(query(Slot::Root).state(state));
                cx.slot(rect, &ring, |_, _| {});
            }
            x += column.width;
        }
    }
}

/// Which editor a cell uses.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorKind {
    Text,
    Integer,
    Decimal { fraction_digits: usize },
    Bool,
    Date,
}

/// Chooses an editor per cell; the default picks by the cell's
/// [`Role::Edit`] value type.
pub trait EditorFactory {
    fn editor_for(&self, model: &dyn ItemModel, key: ItemKey, column: usize) -> Option<EditorKind>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultEditorFactory;

impl EditorFactory for DefaultEditorFactory {
    fn editor_for(&self, model: &dyn ItemModel, key: ItemKey, column: usize) -> Option<EditorKind> {
        if !model.editable(key, column) {
            return None;
        }
        Some(match model.data(key, column, Role::Edit) {
            Value::Int(_) => EditorKind::Integer,
            Value::Float(_) => EditorKind::Decimal { fraction_digits: 2 },
            Value::Bool(_) | Value::Check(_) => EditorKind::Bool,
            _ => EditorKind::Text,
        })
    }
}

/// An in-progress cell edit. The text stays editable while invalid;
/// [`commit`](Self::commit) writes only a valid value.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEdit {
    pub key: ItemKey,
    pub column: usize,
    pub kind: EditorKind,
    pub text: String,
    locale: Locale,
}

impl CellEdit {
    pub fn begin(
        model: &dyn ItemModel,
        factory: &dyn EditorFactory,
        key: ItemKey,
        column: usize,
        locale: Locale,
    ) -> Option<Self> {
        let kind = factory.editor_for(model, key, column)?;
        let value = model.data(key, column, Role::Edit);
        let text = match (&kind, &value) {
            (EditorKind::Integer, Value::Int(number)) => locale.format_integer(*number),
            (EditorKind::Decimal { fraction_digits }, Value::Float(number)) => locale
                .format_decimal(*number, *fraction_digits)
                .unwrap_or_default(),
            _ => value.to_display_string(),
        };
        Some(Self {
            key,
            column,
            kind,
            text,
            locale,
        })
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn parse(&self) -> Result<Value, String> {
        let text = self.text.trim();
        match &self.kind {
            EditorKind::Text => Ok(Value::Text(self.text.clone())),
            EditorKind::Integer => self
                .locale
                .parse_integer(text)
                .map(Value::Int)
                .map_err(|_| "Enter a whole number".into()),
            EditorKind::Decimal { fraction_digits } => {
                let digits = text
                    .rsplit_once(self.locale.data().decimal)
                    .map_or(0, |(_, fraction)| fraction.len());
                if digits > *fraction_digits {
                    return Err(format!("Use at most {fraction_digits} decimal places"));
                }
                self.locale
                    .parse_decimal(text)
                    .map(Value::Float)
                    .map_err(|_| "Enter a number".into())
            }
            EditorKind::Bool => match text.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Ok(Value::Bool(true)),
                "false" | "no" | "0" | "off" => Ok(Value::Bool(false)),
                _ => Err("Enter yes or no".into()),
            },
            EditorKind::Date => CivilDate::parse_short(text, &self.locale)
                .map(|date| Value::Text(date.format_short(&self.locale)))
                .map_err(|_| "Enter a valid date".into()),
        }
    }

    pub fn validation(&self) -> ValidationState {
        match self.parse() {
            Ok(_) => ValidationState::Valid,
            Err(message) => ValidationState::Invalid(message),
        }
    }

    /// Writes the parsed value; on error the edit stays open unchanged.
    pub fn commit(&self, model: &mut dyn EditableModel) -> Result<(), String> {
        let value = self.parse()?;
        model
            .set_data(self.key, self.column, Role::Edit, value)
            .map_err(|error| match error {
                ModelError::ReadOnly => "This cell is read-only".into(),
                ModelError::MissingItem => "The row no longer exists".into(),
                _ => "The value was rejected".into(),
            })
    }
}

/// Binds editor fields to columns of the current model item. Pending
/// (uncommitted or invalid) text is kept per item, so switching rows and
/// back restores it.
#[derive(Clone, Debug, Default)]
pub struct DataMapper {
    current: Option<ItemKey>,
    columns: Vec<usize>,
    pending: HashMap<(ItemKey, usize), String>,
}

impl DataMapper {
    pub fn new(columns: Vec<usize>) -> Self {
        Self {
            current: None,
            columns,
            pending: HashMap::new(),
        }
    }
    pub fn current(&self) -> Option<ItemKey> {
        self.current
    }
    pub fn set_current(&mut self, key: Option<ItemKey>) -> bool {
        let changed = self.current != key;
        self.current = key;
        changed
    }
    /// Field texts for the current item: pending text, else the model's
    /// edit value.
    pub fn values(&self, model: &dyn ItemModel) -> Vec<(usize, String)> {
        let Some(key) = self.current else {
            return Vec::new();
        };
        self.columns
            .iter()
            .map(|column| {
                let text = self
                    .pending
                    .get(&(key, *column))
                    .cloned()
                    .unwrap_or_else(|| model.data(key, *column, Role::Edit).to_display_string());
                (*column, text)
            })
            .collect()
    }
    pub fn edit(&mut self, column: usize, text: impl Into<String>) {
        if let Some(key) = self.current
            && self.columns.contains(&column)
        {
            self.pending.insert((key, column), text.into());
        }
    }
    pub fn has_pending(&self, key: ItemKey) -> bool {
        self.pending.keys().any(|(item, _)| *item == key)
    }
    /// Writes every pending field of the current item that `parse`
    /// accepts. Invalid fields stay pending and are returned with their
    /// messages.
    pub fn submit(
        &mut self,
        model: &mut dyn EditableModel,
        parse: impl Fn(usize, &str) -> Result<Value, String>,
    ) -> Result<(), Vec<(usize, String)>> {
        let Some(key) = self.current else {
            return Ok(());
        };
        let mut errors = Vec::new();
        for column in self.columns.clone() {
            let Some(text) = self.pending.get(&(key, column)).cloned() else {
                continue;
            };
            let result = parse(column, &text).and_then(|value| {
                model
                    .set_data(key, column, Role::Edit, value)
                    .map_err(|error| format!("{error:?}"))
            });
            match result {
                Ok(()) => {
                    self.pending.remove(&(key, column));
                }
                Err(message) => errors.push((column, message)),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    /// Drops pending text for keys that no longer exist.
    pub fn retain_existing(&mut self, model: &dyn ItemModel) {
        self.pending.retain(|(key, _), _| model.row(*key).is_some());
        if self.current.is_some_and(|key| model.row(key).is_none()) {
            self.current = None;
        }
    }
}

/// Column (Miller) browser: column `n` lists the children of the item
/// selected in column `n - 1`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnBrowser {
    path: Vec<ItemKey>,
    focused_column: usize,
}

impl ColumnBrowser {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn path(&self) -> &[ItemKey] {
        &self.path
    }
    pub fn focused_column(&self) -> usize {
        self.focused_column
    }
    /// Parents shown by each column, root first.
    pub fn columns(&self, model: &dyn ItemModel) -> Vec<Option<ItemKey>> {
        let mut columns = vec![None];
        for key in &self.path {
            if model.has_children(*key) || model.can_fetch_more(Some(*key)) {
                columns.push(Some(*key));
            }
        }
        columns
    }
    /// Paints one entry in the context's bounds with the `ListView` row
    /// look. A `selected` entry fills `muted`; in the `active` column it
    /// fills `accent`. An entry with children ends in a chevron.
    pub fn paint_entry(
        cx: &mut PaintContext<'_>,
        label: &str,
        has_children: bool,
        selected: bool,
        active: bool,
    ) {
        let state = VisualState::NONE
            .set(VisualState::SELECTED, selected)
            .set(VisualState::HIGHLIGHTED, selected && active);
        let query = |slot| StyleQuery::new(ControlKind::ListView, slot).state(state);
        let (row, mut cell, mut indicator) = (
            cx.style(query(Slot::Row)),
            cx.style(query(Slot::Cell)),
            cx.style(query(Slot::Indicator)),
        );
        cell.foreground = row.foreground;
        if active && selected {
            indicator.foreground = row.foreground;
        }
        let bounds = cx.bounds;
        cx.slot(bounds, &row, |cx, inner| {
            let mut text = inner;
            if has_children {
                let width = indicator.icon_size.min(inner.size.width);
                let x = if cx.rtl {
                    inner.origin.x
                } else {
                    inner.max_x() - width
                };
                let icon = if cx.rtl {
                    Icon::ChevronLeft
                } else {
                    Icon::ChevronRight
                };
                cx.icon(
                    Rect::new(
                        Point::new(x, inner.origin.y),
                        Size::new(width, inner.size.height),
                    ),
                    icon,
                    &indicator,
                );
                text.size.width = (inner.size.width - width).max(0.0);
                if cx.rtl {
                    text.origin.x += width;
                }
            }
            cx.label(text, label, &cell, TextAlign::Start);
        });
    }

    /// Selects `key` in `column`, truncating deeper columns.
    pub fn select(&mut self, column: usize, key: ItemKey) {
        self.path.truncate(column);
        self.path.push(key);
        self.focused_column = column;
    }
    /// Arrow-key navigation across and within columns.
    pub fn handle_key(&mut self, model: &dyn ItemModel, key: Key) -> bool {
        let column = self.focused_column;
        let parent = if column == 0 {
            None
        } else {
            self.path.get(column - 1).copied()
        };
        let current = self.path.get(column).copied();
        match key {
            Key::ArrowRight => {
                let Some(current) = current else { return false };
                if let Some(first) = model.key(Some(current), 0) {
                    self.select(column + 1, first);
                    true
                } else {
                    false
                }
            }
            Key::ArrowLeft if column > 0 => {
                self.path.truncate(column);
                self.focused_column = column - 1;
                true
            }
            Key::ArrowDown | Key::ArrowUp => {
                let count = model.row_count(parent);
                if count == 0 {
                    return false;
                }
                let row = current.and_then(|key| model.row(key));
                let next = match (key, row) {
                    (Key::ArrowDown, Some(row)) => (row + 1).min(count - 1),
                    (Key::ArrowUp, Some(row)) => row.saturating_sub(1),
                    _ => 0,
                };
                match model.key(parent, next) {
                    Some(next) if Some(next) != current => {
                        self.select(column, next);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
    pub fn retain_existing(&mut self, model: &dyn ItemModel) {
        if let Some(missing) = self.path.iter().position(|key| model.row(*key).is_none()) {
            self.path.truncate(missing);
            self.focused_column = self.focused_column.min(self.path.len().saturating_sub(1));
        }
    }
}

/// One row of an undo-history view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoHistoryRow {
    pub label: String,
    /// The state the document is in now.
    pub current: bool,
    /// Rows after the current one are redoable.
    pub redoable: bool,
}

/// Rows for an undo-history list: the initial state (`empty_label`), then
/// one row per group. Selecting row `n` means "undo or redo until
/// `applied() == n`"; see [`undo_steps_to`].
pub fn undo_history<T>(stack: &UndoStack<T>, empty_label: &str) -> Vec<UndoHistoryRow> {
    let applied = stack.applied();
    std::iter::once(empty_label.to_owned())
        .chain((0..stack.len()).filter_map(|index| stack.label(index).map(str::to_owned)))
        .enumerate()
        .map(|(row, label)| UndoHistoryRow {
            label,
            current: row == applied,
            redoable: row > applied,
        })
        .collect()
}

/// Signed number of steps from the current state to history row `target`:
/// negative means undo that many groups, positive means redo.
pub fn undo_steps_to<T>(stack: &UndoStack<T>, target: usize) -> isize {
    target.min(stack.len()) as isize - stack.applied() as isize
}

impl HeaderView {
    /// Runs `input` and reports the response: a sort change as
    /// `ItemActivated(column)` and a resize as `Changed(Index(column))`.
    fn hosted_input(&mut self, cx: &mut ControlContext<'_>, input: HeaderInput) {
        let response = self.handle(input);
        if response.sort_changed {
            cx.emit(ControlMessage::ItemActivated(self.focused));
            cx.request_semantics();
        }
        if let Some(column) = response.resized {
            cx.emit(ControlMessage::Changed(ControlValue::Index(Some(column))));
            cx.request_layout();
            cx.request_semantics();
        }
        cx.request_paint();
    }
}

impl CustomControl for HeaderView {
    /// The columns' widths side by side, one header cell high.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let cell = cx.style(StyleQuery::new(ControlKind::Table, Slot::HeaderCell));
        let height = cell
            .min_height
            .max(cell.text.line_height + cell.padding.vertical());
        let width = self.columns.iter().map(|column| column.width).sum();
        Measured::new(cx.constraints.constrain(Size::new(width, height)))
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        self.origin_x = bounds.origin.x;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let Some(routed) = crate::hosted::route(cx, event) else {
            return EventReply::Continue;
        };
        match routed {
            Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                self.hosted_input(cx, HeaderInput::Press { x: position.x });
                if self.resizing.is_some() {
                    cx.capture_pointer(true);
                }
                EventReply::Stop
            }
            Routed::Pointer(PointerEvent::Move { position }) if self.resizing.is_some() => {
                self.hosted_input(cx, HeaderInput::Drag { x: position.x });
                EventReply::Stop
            }
            Routed::Pointer(PointerEvent::Up { .. }) | Routed::Cancel
                if self.resizing.is_some() =>
            {
                self.handle(HeaderInput::Release);
                cx.capture_pointer(false);
                EventReply::Stop
            }
            Routed::Key(KeyEvent::Down { key, .. }) => {
                let handled = matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Space
                        | Key::Enter
                        | Key::Character('+' | '=' | '-')
                );
                if !handled {
                    return EventReply::Continue;
                }
                let focused = self.focused;
                self.hosted_input(cx, HeaderInput::Key(key));
                if self.focused != focused {
                    cx.request_semantics();
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let focus_visible = cx.state.focus_visible();
        HeaderView::paint(self, cx, focus_visible);
    }

    /// The focused column's node: the header is one tab stop whose arrow
    /// keys move between columns.
    fn semantics(&self) -> SemanticNode {
        self.column_semantics(self.focused)
            .unwrap_or_else(|| crate::hosted::node(SemanticRole::ColumnHeader, "", Vec::new()))
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                self.hosted_input(cx, HeaderInput::Key(Key::Enter));
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.resizing = None;
        }
    }
}

impl ColumnBrowser {
    /// A browser over its own `model` that mounts in a
    /// [`crate::ControlHost`] as a single node named `name`.
    pub fn hosted(self, name: impl Into<String>, model: Box<dyn ItemModel>) -> HostedColumnBrowser {
        HostedColumnBrowser {
            name: name.into(),
            browser: self,
            model,
            column_width: 0.0,
            row_height: 1.0,
            bounds: Rect::default(),
            rtl: false,
        }
    }
}

/// A [`ColumnBrowser`] with its model, mounted in a
/// [`crate::ControlHost`]. It paints one column per level and reports the
/// path as `Changed(Indices(rows))`: the row of each path item among its
/// siblings.
pub struct HostedColumnBrowser {
    name: String,
    browser: ColumnBrowser,
    model: Box<dyn ItemModel>,
    column_width: f64,
    row_height: f64,
    bounds: Rect,
    rtl: bool,
}

impl HostedColumnBrowser {
    pub fn browser(&self) -> &ColumnBrowser {
        &self.browser
    }

    pub fn model(&self) -> &dyn ItemModel {
        self.model.as_ref()
    }

    /// The row of each path item among its siblings.
    pub fn path_rows(&self) -> Vec<usize> {
        self.browser
            .path()
            .iter()
            .filter_map(|key| self.model.row(*key))
            .collect()
    }

    /// The rectangle of column `column`; columns follow reading order.
    fn column_rect(&self, column: usize) -> Rect {
        let offset = column as f64 * self.column_width;
        let x = if self.rtl {
            self.bounds.max_x() - offset - self.column_width
        } else {
            self.bounds.origin.x + offset
        };
        Rect::from_xywh(
            x,
            self.bounds.origin.y,
            self.column_width,
            self.bounds.size.height,
        )
    }

    /// Rows that fit in a column.
    fn page(&self) -> usize {
        ((self.bounds.size.height / self.row_height).floor() as usize).max(1)
    }

    /// The first row a column shows: it scrolls to keep the path item in
    /// view.
    fn first_row(&self, column: usize) -> usize {
        let selected = self
            .browser
            .path()
            .get(column)
            .and_then(|key| self.model.row(*key))
            .unwrap_or(0);
        (selected + 1).saturating_sub(self.page())
    }

    fn changed(&self, cx: &mut ControlContext<'_>) {
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            self.path_rows(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    fn press(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        let columns = self.browser.columns(self.model.as_ref());
        let Some(column) =
            (0..columns.len()).find(|column| self.column_rect(*column).contains(position))
        else {
            return EventReply::Continue;
        };
        let line = ((position.y - self.bounds.origin.y) / self.row_height).floor() as usize;
        let row = self.first_row(column) + line;
        if let Some(key) = self.model.key(columns[column], row) {
            let before = self.browser.clone();
            self.browser.select(column, key);
            if self.browser != before {
                self.changed(cx);
            }
        }
        EventReply::Stop
    }
}

impl CustomControl for HostedColumnBrowser {
    /// A browser takes the offered size; unbounded, it shows three
    /// columns of ten rows.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let row_height = list_row_height(cx);
        let max = cx.constraints.max;
        let width = if max.width.is_finite() {
            max.width
        } else {
            cx.tokens.spacing * 50.0 * 3.0
        };
        let height = if max.height.is_finite() {
            max.height
        } else {
            row_height * 10.0
        };
        Measured::new(Size::new(width, height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.row_height = list_row_height(cx);
        self.column_width = cx.tokens.spacing * 50.0;
        self.bounds = bounds;
        self.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let Some(routed) = crate::hosted::route(cx, event) else {
            return EventReply::Continue;
        };
        match routed {
            Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => self.press(cx, position),
            Routed::Key(KeyEvent::Down { key, .. }) => {
                // Columns follow reading order, so the keys mirror too.
                let key = match (key, self.rtl) {
                    (Key::ArrowLeft, true) => Key::ArrowRight,
                    (Key::ArrowRight, true) => Key::ArrowLeft,
                    (key, _) => key,
                };
                if !matches!(
                    key,
                    Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                ) {
                    return EventReply::Continue;
                }
                let before = self.browser.clone();
                self.browser.handle_key(self.model.as_ref(), key);
                if self.browser.path() != before.path() {
                    self.changed(cx);
                } else if self.browser != before {
                    cx.request_paint();
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints only the rows that fit in each column.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let focused = cx.state.focused();
        let page = self.page();
        for (column, parent) in self.browser.columns(self.model.as_ref()).iter().enumerate() {
            let rect = self.column_rect(column);
            let first = self.first_row(column);
            let end = self.model.row_count(*parent).min(first + page + 1);
            let selected = self.browser.path().get(column).copied();
            let active = focused && column == self.browser.focused_column();
            for row in first..end {
                let Some(key) = self.model.key(*parent, row) else {
                    continue;
                };
                let entry = Rect::from_xywh(
                    rect.origin.x,
                    rect.origin.y + (row - first) as f64 * self.row_height,
                    rect.size.width,
                    self.row_height,
                );
                let label = self.model.data(key, 0, Role::Display).to_display_string();
                let has_children =
                    self.model.has_children(key) || self.model.can_fetch_more(Some(key));
                ColumnBrowser::paint_entry(
                    &mut cx.at(entry),
                    &label,
                    has_children,
                    selected == Some(key),
                    active,
                );
            }
        }
    }

    fn semantics(&self) -> SemanticNode {
        let mut node =
            crate::hosted::node(SemanticRole::Tree, &self.name, vec![SemanticAction::Focus]);
        node.value = self
            .browser
            .path()
            .get(self.browser.focused_column())
            .map(|key| self.model.data(*key, 0, Role::Display).to_display_string());
        node
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

    /// Takes `Indices`: the row of each path item among its siblings.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(rows) = value else {
            return false;
        };
        let mut browser = ColumnBrowser::new();
        let mut parent = None;
        for (column, row) in rows.iter().enumerate() {
            let Some(key) = self.model.key(parent, *row) else {
                return false;
            };
            browser.select(column, key);
            parent = Some(key);
        }
        self.browser = browser;
        true
    }
}
