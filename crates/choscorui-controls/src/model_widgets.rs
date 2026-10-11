//! Companions to [`crate::ItemView`]: a sortable and resizable header,
//! per-cell editors chosen by a factory, a data-widget mapper, a column
//! (Miller) browser, and undo-history rows.

use crate::ValidationState;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    CivilDate, EditableModel, Icon, ItemKey, ItemModel, Key, Locale, ModelError, Point, Rect, Role,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, SortOrder, UndoStack, Value,
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
