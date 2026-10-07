//! Sectioned date/time editors and a month calendar.
//!
//! The editor never holds an invalid date: each section edits one field,
//! Up/Down step it (wrapping within its field, days clamped to the month),
//! digits type into it, and Left/Right or the locale separator move between
//! sections. Values outside the minimum/maximum are reported as
//! `Intermediate` while typing and clamped on commit.

use crate::ValidationState;
use choscorui_core::{
    CivilDate, CivilDateTime, CivilTime, DateOrder, Key, Locale, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, days_in_month, month_grid, month_name, weekday_short_name,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateTimeField {
    Day,
    Month,
    Year,
    Hour,
    Minute,
    Second,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateTimeKind {
    Date,
    Time,
    DateTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DateTimeEdit {
    name: String,
    locale: Locale,
    kind: DateTimeKind,
    value: CivilDateTime,
    minimum: Option<CivilDateTime>,
    maximum: Option<CivilDateTime>,
    fields: Vec<DateTimeField>,
    focused: usize,
    typed: String,
}

impl DateTimeEdit {
    pub fn new(
        name: impl Into<String>,
        locale: Locale,
        kind: DateTimeKind,
        value: CivilDateTime,
    ) -> Self {
        let mut edit = Self {
            name: name.into(),
            locale,
            kind,
            value,
            minimum: None,
            maximum: None,
            fields: Vec::new(),
            focused: 0,
            typed: String::new(),
        };
        edit.fields = edit.field_order();
        edit
    }

    fn field_order(&self) -> Vec<DateTimeField> {
        let date = match self.locale.data().date_order {
            DateOrder::DayMonthYear => [
                DateTimeField::Day,
                DateTimeField::Month,
                DateTimeField::Year,
            ],
            DateOrder::MonthDayYear => [
                DateTimeField::Month,
                DateTimeField::Day,
                DateTimeField::Year,
            ],
            DateOrder::YearMonthDay => [
                DateTimeField::Year,
                DateTimeField::Month,
                DateTimeField::Day,
            ],
        };
        let time = [DateTimeField::Hour, DateTimeField::Minute];
        match self.kind {
            DateTimeKind::Date => date.to_vec(),
            DateTimeKind::Time => time.to_vec(),
            DateTimeKind::DateTime => date.into_iter().chain(time).collect(),
        }
    }

    pub fn set_range(&mut self, minimum: Option<CivilDateTime>, maximum: Option<CivilDateTime>) {
        self.minimum = minimum;
        self.maximum = maximum;
        self.value = self.clamped(self.value);
    }
    pub fn value(&self) -> CivilDateTime {
        self.value
    }
    pub fn fields(&self) -> &[DateTimeField] {
        &self.fields
    }
    pub fn focused_field(&self) -> DateTimeField {
        self.fields[self.focused]
    }
    pub fn set_locale(&mut self, locale: Locale) {
        let field = self.focused_field();
        self.locale = locale;
        self.fields = self.field_order();
        self.focused = self.fields.iter().position(|f| *f == field).unwrap_or(0);
    }

    fn clamped(&self, value: CivilDateTime) -> CivilDateTime {
        let mut value = value;
        if let Some(minimum) = self.minimum {
            value = value.max(minimum);
        }
        if let Some(maximum) = self.maximum {
            value = value.min(maximum);
        }
        value
    }

    pub fn validation(&self) -> ValidationState {
        let below = self.minimum.is_some_and(|minimum| self.value < minimum);
        let above = self.maximum.is_some_and(|maximum| self.value > maximum);
        if below || above {
            ValidationState::Intermediate
        } else {
            ValidationState::Valid
        }
    }

    fn field_value(&self, field: DateTimeField) -> i64 {
        let date = self.value.date;
        let time = self.value.time;
        match field {
            DateTimeField::Day => i64::from(date.day()),
            DateTimeField::Month => i64::from(date.month()),
            DateTimeField::Year => i64::from(date.year()),
            DateTimeField::Hour => i64::from(time.hour()),
            DateTimeField::Minute => i64::from(time.minute()),
            DateTimeField::Second => i64::from(time.second()),
        }
    }

    fn with_field(&self, field: DateTimeField, value: i64) -> Option<CivilDateTime> {
        let date = self.value.date;
        let time = self.value.time;
        let clamp_day = |year: i32, month: u8, day: u8| day.min(days_in_month(year, month));
        let result = match field {
            DateTimeField::Day => {
                let day = value.clamp(1, i64::from(days_in_month(date.year(), date.month())));
                CivilDateTime {
                    date: CivilDate::new(date.year(), date.month(), day as u8).ok()?,
                    time,
                }
            }
            DateTimeField::Month => {
                let month = value.clamp(1, 12) as u8;
                CivilDateTime {
                    date: CivilDate::new(
                        date.year(),
                        month,
                        clamp_day(date.year(), month, date.day()),
                    )
                    .ok()?,
                    time,
                }
            }
            DateTimeField::Year => {
                let year = value.clamp(1, 9999) as i32;
                CivilDateTime {
                    date: CivilDate::new(
                        year,
                        date.month(),
                        clamp_day(year, date.month(), date.day()),
                    )
                    .ok()?,
                    time,
                }
            }
            DateTimeField::Hour => CivilDateTime {
                date,
                time: CivilTime::new(value.clamp(0, 23) as u8, time.minute(), time.second())
                    .ok()?,
            },
            DateTimeField::Minute => CivilDateTime {
                date,
                time: CivilTime::new(time.hour(), value.clamp(0, 59) as u8, time.second()).ok()?,
            },
            DateTimeField::Second => CivilDateTime {
                date,
                time: CivilTime::new(time.hour(), time.minute(), value.clamp(0, 59) as u8).ok()?,
            },
        };
        Some(result)
    }

    fn field_bounds(&self, field: DateTimeField) -> (i64, i64) {
        match field {
            DateTimeField::Day => (
                1,
                i64::from(days_in_month(
                    self.value.date.year(),
                    self.value.date.month(),
                )),
            ),
            DateTimeField::Month => (1, 12),
            DateTimeField::Year => (1, 9999),
            DateTimeField::Hour => (0, 23),
            DateTimeField::Minute | DateTimeField::Second => (0, 59),
        }
    }

    fn step(&mut self, delta: i64) -> bool {
        let field = self.focused_field();
        let (low, high) = self.field_bounds(field);
        let span = high - low + 1;
        let current = self.field_value(field);
        let next = if field == DateTimeField::Year {
            (current + delta).clamp(low, high)
        } else {
            low + (current - low + delta).rem_euclid(span)
        };
        self.typed.clear();
        self.set(field, next)
    }

    fn set(&mut self, field: DateTimeField, value: i64) -> bool {
        match self.with_field(field, value) {
            Some(next) if next != self.value => {
                self.value = next;
                true
            }
            _ => false,
        }
    }

    fn type_digit(&mut self, digit: char) -> bool {
        let field = self.focused_field();
        let width = if field == DateTimeField::Year { 4 } else { 2 };
        self.typed.push(digit);
        let typed: i64 = self.typed.parse().unwrap_or(0);
        let (_, high) = self.field_bounds(field);
        let changed = if field == DateTimeField::Year && self.typed.len() < width {
            false
        } else {
            self.set(
                field,
                typed.max(if field == DateTimeField::Year { 1 } else { 0 }),
            )
        };
        // Advance when the section is full or no further digit can fit.
        if self.typed.len() >= width || typed * 10 > high {
            self.typed.clear();
            if self.focused + 1 < self.fields.len() {
                self.focused += 1;
            }
        }
        changed
    }

    /// Returns true when the value changed.
    pub fn handle_key(&mut self, key: Key) -> bool {
        match key {
            Key::ArrowUp => self.step(1),
            Key::ArrowDown => self.step(-1),
            Key::PageUp => self.step(10),
            Key::PageDown => self.step(-10),
            Key::ArrowLeft => {
                self.typed.clear();
                self.focused = self.focused.saturating_sub(1);
                false
            }
            Key::ArrowRight => {
                self.typed.clear();
                self.focused = (self.focused + 1).min(self.fields.len() - 1);
                false
            }
            Key::Home => {
                let field = self.focused_field();
                let low = self.field_bounds(field).0;
                self.set(field, low)
            }
            Key::End => {
                let field = self.focused_field();
                let high = self.field_bounds(field).1;
                self.set(field, high)
            }
            Key::Character(character) if character.is_ascii_digit() => self.type_digit(character),
            Key::Character(character)
                if character == self.locale.data().date_separator
                    || matches!(character, '/' | '.' | '-' | ':' | ' ') =>
            {
                self.typed.clear();
                self.focused = (self.focused + 1).min(self.fields.len() - 1);
                false
            }
            Key::Enter => self.commit(),
            _ => false,
        }
    }

    /// Clamps an out-of-range value into the allowed range.
    pub fn commit(&mut self) -> bool {
        self.typed.clear();
        let clamped = self.clamped(self.value);
        let changed = clamped != self.value;
        self.value = clamped;
        changed
    }

    pub fn text(&self) -> String {
        let date = self.value.date.format_short(&self.locale);
        let time = self.value.time.format_short(&self.locale);
        match self.kind {
            DateTimeKind::Date => date,
            DateTimeKind::Time => time,
            DateTimeKind::DateTime => format!("{date} {time}"),
        }
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::SpinButton,
            name: self.name.clone(),
            description: format!("{:?} field", self.focused_field()),
            value: Some(match self.kind {
                DateTimeKind::Time => self.text(),
                _ => {
                    let long = self.value.date.format_long(&self.locale);
                    if self.kind == DateTimeKind::DateTime {
                        format!("{long} {}", self.value.time.format_short(&self.locale))
                    } else {
                        long
                    }
                }
            }),
            state: SemanticState {
                invalid: self.validation() != ValidationState::Valid,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
            text: None,
        }
    }
}

/// Month calendar: arrows move by day and week, Page Up/Down by month,
/// Home/End to the start/end of the week (locale first weekday). Choosing
/// a date (for example on Return) is left to the host. Dates outside the
/// range are visible but not selectable.
#[derive(Clone, Debug, PartialEq)]
pub struct Calendar {
    locale: Locale,
    selected: CivilDate,
    shown: (i32, u8),
    minimum: Option<CivilDate>,
    maximum: Option<CivilDate>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarCell {
    pub date: CivilDate,
    pub in_month: bool,
    pub selected: bool,
    pub enabled: bool,
}

impl Calendar {
    pub fn new(locale: Locale, selected: CivilDate) -> Self {
        Self {
            locale,
            selected,
            shown: (selected.year(), selected.month()),
            minimum: None,
            maximum: None,
        }
    }
    pub fn set_range(&mut self, minimum: Option<CivilDate>, maximum: Option<CivilDate>) {
        self.minimum = minimum;
        self.maximum = maximum;
    }
    pub fn selected(&self) -> CivilDate {
        self.selected
    }
    pub fn shown_month(&self) -> (i32, u8) {
        self.shown
    }
    fn allowed(&self, date: CivilDate) -> bool {
        self.minimum.is_none_or(|minimum| date >= minimum)
            && self.maximum.is_none_or(|maximum| date <= maximum)
    }

    /// Selects `date` if allowed, showing its month.
    pub fn select(&mut self, date: CivilDate) -> bool {
        if !self.allowed(date) || date == self.selected {
            return false;
        }
        self.selected = date;
        self.shown = (date.year(), date.month());
        true
    }

    /// Six weeks starting on the locale's first weekday.
    pub fn cells(&self) -> Vec<CalendarCell> {
        let Ok(grid) = month_grid(self.shown.0, self.shown.1, &self.locale) else {
            return Vec::new();
        };
        grid.iter()
            .map(|date| CalendarCell {
                date: *date,
                in_month: date.month() == self.shown.1,
                selected: *date == self.selected,
                enabled: self.allowed(*date),
            })
            .collect()
    }

    pub fn title(&self) -> String {
        format!(
            "{} {}",
            month_name(&self.locale, self.shown.1),
            self.shown.0
        )
    }

    pub fn weekday_headers(&self) -> Vec<&'static str> {
        self.cells()
            .iter()
            .take(7)
            .map(|cell| weekday_short_name(&self.locale, cell.date.weekday()))
            .collect()
    }

    /// Returns true when the selection changed.
    pub fn handle_key(&mut self, key: Key) -> bool {
        let target = match key {
            Key::ArrowLeft => self.selected.add_days(-1),
            Key::ArrowRight => self.selected.add_days(1),
            Key::ArrowUp => self.selected.add_days(-7),
            Key::ArrowDown => self.selected.add_days(7),
            Key::PageUp => self.selected.add_months(-1),
            Key::PageDown => self.selected.add_months(1),
            Key::Home | Key::End => {
                let first = i64::from(self.locale.data().first_weekday);
                let weekday = i64::from(self.selected.weekday().0);
                let back = (weekday - first).rem_euclid(7);
                if key == Key::Home {
                    self.selected.add_days(-back)
                } else {
                    self.selected.add_days(6 - back)
                }
            }
            _ => return false,
        };
        match target {
            Ok(date) => self.select(date),
            Err(_) => false,
        }
    }

    pub fn show_month(&mut self, delta: i32) {
        let total = self.shown.0 * 12 + i32::from(self.shown.1) - 1 + delta;
        self.shown = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u8);
    }

    pub fn semantics(&self, name: &str) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Table,
            name: name.to_owned(),
            description: self.title(),
            value: Some(self.selected.format_long(&self.locale)),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Focus],
            text: None,
        }
    }

    pub fn cell_semantics(&self, cell: &CalendarCell) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Cell,
            name: cell.date.format_long(&self.locale),
            description: String::new(),
            value: Some(cell.date.day().to_string()),
            state: SemanticState {
                selected: Some(cell.selected),
                disabled: !cell.enabled,
                hidden: false,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if cell.enabled {
                vec![SemanticAction::Activate]
            } else {
                Vec::new()
            },
            text: None,
        }
    }
}
