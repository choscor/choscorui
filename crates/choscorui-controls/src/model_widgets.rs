//! Companions to [`crate::ItemView`]: a sortable and resizable header,
//! per-cell editors chosen by a factory, a data-widget mapper, a column
//! (Miller) browser, and undo-history rows.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{Parts, Routed, TypeAhead};
use crate::item_view::list_row_height;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState, fill_area,
};
use crate::validation::{DateError, NumberError, ValidationError};
use crate::{DateInput, InputFilter, NumberInput, ValidationState};
use crate::{OverlayScrollBar, ScrollOrientation};
use choscorui_core::{
    CivilDate, EditableModel, EventPhase, EventReply, Icon, ItemKey, ItemModel, Key, KeyEvent,
    LayoutDirection, Locale, Localizer, Measured, ModelError, Point, PointerButton, PointerEvent,
    Rect, Role, SemanticAction, SemanticCollection, SemanticNode, SemanticParts, SemanticRole,
    SemanticState, Size, SortOrder, UndoStack, Value, WheelUnit,
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
    Press {
        x: f64,
    },
    Drag {
        x: f64,
    },
    /// The pointer release at `x`. A release in the pressed column sorts.
    Release {
        x: f64,
    },
    /// Ends a press or a resize without a change: the width returns.
    Cancel,
    Key(Key),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeaderResponse {
    pub sort_changed: bool,
    pub resized: Option<usize>,
}

/// Table header: a click sorts (ascending, descending, then no sort),
/// dragging a column edge resizes, and the keyboard moves between columns
/// (Left, Right, Home, and End), sorts (Space or Return), and resizes
/// (`+` and `-`). Escape during a resize restores the width.
#[derive(Clone, Debug)]
pub struct HeaderView {
    columns: Vec<HeaderColumn>,
    sort: Option<(usize, SortOrder)>,
    focused: usize,
    /// The column, the pointer offset, and the width at a resize press.
    resizing: Option<(usize, f64, f64)>,
    /// The column that a press started on; the release sorts it.
    pressed: Option<usize>,
    /// The start edge of the first column in reading order.
    origin_x: f64,
    /// The bounds of the last arrange in a control host.
    bounds: Rect,
    rtl: bool,
    /// Hover and press of the cells.
    tracked: Parts,
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
            pressed: None,
            origin_x: 0.0,
            bounds: Rect::default(),
            rtl: false,
            tracked: Parts::default(),
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
    /// Places the start edge of the first column at `x`: its left edge,
    /// or its right edge in a right-to-left layout.
    pub fn set_origin_x(&mut self, x: f64) {
        self.origin_x = x;
    }
    /// Runs the columns from the right in a right-to-left layout, for
    /// pointer input and the arrow keys. Paint reads the direction from
    /// its context.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.rtl = direction == LayoutDirection::Rtl;
    }
    /// Restores saved widths (extra or missing entries are ignored).
    pub fn restore_widths(&mut self, widths: &[f64]) {
        for (column, width) in self.columns.iter_mut().zip(widths) {
            if width.is_finite() {
                column.width = width.max(column.min_width);
            }
        }
    }
    /// Restores a saved sort. A column past the last column clears it.
    pub fn restore_sort(&mut self, sort: Option<(usize, SortOrder)>) {
        self.sort = sort.filter(|(column, _)| *column < self.columns.len());
    }

    /// The distance of `x` from the start edge, in reading order.
    fn offset(&self, x: f64) -> f64 {
        if self.rtl {
            self.origin_x - x
        } else {
            x - self.origin_x
        }
    }

    fn edge_at(&self, offset: f64) -> Option<usize> {
        let mut end = 0.0;
        for (index, column) in self.columns.iter().enumerate() {
            end += column.width;
            if (offset - end).abs() <= RESIZE_SLOP {
                return Some(index);
            }
        }
        None
    }

    fn column_at(&self, offset: f64) -> Option<usize> {
        let mut start = 0.0;
        for (index, column) in self.columns.iter().enumerate() {
            if offset >= start && offset < start + column.width {
                return Some(index);
            }
            start += column.width;
        }
        None
    }

    /// Sorts by `column`: ascending, descending, then no sort.
    fn toggle_sort(&mut self, column: usize) {
        self.sort = match self.sort {
            Some((current, SortOrder::Ascending)) if current == column => {
                Some((column, SortOrder::Descending))
            }
            Some((current, SortOrder::Descending)) if current == column => None,
            _ => Some((column, SortOrder::Ascending)),
        };
    }

    /// Ends a press or a resize. A resize gets its first width back.
    fn cancel(&mut self, response: &mut HeaderResponse) {
        self.pressed = None;
        if let Some((column, _, width)) = self.resizing.take()
            && self.resize(column, width)
        {
            response.resized = Some(column);
        }
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
                let offset = self.offset(x);
                if let Some(edge) = self.edge_at(offset) {
                    self.resizing = Some((edge, offset, self.columns[edge].width));
                } else if let Some(column) = self.column_at(offset) {
                    self.focused = column;
                    self.pressed = Some(column);
                }
            }
            HeaderInput::Drag { x } => {
                if let Some((column, start, width)) = self.resizing
                    && self.resize(column, width + self.offset(x) - start)
                {
                    response.resized = Some(column);
                }
            }
            HeaderInput::Release { x } => {
                self.resizing = None;
                if let Some(column) = self.pressed.take()
                    && self.column_at(self.offset(x)) == Some(column)
                {
                    self.toggle_sort(column);
                    response.sort_changed = true;
                }
            }
            HeaderInput::Cancel => self.cancel(&mut response),
            HeaderInput::Key(key) => match key.in_reading_order(self.rtl) {
                Key::Escape => self.cancel(&mut response),
                Key::ArrowLeft => self.focused = self.focused.saturating_sub(1),
                Key::ArrowRight => {
                    self.focused = (self.focused + 1).min(self.columns.len().saturating_sub(1))
                }
                Key::Home => self.focused = 0,
                Key::End => self.focused = self.columns.len().saturating_sub(1),
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

    /// The semantics of header `column`, with the sort order and the
    /// width in the language of `strings`.
    pub fn column_semantics(&self, column: usize, strings: &Localizer) -> Option<SemanticNode> {
        let entry = self.columns.get(column)?;
        let value = match self.sort {
            Some((sorted, SortOrder::Ascending)) if sorted == column => {
                Some(strings.message("choscorui.column.ascending", &[]))
            }
            Some((sorted, SortOrder::Descending)) if sorted == column => {
                Some(strings.message("choscorui.column.descending", &[]))
            }
            _ => None,
        };
        let width = entry.width.round().to_string();
        Some(SemanticNode {
            role: SemanticRole::ColumnHeader,
            name: entry.title.clone(),
            description: strings.message("choscorui.column.width", &[("width", &width)]),
            value,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate, SemanticAction::Focus],
            text: None,
        })
    }

    /// Paints the header row in the context's bounds with the `Table`
    /// recipe. `focus_visible` rings the focused column.
    pub fn paint(&self, cx: &mut PaintContext<'_>, focus_visible: bool) {
        let host = cx.state;
        let bounds = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Table, slot);
        let header = cx.style(query(Slot::Header));
        cx.slot(bounds, &header, |_, _| {});
        let indicator = cx.style(query(Slot::Indicator));
        let handle = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Handle));
        let mut offset = 0.0;
        for (index, column) in self.columns.iter().enumerate() {
            let x = if cx.rtl {
                bounds.max_x() - offset - column.width
            } else {
                bounds.origin.x + offset
            };
            let rect = Rect::new(
                Point::new(x, bounds.origin.y),
                Size::new(column.width, bounds.size.height),
            );
            let state = self
                .tracked
                .state(index, host)
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
            let end = if cx.rtl { x } else { x + column.width };
            let divider = Rect::from_xywh(
                if cx.rtl {
                    end
                } else {
                    end - header.border_width
                },
                bounds.origin.y + cell.padding.block_start,
                header.border_width,
                (bounds.size.height - cell.padding.vertical()).max(0.0),
            );
            cx.list.fill_rect(divider, header.border);
            // The edge that starts a resize shows the cursor of a handle.
            let edge = Rect::from_xywh(
                end - RESIZE_SLOP,
                bounds.origin.y,
                RESIZE_SLOP * 2.0,
                bounds.size.height,
            );
            cx.cursor(edge, handle.cursor);
            // The ring stays inside the cell: the control clips its
            // bounds, and the next cell paints over the cell's end.
            if state.focus_visible() {
                let root = cx.style(query(Slot::Root).state(state));
                cx.ring_inside(rect, &root);
            }
            offset += column.width;
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
    range: (Option<f64>, Option<f64>),
}

/// The text that an editor shows for `value`: a number in the locale
/// format, with `fraction_digits` fraction digits for a decimal, or the
/// display string of another value.
fn edit_text(value: &Value, fraction_digits: usize, locale: &Locale) -> String {
    match value {
        Value::Int(number) => locale.format_integer(*number),
        Value::Float(number) => locale
            .format_decimal(*number, fraction_digits)
            .unwrap_or_default(),
        _ => value.to_display_string(),
    }
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
        let digits = match kind {
            EditorKind::Decimal { fraction_digits } => fraction_digits,
            _ => 0,
        };
        let text = edit_text(&model.data(key, column, Role::Edit), digits, &locale);
        Some(Self {
            key,
            column,
            kind,
            text,
            locale,
            range: (None, None),
        })
    }

    /// Limits a number column to `min` and `max`. With a minimum that is
    /// not negative, the editor refuses a minus sign. A value outside the
    /// range gets an error that names the bound.
    #[must_use]
    pub fn with_range(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.range = (min, max);
        self
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// The filter of the editor's text: the number rule for a number
    /// column, and the date rule for a date column. A bool column takes
    /// no typed text: its editor is a checkbox.
    pub fn filter(&self) -> InputFilter {
        match &self.kind {
            EditorKind::Text => InputFilter::new(),
            EditorKind::Integer => self.number(0).filter(),
            EditorKind::Decimal { fraction_digits } => self.number(*fraction_digits).filter(),
            EditorKind::Bool => InputFilter::new().rule(|_| false),
            EditorKind::Date => DateInput::new(self.locale.clone()).filter(),
        }
    }

    /// The value of a bool column's checkbox, or `None` for another
    /// column.
    pub fn checked(&self) -> Option<bool> {
        match (&self.kind, self.parse()) {
            (EditorKind::Bool, Ok(Value::Bool(checked))) => Some(checked),
            (EditorKind::Bool, _) => Some(false),
            _ => None,
        }
    }

    /// Sets the value of a bool column's checkbox. Space toggles it.
    pub fn set_checked(&mut self, checked: bool) {
        if self.kind == EditorKind::Bool {
            self.text = checked.to_string();
        }
    }

    fn number(&self, fraction_digits: usize) -> NumberInput {
        NumberInput::new(self.locale.clone(), fraction_digits)
            .with_range(self.range.0, self.range.1)
    }

    pub fn parse(&self) -> Result<Value, ValidationError> {
        let text = self.text.trim();
        let number = |fraction_digits| match self.number(fraction_digits).commit(text) {
            (_, Some(value)) => Ok(value),
            (ValidationState::Invalid(error), None) => Err(error),
            _ => Err(NumberError::NotANumber.into()),
        };
        match &self.kind {
            EditorKind::Text => Ok(Value::Text(self.text.clone())),
            EditorKind::Integer => {
                number(0)?;
                self.locale
                    .parse_integer(text)
                    .map(Value::Int)
                    .map_err(|_| NumberError::NotANumber.into())
            }
            EditorKind::Decimal { fraction_digits } => number(*fraction_digits).map(Value::Float),
            EditorKind::Bool => match text.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Ok(Value::Bool(true)),
                "false" | "no" | "0" | "off" => Ok(Value::Bool(false)),
                _ => Err(ValidationError::YesNo),
            },
            EditorKind::Date => CivilDate::parse_short(text, &self.locale)
                .map(|date| Value::Text(date.format_short(&self.locale)))
                .map_err(|_| DateError::NotADate.into()),
        }
    }

    pub fn validation(&self) -> ValidationState {
        match self.parse() {
            Ok(_) => ValidationState::Valid,
            Err(error) => ValidationState::Invalid(error),
        }
    }

    /// Writes the parsed value; on error the edit stays open unchanged.
    pub fn commit(&self, model: &mut dyn EditableModel) -> Result<(), ValidationError> {
        let value = self.parse()?;
        model
            .set_data(self.key, self.column, Role::Edit, value)
            .map_err(ValidationError::from)
    }
}

impl From<ModelError> for ValidationError {
    fn from(error: ModelError) -> Self {
        match error {
            ModelError::ReadOnly => Self::ReadOnly,
            ModelError::MissingItem => Self::MissingRow,
            _ => Self::Rejected,
        }
    }
}

/// Binds editor fields to columns of the current model item. Pending
/// (uncommitted or invalid) text is kept per item, so switching rows and
/// back restores it. The errors of the last submit stay with their
/// pending text.
#[derive(Clone, Debug, Default)]
pub struct DataMapper {
    current: Option<ItemKey>,
    columns: Vec<usize>,
    pending: HashMap<(ItemKey, usize), String>,
    errors: HashMap<(ItemKey, usize), ValidationError>,
    locale: Option<Locale>,
}

impl DataMapper {
    pub fn new(columns: Vec<usize>) -> Self {
        Self {
            columns,
            ..Self::default()
        }
    }

    /// Formats the numbers of the model in the conventions of `locale`
    /// when the fields fill: integers with group separators, and decimals
    /// with two fraction digits.
    #[must_use]
    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.locale = Some(locale);
        self
    }

    /// The text of the model value of `column` in item `key`.
    fn model_text(&self, model: &dyn ItemModel, key: ItemKey, column: usize) -> String {
        let value = model.data(key, column, Role::Edit);
        match &self.locale {
            Some(locale) => edit_text(&value, 2, locale),
            None => value.to_display_string(),
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
                    .unwrap_or_else(|| self.model_text(model, key, *column));
                (*column, text)
            })
            .collect()
    }
    /// Keeps `text` as the pending text of `column` in the current item.
    /// Text that equals the model value, apart from surrounding spaces,
    /// is not pending. The field keeps its error until
    /// [`check`](Self::check) or [`submit`](Self::submit) runs again.
    pub fn edit(&mut self, model: &dyn ItemModel, column: usize, text: impl Into<String>) {
        let Some(key) = self.current.filter(|_| self.columns.contains(&column)) else {
            return;
        };
        let text = text.into();
        if text.trim() == self.model_text(model, key, column).trim() {
            self.pending.remove(&(key, column));
            self.errors.remove(&(key, column));
        } else {
            self.pending.insert((key, column), text);
        }
    }

    /// Checks the pending text of `column` in the current item again with
    /// `parse`, when the field has an error. A fix clears the error.
    pub fn check(
        &mut self,
        column: usize,
        parse: impl Fn(usize, &str) -> Result<Value, ValidationError>,
    ) {
        let Some(key) = self.current else {
            return;
        };
        let field = (key, column);
        if let (Some(text), true) = (self.pending.get(&field), self.errors.contains_key(&field)) {
            match parse(column, text) {
                Ok(_) => self.errors.remove(&field),
                Err(error) => self.errors.insert(field, error),
            };
        }
    }

    pub fn has_pending(&self, key: ItemKey) -> bool {
        self.pending.keys().any(|(item, _)| *item == key)
    }

    /// Whether `column` of the current item has pending text.
    pub fn is_pending(&self, column: usize) -> bool {
        self.current
            .is_some_and(|key| self.pending.contains_key(&(key, column)))
    }

    /// The error of `column` in the current item, from the last submit
    /// or check.
    pub fn error(&self, column: usize) -> Option<&ValidationError> {
        self.errors.get(&(self.current?, column))
    }
    /// Writes every pending field of the current item that `parse`
    /// accepts. Invalid fields stay pending and are returned with their
    /// errors, as are the fields that the model refuses.
    pub fn submit(
        &mut self,
        model: &mut dyn EditableModel,
        parse: impl Fn(usize, &str) -> Result<Value, ValidationError>,
    ) -> Result<(), Vec<(usize, ValidationError)>> {
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
                    .map_err(ValidationError::from)
            });
            match result {
                Ok(()) => {
                    self.pending.remove(&(key, column));
                    self.errors.remove(&(key, column));
                }
                Err(message) => {
                    self.errors.insert((key, column), message.clone());
                    errors.push((column, message));
                }
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
        self.errors.retain(|(key, _), _| model.row(*key).is_some());
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
        Self::paint_row(cx, label, has_children, selected, active, VisualState::NONE);
    }

    /// Paints one entry as [`Self::paint_entry`] does, with the hover,
    /// press, and disabled flags in `pointer`.
    fn paint_row(
        cx: &mut PaintContext<'_>,
        label: &str,
        has_children: bool,
        selected: bool,
        active: bool,
        pointer: VisualState,
    ) {
        let state = pointer
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

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        self.bounds = bounds;
        self.origin_x = if self.rtl {
            bounds.max_x()
        } else {
            bounds.origin.x
        };
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let bounds = cx.bounds();
        let mut tracked = self.tracked;
        tracked.track(cx, event, |point| {
            let offset = self.offset(point.x);
            let cell = bounds.contains(point) && self.edge_at(offset).is_none();
            cell.then(|| self.column_at(offset)).flatten()
        });
        self.tracked = tracked;
        let Some(routed) = crate::hosted::route(cx, event) else {
            return EventReply::Continue;
        };
        match routed {
            Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                self.hosted_input(cx, HeaderInput::Press { x: position.x });
                cx.capture_pointer(true);
                EventReply::Stop
            }
            Routed::Pointer(PointerEvent::Move { position }) if self.resizing.is_some() => {
                self.hosted_input(cx, HeaderInput::Drag { x: position.x });
                EventReply::Stop
            }
            Routed::Pointer(PointerEvent::Up { position, .. })
                if self.resizing.is_some() || self.pressed.is_some() =>
            {
                self.hosted_input(cx, HeaderInput::Release { x: position.x });
                cx.capture_pointer(false);
                EventReply::Stop
            }
            Routed::Cancel if self.resizing.is_some() || self.pressed.is_some() => {
                self.hosted_input(cx, HeaderInput::Cancel);
                cx.capture_pointer(false);
                EventReply::Continue
            }
            Routed::Key(KeyEvent::Down { key, .. }) => {
                let handled = matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Home
                        | Key::End
                        | Key::Space
                        | Key::Enter
                        | Key::Character('+' | '=' | '-')
                ) || (key == Key::Escape && self.resizing.is_some());
                if key == Key::Escape && self.resizing.is_some() {
                    cx.capture_pointer(false);
                }
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
        self.semantics_in(Localizer::builtin())
    }

    /// A `Group` with the actions of the focused column.
    fn semantics_in(&self, _strings: &Localizer) -> SemanticNode {
        let actions = if self.columns.is_empty() {
            Vec::new()
        } else {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        };
        crate::hosted::node(SemanticRole::Group, "", actions)
    }

    /// A `ColumnHeader` part for each column. The focused column is the
    /// focused part.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let bounds = self.bounds;
        let mut offset = 0.0;
        for (index, column) in self.columns.iter().enumerate() {
            let x = if self.rtl {
                bounds.max_x() - offset - column.width
            } else {
                bounds.origin.x + offset
            };
            offset += column.width;
            let Some(mut node) = self.column_semantics(index, strings) else {
                continue;
            };
            node.actions = vec![SemanticAction::Activate];
            let rect = Rect::from_xywh(x, bounds.origin.y, column.width, bounds.size.height);
            parts.push(None, SemanticRole::ColumnHeader, "", rect).node = node;
            if index == self.focused {
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
            self.pressed = None;
        }
    }
}

/// The model of a hosted browser: read-only, or lazy with `fetch_more`.
enum BrowserModel {
    Plain(Box<dyn ItemModel>),
    Lazy(Box<dyn EditableModel>),
}

impl BrowserModel {
    fn get(&self) -> &dyn ItemModel {
        match self {
            Self::Plain(model) => model.as_ref(),
            Self::Lazy(model) => model.as_ref(),
        }
    }
}

impl ColumnBrowser {
    /// A browser over its own `model` that mounts in a
    /// [`crate::ControlHost`] as a single node named `name`.
    pub fn hosted(self, name: impl Into<String>, model: Box<dyn ItemModel>) -> HostedColumnBrowser {
        self.mount(name.into(), BrowserModel::Plain(model))
    }

    /// A hosted browser over a lazy `model`: when a column opens whose
    /// parent can fetch more children, the browser calls `fetch_more`.
    pub fn hosted_lazy(
        self,
        name: impl Into<String>,
        model: Box<dyn EditableModel>,
    ) -> HostedColumnBrowser {
        self.mount(name.into(), BrowserModel::Lazy(model))
    }

    fn mount(self, name: String, model: BrowserModel) -> HostedColumnBrowser {
        let mut hosted = HostedColumnBrowser {
            name,
            browser: self,
            model,
            column_width: 0.0,
            row_height: 1.0,
            bounds: Rect::default(),
            rtl: false,
            tracked: Parts::default(),
            offsets: Vec::new(),
            scroll_x: 0.0,
            bar: OverlayScrollBar::new(ScrollOrientation::Vertical),
            dragging: None,
            type_ahead: TypeAhead::default(),
        };
        hosted.fetch();
        hosted
    }
}

/// A [`ColumnBrowser`] with its model, mounted in a
/// [`crate::ControlHost`]. It paints one column per level and reports the
/// path as `Changed(Indices(rows))`: the row of each path item among its
/// siblings.
pub struct HostedColumnBrowser {
    name: String,
    browser: ColumnBrowser,
    model: BrowserModel,
    column_width: f64,
    row_height: f64,
    bounds: Rect,
    rtl: bool,
    /// Hover and press of the rows, by column and row.
    tracked: Parts<(usize, usize)>,
    /// The vertical scroll offset of each column, in points.
    offsets: Vec<f64>,
    /// The horizontal scroll offset of the columns, in points.
    scroll_x: f64,
    /// The scroll bar look; each column places a copy at its edge.
    bar: OverlayScrollBar,
    /// The column whose scroll bar thumb follows a drag.
    dragging: Option<(usize, OverlayScrollBar)>,
    type_ahead: TypeAhead,
}

impl HostedColumnBrowser {
    pub fn browser(&self) -> &ColumnBrowser {
        &self.browser
    }

    pub fn model(&self) -> &dyn ItemModel {
        self.model.get()
    }

    /// The row of each path item among its siblings.
    pub fn path_rows(&self) -> Vec<usize> {
        self.browser
            .path()
            .iter()
            .filter_map(|key| self.model().row(*key))
            .collect()
    }

    /// The parent that each column lists, root first.
    fn parents(&self) -> Vec<Option<ItemKey>> {
        self.browser.columns(self.model())
    }

    /// Calls `fetch_more` for each shown column of a lazy model that can
    /// fetch more children.
    fn fetch(&mut self) {
        let parents = self.parents();
        if let BrowserModel::Lazy(model) = &mut self.model {
            for parent in parents {
                if model.can_fetch_more(parent) {
                    model.fetch_more(parent);
                }
            }
        }
    }

    /// The rectangle of column `column`; columns follow reading order and
    /// scroll by the horizontal offset.
    fn column_rect(&self, column: usize) -> Rect {
        let offset = column as f64 * self.column_width - self.scroll_x;
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

    /// The column at `position`, among `count` columns.
    fn column_at(&self, position: Point, count: usize) -> Option<usize> {
        (0..count).find(|column| self.column_rect(*column).contains(position))
    }

    /// Rows that fit in a column.
    fn page(&self) -> usize {
        ((self.bounds.size.height / self.row_height).floor() as usize).max(1)
    }

    fn offset(&self, column: usize) -> f64 {
        self.offsets.get(column).copied().unwrap_or(0.0)
    }

    /// The scroll bar of `column`, placed at its edge with its offset.
    fn column_bar(&self, column: usize, parent: Option<ItemKey>) -> OverlayScrollBar {
        let mut bar = self.bar.clone();
        bar.set_viewport(self.column_rect(column));
        let content = self.model().row_count(parent) as f64 * self.row_height;
        bar.set_scroll(content, self.offset(column));
        bar
    }

    /// Keeps the path item of each column and the focused column in view,
    /// and drops the offsets of closed columns.
    fn reveal(&mut self) {
        let parents = self.parents();
        self.offsets.resize(parents.len(), 0.0);
        for (column, parent) in parents.iter().enumerate() {
            let mut bar = self.column_bar(column, *parent);
            if let Some(row) = self
                .browser
                .path()
                .get(column)
                .and_then(|key| self.model().row(*key))
            {
                let top = row as f64 * self.row_height;
                bar.reveal(top, top + self.row_height);
            }
            self.offsets[column] = bar.offset();
        }
        let width = self.bounds.size.width;
        let total = parents.len() as f64 * self.column_width;
        let start = self.browser.focused_column() as f64 * self.column_width;
        if start < self.scroll_x {
            self.scroll_x = start;
        } else if start + self.column_width > self.scroll_x + width {
            self.scroll_x = start + self.column_width - width;
        }
        self.scroll_x = self.scroll_x.clamp(0.0, (total - width).max(0.0));
    }

    /// Reports a path change, or repaints after a focus change.
    fn moved(&mut self, cx: &mut ControlContext<'_>, before: &ColumnBrowser) {
        if self.browser == *before {
            return;
        }
        self.fetch();
        self.reveal();
        if self.browser.path() != before.path() {
            cx.emit(ControlMessage::Changed(ControlValue::Indices(
                self.path_rows(),
            )));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    /// The column at `position` and the row that it shows there.
    fn entry_at(&self, position: Point) -> Option<(usize, usize)> {
        if !self.bounds.contains(position) {
            return None;
        }
        let parents = self.parents();
        let column = self.column_at(position, parents.len())?;
        let y = position.y - self.bounds.origin.y + self.offset(column);
        let row = (y / self.row_height).floor() as usize;
        (row < self.model().row_count(parents[column])).then_some((column, row))
    }

    fn press(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        let Some((column, row)) = self.entry_at(position) else {
            return EventReply::Continue;
        };
        let parent = self.parents()[column];
        if let Some(key) = self.model().key(parent, row) {
            let before = self.browser.clone();
            self.browser.select(column, key);
            self.moved(cx, &before);
            if cx.click_count() >= 2 {
                cx.emit(ControlMessage::ItemActivated(row));
            }
        }
        EventReply::Stop
    }

    /// Gives a pointer event to the scroll bar of the column under it, or
    /// to the bar whose thumb follows a drag. Returns true when a bar
    /// used it.
    fn bar_pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> bool {
        let (column, mut bar) = match self.dragging.take() {
            Some(dragging) => dragging,
            None => {
                let parents = self.parents();
                let Some(column) = self.column_at(event.position(), parents.len()) else {
                    return false;
                };
                (column, self.column_bar(column, parents[column]))
            }
        };
        let used = bar.hosted_pointer(cx, event).is_some();
        if let Some(offset) = self.offsets.get_mut(column) {
            *offset = bar.offset();
        }
        if bar.dragging() {
            self.dragging = Some((column, bar));
        }
        used
    }

    /// Selects a row in the focused column: the first for Home, the last
    /// for End, one page away for PageUp and PageDown, or the row that
    /// `typed` text (at a time in seconds) starts.
    fn move_in_column(&mut self, key: Key, typed: Option<(&str, f64)>) {
        let column = self.browser.focused_column();
        let Some(parent) = self.parents().get(column).copied() else {
            return;
        };
        let model = self.model.get();
        let count = model.row_count(parent);
        if count == 0 {
            return;
        }
        let current = self
            .browser
            .path()
            .get(column)
            .and_then(|key| model.row(*key));
        let page = self.page();
        let row = match (typed, key) {
            (Some((text, time)), _) => {
                let label = |row| {
                    model
                        .key(parent, row)
                        .map(|key| model.data(key, 0, Role::Display).to_display_string())
                        .unwrap_or_default()
                };
                self.type_ahead.find(text, time, current, count, label)
            }
            (None, Key::Home) => Some(0),
            (None, Key::End) => Some(count - 1),
            (None, Key::PageDown) => Some(current.map_or(0, |row| (row + page).min(count - 1))),
            (None, Key::PageUp) => Some(current.map_or(0, |row| row.saturating_sub(page))),
            _ => None,
        };
        if let Some(key) = row.and_then(|row| model.key(parent, row)) {
            self.browser.select(column, key);
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let before = self.browser.clone();
        // Columns follow reading order, so the keys mirror too.
        let key = key.in_reading_order(self.rtl);
        match key {
            Key::Enter => {
                let column = self.browser.focused_column();
                let Some(row) = self.path_rows().get(column).copied() else {
                    return EventReply::Continue;
                };
                cx.emit(ControlMessage::ItemActivated(row));
                return EventReply::Stop;
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                self.browser.handle_key(self.model.get(), key);
            }
            Key::Home | Key::End | Key::PageUp | Key::PageDown => self.move_in_column(key, None),
            _ => return EventReply::Continue,
        }
        self.moved(cx, &before);
        EventReply::Stop
    }

    /// Scrolls the column under `position` by a wheel event, and the
    /// columns sideways by its horizontal part. At the limits the event
    /// goes on to an outer scroll view.
    fn wheel(
        &mut self,
        cx: &mut ControlContext<'_>,
        position: Point,
        delta: Point,
        unit: WheelUnit,
    ) -> EventReply {
        let parents = self.parents();
        let Some(column) = self.column_at(position, parents.len()) else {
            return EventReply::Continue;
        };
        let line = self.row_height;
        let mut bar = self.column_bar(column, parents[column]);
        let mut reply = bar.wheel(cx, delta, unit, line);
        self.offsets[column] = bar.offset();
        let scale = if unit == WheelUnit::Lines { line } else { 1.0 };
        let total = parents.len() as f64 * self.column_width;
        let limit = (total - self.bounds.size.width).max(0.0);
        let x = (self.scroll_x + delta.x * scale).clamp(0.0, limit);
        if x != self.scroll_x {
            self.scroll_x = x;
            cx.request_paint();
            reply = EventReply::Stop;
        }
        if reply == EventReply::Stop {
            // Other rows move under the pointer.
            self.tracked.hovered = None;
        }
        reply
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
        self.bar.arrange(cx, bounds);
        self.reveal();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() == EventPhase::Target
            && let ControlEvent::Pointer(pointer) = event
            && self.bar_pointer(cx, pointer)
        {
            return EventReply::Stop;
        }
        let mut tracked = self.tracked;
        tracked.track(cx, event, |point| self.entry_at(point));
        self.tracked = tracked;
        if cx.phase() == EventPhase::Target
            && let ControlEvent::Text(text) = event
        {
            let before = self.browser.clone();
            self.move_in_column(Key::Other(0), Some((text, cx.time())));
            self.moved(cx, &before);
            return EventReply::Stop;
        }
        let Some(routed) = crate::hosted::route(cx, event) else {
            return EventReply::Continue;
        };
        match routed {
            Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => self.press(cx, position),
            Routed::Pointer(PointerEvent::Wheel {
                position,
                delta,
                unit,
            }) => self.wheel(cx, position, delta, unit),
            Routed::Key(KeyEvent::Down { key, .. }) => self.key(cx, key),
            Routed::Cancel => {
                if let Some((_, mut bar)) = self.dragging.take() {
                    bar.release();
                    cx.capture_pointer(false);
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints only the rows in view in each column, a divider at the end
    /// of each column, and the scroll bar of each column that overflows.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let focused = host.focused();
        let page = self.page();
        let model = self.model();
        let divider = cx.style(StyleQuery::new(ControlKind::Separator, Slot::Root));
        cx.list.push_clip(self.bounds);
        for (column, parent) in self.parents().iter().enumerate() {
            let rect = self.column_rect(column);
            if rect.max_x() < self.bounds.origin.x || rect.origin.x > self.bounds.max_x() {
                continue;
            }
            let offset = self.offset(column);
            let first = (offset / self.row_height).floor() as usize;
            let end = model.row_count(*parent).min(first + page + 2);
            let selected = self.browser.path().get(column).copied();
            let active = focused && column == self.browser.focused_column();
            cx.list.push_clip(rect);
            for row in first..end {
                let Some(key) = model.key(*parent, row) else {
                    continue;
                };
                let entry = Rect::from_xywh(
                    rect.origin.x,
                    rect.origin.y + row as f64 * self.row_height - offset,
                    rect.size.width,
                    self.row_height,
                );
                let label = model.data(key, 0, Role::Display).to_display_string();
                let has_children = model.has_children(key) || model.can_fetch_more(Some(key));
                ColumnBrowser::paint_row(
                    &mut cx.at(entry),
                    &label,
                    has_children,
                    selected == Some(key),
                    active,
                    self.tracked.state((column, row), host),
                );
            }
            let _ = cx.list.pop_clip();
            // The divider at the end edge of the column.
            let width = divider.min_width;
            let x = if self.rtl {
                rect.origin.x
            } else {
                rect.max_x() - width
            };
            fill_area(
                cx.list,
                Rect::from_xywh(x, rect.origin.y, width, rect.size.height),
                cx.rtl,
                &divider.background,
            );
            match &self.dragging {
                Some((dragged, bar)) if *dragged == column => bar.paint(cx),
                _ => self.column_bar(column, *parent).paint(cx),
            }
        }
        // Keyboard focus rings the focused column.
        let column = self.column_rect(self.browser.focused_column());
        let radius = cx
            .style(StyleQuery::new(ControlKind::ListView, Slot::Root))
            .radius
            .top_start;
        cx.focus_ring(column, radius);
        let _ = cx.list.pop_clip();
    }

    fn semantics(&self) -> SemanticNode {
        crate::hosted::node(SemanticRole::Tree, &self.name, vec![SemanticAction::Focus])
    }

    /// A `List` part for each column, named by the item that it lists,
    /// with its row count as the collection. Each row in view is a
    /// `ListItem` part with its row index, selected on the path. The path
    /// item of the focused column is the focused part.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        let model = self.model();
        let page = self.page();
        let label = |key: ItemKey| model.data(key, 0, Role::Display).to_display_string();
        let path = self.browser.path();
        for (column, parent) in self.parents().iter().enumerate() {
            let rect = self.column_rect(column);
            let name = parent.map_or_else(|| self.name.clone(), label);
            let list = parts.push(None, SemanticRole::List, &name, rect);
            let count = model.row_count(*parent);
            list.collection = Some(SemanticCollection {
                item_count: count,
                column_count: None,
            });
            let list = Some(parts.len() - 1);
            let offset = self.offset(column);
            let first = (offset / self.row_height).floor() as usize;
            for row in first..count.min(first + page + 2) {
                let Some(key) = model.key(*parent, row) else {
                    continue;
                };
                let y = rect.origin.y + row as f64 * self.row_height - offset;
                let entry = Rect::from_xywh(rect.origin.x, y, rect.size.width, self.row_height);
                let item = crate::hosted::pressable(
                    parts,
                    list,
                    SemanticRole::ListItem,
                    &label(key),
                    entry,
                    true,
                );
                let selected = path.get(column) == Some(&key);
                item.node.state.selected = Some(selected);
                if model.has_children(key) || model.can_fetch_more(Some(key)) {
                    item.node.state.expanded = Some(path.get(column + 1).is_some() && selected);
                }
                // Each column numbers its rows from 0, so the column also
                // goes in the key.
                item.item(row).key += (column as u64) << 32;
                if selected && column == self.browser.focused_column() {
                    parts.set_focused(Some(parts.len() - 1));
                }
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

    /// Takes `Indices`: the row of each path item among its siblings.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(rows) = value else {
            return false;
        };
        let mut browser = ColumnBrowser::new();
        let mut parent = None;
        for (column, row) in rows.iter().enumerate() {
            let Some(key) = self.model().key(parent, *row) else {
                return false;
            };
            browser.select(column, key);
            parent = Some(key);
        }
        self.browser = browser;
        self.fetch();
        self.reveal();
        true
    }
}
