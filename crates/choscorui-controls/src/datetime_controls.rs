//! Sectioned date/time editors and a month calendar.
//!
//! The editor never holds an invalid date: each section edits one field,
//! Up/Down step it (wrapping within its field, days clamped to the month),
//! digits type into it, and Left/Right or the locale separator move between
//! sections. Steps stop at the minimum/maximum. A typed value outside
//! them is reported as `Intermediate` and clamped on commit.

use crate::calendar_caption::{CaptionContext, CaptionDropdowns, CaptionPart, CaptionReply};
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::editing::{ValueEdit, value_edit};
use crate::hosted::Parts;
use crate::input_text::InputText;
use crate::range_controls::paint_stepper_box;
use crate::style::{
    CalendarDay, ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState,
    content_rect,
};
use crate::{STEPPER_WIDTH, ValidationState};
use choscorui_core::{
    CivilDate, CivilDateTime, CivilTime, DateOrder, DesignTokens, EventPhase, EventReply, Icon,
    Key, KeyEvent, LayoutDirection, Locale, Localizer, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
    days_in_month, month_grid, month_name, weekday_short_name,
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateTimeField {
    Day,
    Month,
    Year,
    Hour,
    Minute,
    Second,
    /// AM or PM, the last time section in a 12-hour locale.
    Meridiem,
}

impl DateTimeField {
    /// The built-in message id of the section's accessible description.
    fn message_id(self) -> &'static str {
        match self {
            Self::Day => "choscorui.field.day",
            Self::Month => "choscorui.field.month",
            Self::Year => "choscorui.field.year",
            Self::Hour => "choscorui.field.hour",
            Self::Minute => "choscorui.field.minute",
            Self::Second => "choscorui.field.second",
            Self::Meridiem => "choscorui.field.meridiem",
        }
    }
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
    /// The digits typed into the focused section so far. An empty text
    /// is a section that Backspace cleared. `None` shows the value.
    typed: Option<String>,
    /// The value before the first change since focus or the last commit,
    /// which Escape restores in a host.
    before: Option<CivilDateTime>,
    /// The layout direction of a hosted editor, from `arrange`.
    rtl: bool,
    /// The text box relative to the bounds origin, and the end offset of
    /// each section in it, from `arrange`. A click picks a section by them.
    text_box: Rect,
    edges: Vec<f64>,
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
            typed: None,
            before: None,
            rtl: false,
            text_box: Rect::default(),
            edges: Vec::new(),
        };
        edit.fields = edit.field_order();
        edit
    }

    /// True when the locale shows the hour from 1 to 12 with AM or PM.
    fn hour12(&self) -> bool {
        !self.locale.data().hour_cycle_24
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
        let mut time = vec![DateTimeField::Hour, DateTimeField::Minute];
        if self.hour12() {
            time.push(DateTimeField::Meridiem);
        }
        match self.kind {
            DateTimeKind::Date => date.to_vec(),
            DateTimeKind::Time => time,
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
        self.typed = None;
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
        if self.clamped(self.value) != self.value {
            ValidationState::Intermediate
        } else {
            ValidationState::Valid
        }
    }

    /// The number a section shows: the hour from 1 to 12 in a 12-hour
    /// locale, and 0 (AM) or 1 (PM) for the meridiem.
    fn field_value(&self, field: DateTimeField) -> i64 {
        let date = self.value.date;
        let hour = i64::from(self.value.time.hour());
        match field {
            DateTimeField::Day => i64::from(date.day()),
            DateTimeField::Month => i64::from(date.month()),
            DateTimeField::Year => i64::from(date.year()),
            DateTimeField::Hour if self.hour12() => (hour + 11) % 12 + 1,
            DateTimeField::Hour => hour,
            DateTimeField::Minute => i64::from(self.value.time.minute()),
            DateTimeField::Second => i64::from(self.value.time.second()),
            DateTimeField::Meridiem => hour / 12,
        }
    }

    /// The value with `field` set to `value` (clamped to the field), and
    /// the day clamped to the month.
    fn with_field(&self, field: DateTimeField, value: i64) -> Option<CivilDateTime> {
        let date = self.value.date;
        let time = self.value.time;
        let (low, high) = self.field_bounds(field);
        let value = value.clamp(low, high);
        let (mut year, mut month, mut day) = (date.year(), date.month(), date.day());
        let (mut hour, mut minute, mut second) = (time.hour(), time.minute(), time.second());
        let small = value as u8;
        match field {
            DateTimeField::Day => day = small,
            DateTimeField::Month => month = small,
            DateTimeField::Year => year = value as i32,
            DateTimeField::Hour if self.hour12() => hour = small % 12 + hour / 12 * 12,
            DateTimeField::Hour => hour = small,
            DateTimeField::Minute => minute = small,
            DateTimeField::Second => second = small,
            DateTimeField::Meridiem => hour = hour % 12 + small * 12,
        }
        Some(CivilDateTime {
            date: CivilDate::new(year, month, day.min(days_in_month(year, month))).ok()?,
            time: CivilTime::new(hour, minute, second).ok()?,
        })
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
            DateTimeField::Hour if self.hour12() => (1, 12),
            DateTimeField::Hour => (0, 23),
            DateTimeField::Minute | DateTimeField::Second => (0, 59),
            DateTimeField::Meridiem => (0, 1),
        }
    }

    /// Steps the focused section. It wraps inside its field, the year
    /// clamps, and the value stops at the minimum and the maximum.
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
        self.typed = None;
        self.set(field, next, true)
    }

    /// Sets `field` to `value`, clamped into the range when `limit`.
    fn set(&mut self, field: DateTimeField, value: i64, limit: bool) -> bool {
        let Some(next) = self.with_field(field, value) else {
            return false;
        };
        let next = if limit { self.clamped(next) } else { next };
        let changed = next != self.value;
        self.value = next;
        changed
    }

    /// Applies the typed digits of the focused section, then focuses
    /// section `index`.
    fn move_to(&mut self, index: usize) -> bool {
        let changed = self.apply_typed();
        self.focused = index.min(self.fields.len() - 1);
        changed
    }

    /// Applies the pending digits, so a year with fewer than four digits
    /// takes effect too. A cleared section keeps its value.
    fn apply_typed(&mut self) -> bool {
        let field = self.focused_field();
        match self
            .typed
            .take()
            .and_then(|typed| typed.parse::<i64>().ok())
        {
            Some(number) if number >= self.field_bounds(field).0 => self.set(field, number, false),
            _ => false,
        }
    }

    fn type_digit(&mut self, digit: char) -> bool {
        let field = self.focused_field();
        if field == DateTimeField::Meridiem {
            return false;
        }
        let width = if field == DateTimeField::Year { 4 } else { 2 };
        let typed = self.typed.get_or_insert_with(String::new);
        typed.push(digit);
        let length = typed.len();
        let number: i64 = typed.parse().unwrap_or(0);
        let (low, high) = self.field_bounds(field);
        // A number below the lowest value of the field waits for a digit.
        let changed = if (field == DateTimeField::Year && length < width) || number < low {
            false
        } else {
            self.set(field, number, false)
        };
        // Advance when the section is full or no further digit can fit.
        if length >= width || number * 10 > high {
            self.typed = None;
            if self.focused + 1 < self.fields.len() {
                self.focused += 1;
            }
        }
        changed
    }

    /// Backspace removes the last typed digit, or clears the section.
    fn backspace(&mut self) {
        match &mut self.typed {
            Some(typed) if !typed.is_empty() => {
                typed.pop();
            }
            _ => self.typed = Some(String::new()),
        }
    }

    /// Returns true when the value changed.
    pub fn handle_key(&mut self, key: Key) -> bool {
        let field = self.focused_field();
        match key {
            Key::ArrowUp => self.step(1),
            Key::ArrowDown => self.step(-1),
            Key::PageUp => self.step(10),
            Key::PageDown => self.step(-10),
            Key::ArrowLeft => self.move_to(self.focused.saturating_sub(1)),
            Key::ArrowRight => self.move_to(self.focused + 1),
            Key::Home | Key::End => {
                self.typed = None;
                let (low, high) = self.field_bounds(field);
                self.set(field, if key == Key::Home { low } else { high }, true)
            }
            Key::Backspace | Key::Delete => {
                self.backspace();
                false
            }
            Key::Character(character) if character.is_ascii_digit() => self.type_digit(character),
            Key::Character(character @ ('a' | 'A' | 'p' | 'P'))
                if field == DateTimeField::Meridiem =>
            {
                self.typed = None;
                let pm = character.eq_ignore_ascii_case(&'p');
                self.set(field, i64::from(pm), false)
            }
            Key::Character(character)
                if character == self.locale.data().date_separator
                    || matches!(character, '/' | '.' | '-' | ':' | ' ') =>
            {
                self.move_to(self.focused + 1)
            }
            Key::Enter => self.commit(),
            _ => false,
        }
    }

    /// Applies the typed digits and clamps an out-of-range value into the
    /// allowed range.
    pub fn commit(&mut self) -> bool {
        let typed = self.apply_typed();
        let clamped = self.clamped(self.value);
        let changed = typed || clamped != self.value;
        self.value = clamped;
        changed
    }

    /// The text of the value and the character range of each section.
    /// With `pending`, the focused section shows its typed digits, and a
    /// cleared section shows dashes.
    fn sections(&self, pending: bool) -> (String, Vec<Range<usize>>) {
        let separator = self.locale.data().date_separator;
        let mut text = String::new();
        let mut ranges = Vec::with_capacity(self.fields.len());
        for (index, field) in self.fields.iter().enumerate() {
            if index > 0 {
                text.push(match field {
                    DateTimeField::Day | DateTimeField::Month | DateTimeField::Year => separator,
                    DateTimeField::Minute | DateTimeField::Second => ':',
                    DateTimeField::Hour | DateTimeField::Meridiem => ' ',
                });
            }
            let start = text.chars().count();
            let value = self.field_value(*field);
            let typed = (pending && index == self.focused)
                .then_some(self.typed.as_deref())
                .flatten();
            match (typed, field) {
                (Some(""), DateTimeField::Year) => text.push_str("----"),
                (Some(""), _) => text.push_str("--"),
                (Some(digits), _) => text.push_str(digits),
                (None, DateTimeField::Year) => text.push_str(&format!("{value:04}")),
                (None, DateTimeField::Meridiem) => {
                    text.push_str(if value == 0 { "AM" } else { "PM" });
                }
                (None, DateTimeField::Hour) if self.hour12() => text.push_str(&value.to_string()),
                (None, _) => text.push_str(&format!("{value:02}")),
            }
            ranges.push(start..text.chars().count());
        }
        (text, ranges)
    }

    pub fn text(&self) -> String {
        self.sections(false).0
    }

    /// The flags the input recipe reads: the host's focus flags in
    /// `host`, and invalid while the value is out of range.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(
            VisualState::INVALID,
            host.invalid() || self.validation() != ValidationState::Valid,
        )
    }

    /// Paints the `Input` box with the text and the steppers, like
    /// [`crate::SpinBox::paint`]. `host` carries focus flags. While
    /// focused, the focused section shows the selection color and its
    /// typed digits.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let steppers = Self::steppers(cx.bounds, cx.rtl);
        let state = self.visual_state(host);
        let (text, ranges) = self.sections(true);
        let selection = ranges[self.focused].clone();
        let arrow = VisualState::NONE.set(VisualState::DISABLED, state.disabled());
        paint_stepper_box(cx, state, steppers, [arrow; 2], |cx, rect| {
            let line = InputText {
                text: &text,
                selection,
                ..InputText::default()
            };
            line.paint_text(cx, rect, state);
        });
    }

    /// The semantics, with the focused field in the language of
    /// `strings`.
    pub fn semantics(&self, strings: &Localizer) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::SpinButton,
            name: self.name.clone(),
            description: strings.message(self.focused_field().message_id(), &[]),
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
    mode: CalendarMode,
    /// The chosen days of a `Multiple` calendar, in order, or the start
    /// and end of a `Range` calendar.
    chosen: Vec<CivilDate>,
    /// The accessible name of a hosted calendar.
    name: String,
    /// The day a hosted calendar marks as today.
    today: Option<CivilDate>,
    /// The parts of a hosted calendar, from `arrange`.
    layout: Option<CalendarLayout>,
    /// The month and year dropdowns that replace the caption text.
    caption: Option<CaptionDropdowns>,
    /// The part of a hosted calendar that has keyboard focus.
    focus: CalendarFocus,
    /// The hovered and pressed month buttons and days.
    pub(crate) parts: Parts<CalendarPart>,
    /// Whether the day grid runs right to left.
    rtl: bool,
}

/// A month button, or a day cell by index, of a calendar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CalendarPart {
    Previous,
    Next,
    Day(usize),
}

/// How a calendar chooses days.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CalendarMode {
    /// One day: the selected day.
    #[default]
    Single,
    /// Any set of days. Activation adds or removes a day.
    Multiple,
    /// A start and an end day. Activation sets the start, then the end.
    Range,
}

/// Where [`Calendar::layout`] places the caption, the month buttons, the
/// weekday names, and the 42 day cells.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarLayout {
    pub caption: Rect,
    pub previous: Rect,
    pub next: Rect,
    pub weekdays: Vec<Rect>,
    pub days: Vec<Rect>,
}

/// The calendar part that shows a keyboard focus ring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CalendarFocus {
    #[default]
    None,
    Previous,
    Next,
    /// The day grid; the ring is on the selected day.
    Grid,
    /// The month dropdown of a dropdown caption.
    Month,
    /// The year dropdown of a dropdown caption.
    Year,
}

impl CalendarFocus {
    fn caption_part(self) -> Option<CaptionPart> {
        match self {
            Self::Month => Some(CaptionPart::Month),
            Self::Year => Some(CaptionPart::Year),
            _ => None,
        }
    }

    pub(crate) fn from_part(part: CaptionPart) -> Self {
        match part {
            CaptionPart::Month => Self::Month,
            CaptionPart::Year => Self::Year,
        }
    }
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
            mode: CalendarMode::Single,
            chosen: Vec::new(),
            name: String::new(),
            today: None,
            layout: None,
            parts: Parts::default(),
            caption: None,
            focus: CalendarFocus::None,
            rtl: false,
        }
    }

    /// Swaps ArrowLeft and ArrowRight for a right-to-left day grid. A
    /// hosted calendar takes the direction from `arrange`.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.rtl = direction == LayoutDirection::Rtl;
    }

    /// Shows the month and the year as dropdowns in the caption, in
    /// place of the caption text. The year list runs from `first_year`
    /// to `last_year`, in either order, inside 1..=9999.
    #[must_use]
    pub fn with_dropdown_caption(mut self, first_year: i32, last_year: i32) -> Self {
        self.caption = Some(CaptionDropdowns::new(&self.locale, first_year, last_year));
        self
    }

    /// True when the caption shows month and year dropdowns.
    pub fn has_dropdown_caption(&self) -> bool {
        self.caption.is_some()
    }

    /// Shows `month` of `year`, as a choice in a caption dropdown does.
    /// It keeps the selection.
    pub fn show(&mut self, year: i32, month: u8) {
        self.shown = (year, month.clamp(1, 12));
    }

    /// Chooses days as `mode` says. In `Multiple` and `Range` modes the
    /// selected day is the keyboard cursor, and activation chooses.
    #[must_use]
    pub fn with_mode(mut self, mode: CalendarMode) -> Self {
        self.mode = mode;
        self.chosen.clear();
        self
    }

    pub fn mode(&self) -> CalendarMode {
        self.mode
    }

    /// The chosen days: the selected day in `Single` mode, the set in
    /// `Multiple` mode, and the start and end (when set) in `Range` mode.
    pub fn dates(&self) -> Vec<CivilDate> {
        match self.mode {
            CalendarMode::Single => vec![self.selected],
            _ => self.chosen.clone(),
        }
    }

    /// Activates `date` as a click or Enter would. In `Single` mode it
    /// selects the day. In `Multiple` mode it adds or removes it. In
    /// `Range` mode it sets the start, or the end after the start; an
    /// earlier day or a third activation starts again. Returns true when
    /// the chosen days changed.
    pub fn activate(&mut self, date: CivilDate) -> bool {
        if !self.allowed(date) {
            return false;
        }
        match self.mode {
            CalendarMode::Single => return self.select(date),
            CalendarMode::Multiple => match self.chosen.iter().position(|day| *day == date) {
                Some(index) => {
                    self.chosen.remove(index);
                }
                None => {
                    self.chosen.push(date);
                    self.chosen.sort();
                }
            },
            CalendarMode::Range => match self.chosen.as_slice() {
                [start] if date >= *start => self.chosen.push(date),
                _ => self.chosen = vec![date],
            },
        }
        self.selected = date;
        self.shown = (date.year(), date.month());
        true
    }

    /// Replaces the chosen days of a `Multiple` or `Range` calendar.
    pub fn set_dates(&mut self, dates: &[CivilDate]) {
        if self.mode == CalendarMode::Single {
            if let Some(date) = dates.first() {
                self.select(*date);
            }
            return;
        }
        let mut dates: Vec<CivilDate> =
            dates.iter().copied().filter(|d| self.allowed(*d)).collect();
        dates.sort();
        dates.dedup();
        if self.mode == CalendarMode::Range && dates.len() > 2 {
            dates = vec![dates[0], dates[dates.len() - 1]];
        }
        self.chosen = dates;
    }

    fn is_chosen(&self, date: CivilDate) -> bool {
        match self.mode {
            CalendarMode::Single => date == self.selected,
            CalendarMode::Multiple => self.chosen.contains(&date),
            CalendarMode::Range => match self.chosen.as_slice() {
                [start] => date == *start,
                [start, end] => date >= *start && date <= *end,
                _ => false,
            },
        }
    }

    /// The range role of `date` in a `Range` calendar.
    fn range_day(&self, date: CivilDate) -> Option<CalendarDay> {
        if self.mode != CalendarMode::Range {
            return None;
        }
        match self.chosen.as_slice() {
            [start] | [start, _] if date == *start => Some(CalendarDay::RangeStart),
            [_, end] if date == *end => Some(CalendarDay::RangeEnd),
            [start, end] if date > *start && date < *end => Some(CalendarDay::RangeMiddle),
            _ => None,
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
                selected: self.is_chosen(*date),
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
        let target = match key.in_reading_order(self.rtl) {
            Key::ArrowLeft => self.selected.add_days(-1),
            Key::ArrowRight => self.selected.add_days(1),
            Key::ArrowUp => self.selected.add_days(-7),
            Key::ArrowDown => self.selected.add_days(7),
            Key::PageUp => self.selected.add_months(-1),
            Key::PageDown => self.selected.add_months(1),
            Key::Enter | Key::Space if self.mode != CalendarMode::Single => {
                return self.activate(self.selected);
            }
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

    /// The edge of one day cell in the `Calendar` recipe.
    pub fn cell_size(styles: &Styles, tokens: &DesignTokens) -> f64 {
        styles
            .resolve(tokens, StyleQuery::new(ControlKind::Calendar, Slot::Day))
            .min_width
    }

    /// The calendar size: seven cells wide; a caption row, a weekday row,
    /// and six weeks high.
    pub fn size(styles: &Styles, tokens: &DesignTokens) -> Size {
        let cell = Self::cell_size(styles, tokens);
        Size::new(cell * 7.0, cell * 8.0)
    }

    /// Places the parts from `origin`. Columns follow reading order, and
    /// the previous-month button is at the start of the caption row.
    pub fn layout(
        origin: Point,
        styles: &Styles,
        tokens: &DesignTokens,
        rtl: bool,
    ) -> CalendarLayout {
        let cell = Self::cell_size(styles, tokens);
        let column = |index: usize| {
            let slot = if rtl { 6 - index } else { index };
            origin.x + slot as f64 * cell
        };
        let at = |index: usize, row: usize| {
            Rect::from_xywh(column(index), origin.y + row as f64 * cell, cell, cell)
        };
        CalendarLayout {
            caption: Rect::from_xywh(origin.x + cell, origin.y, cell * 5.0, cell),
            previous: at(0, 0),
            next: at(6, 0),
            weekdays: (0..7).map(|index| at(index, 1)).collect(),
            days: (0..42).map(|index| at(index % 7, 2 + index / 7)).collect(),
        }
    }

    /// Paints the caption, the month buttons, the weekday names, and the
    /// days in `layout`. `today` gets the `muted` fill, days outside the
    /// month are muted, and the selected day fills `primary`.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        layout: &CalendarLayout,
        today: Option<CivilDate>,
        focus: CalendarFocus,
    ) {
        let query = |slot| StyleQuery::new(ControlKind::Calendar, slot);
        match &self.caption {
            Some(dropdowns) => dropdowns.paint_triggers(cx, self.shown, focus.caption_part()),
            None => {
                let caption = cx.style(query(Slot::Caption));
                cx.label(layout.caption, &self.title(), &caption, TextAlign::Center);
            }
        }
        let (back, forward) = if cx.rtl {
            (Icon::ChevronRight, Icon::ChevronLeft)
        } else {
            (Icon::ChevronLeft, Icon::ChevronRight)
        };
        let host = cx.state;
        for (rect, slot, icon, part, hit) in [
            (
                layout.previous,
                Slot::Previous,
                back,
                CalendarFocus::Previous,
                CalendarPart::Previous,
            ),
            (
                layout.next,
                Slot::Next,
                forward,
                CalendarFocus::Next,
                CalendarPart::Next,
            ),
        ] {
            let state = self
                .parts
                .state(hit, host)
                .set(VisualState::FOCUS_VISIBLE, focus == part);
            let style = cx.style(query(slot).state(state));
            cx.slot(rect, &style, |cx, inner| cx.icon(inner, icon, &style));
        }
        let weekday = cx.style(query(Slot::Weekday));
        for (name, rect) in self.weekday_headers().iter().zip(&layout.weekdays) {
            cx.label(*rect, name, &weekday, TextAlign::Center);
        }
        let band = cx.style(query(Slot::Range));
        let ranged = matches!(self.chosen.as_slice(), [_, _]);
        for (index, (cell, rect)) in self.cells().iter().zip(&layout.days).enumerate() {
            let range = self.range_day(cell.date);
            let variant = if let Some(range) = range {
                range
            } else if !cell.in_month {
                CalendarDay::Outside
            } else if Some(cell.date) == today {
                CalendarDay::Today
            } else {
                CalendarDay::Default
            };
            // A muted band joins the two ends of a range.
            let half = rect.size.width / 2.0;
            let trailing = (range == Some(CalendarDay::RangeStart)) != cx.rtl;
            if ranged && matches!(range, Some(CalendarDay::RangeStart | CalendarDay::RangeEnd)) {
                let x = if trailing {
                    rect.origin.x + half
                } else {
                    rect.origin.x
                };
                let mut fill = band.clone();
                fill.radius = crate::style::Corners::ZERO;
                cx.slot(
                    Rect::from_xywh(x, rect.origin.y, half, rect.size.height),
                    &fill,
                    |_, _| {},
                );
            }
            let cursor = cell.date == self.selected;
            let state = self
                .parts
                .state(CalendarPart::Day(index), host)
                .set(
                    VisualState::SELECTED,
                    cell.selected && range != Some(CalendarDay::RangeMiddle),
                )
                .set(VisualState::DISABLED, host.disabled() || !cell.enabled)
                .set(
                    VisualState::FOCUS_VISIBLE,
                    cursor && focus == CalendarFocus::Grid,
                );
            let style = cx.style(query(Slot::Day).variant(variant).state(state));
            let day = cell.date.day().to_string();
            cx.slot(*rect, &style, |cx, inner| {
                cx.label(inner, &day, &style, TextAlign::Center);
            });
        }
    }

    /// The chosen days as text: a list of long dates, or a range with an
    /// en dash between its ends.
    pub fn value_text(&self) -> String {
        let dates = self.dates();
        let long: Vec<String> = dates.iter().map(|d| d.format_long(&self.locale)).collect();
        match self.mode {
            CalendarMode::Range => long.join(" – "),
            _ => long.join(", "),
        }
    }

    pub fn semantics(&self, name: &str) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Table,
            name: name.to_owned(),
            description: self.title(),
            value: Some(self.value_text()),
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

/// `HH:MM:SS`, the locale-independent time a hosted editor reports.
fn time_iso(time: CivilTime) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second()
    )
}

impl DateTimeEdit {
    /// The value a hosted editor reports: `Date` for a date editor, and
    /// ISO 8601 text (`HH:MM:SS`, or `YYYY-MM-DDTHH:MM:SS`) otherwise.
    fn hosted_value(&self) -> ControlValue {
        match self.kind {
            DateTimeKind::Date => ControlValue::Date(Some(self.value.date)),
            DateTimeKind::Time => ControlValue::Text(time_iso(self.value.time)),
            DateTimeKind::DateTime => ControlValue::Text(format!(
                "{}T{}",
                self.value.date.iso(),
                time_iso(self.value.time)
            )),
        }
    }

    /// Runs `keys` and reports one change when the value changed. The
    /// value before the first change waits for Escape.
    fn hosted_keys(
        &mut self,
        cx: &mut ControlContext<'_>,
        keys: impl IntoIterator<Item = Key>,
    ) -> bool {
        let old = self.value;
        let mut changed = false;
        for key in keys {
            changed |= self.handle_key(key);
        }
        self.report(cx, old, changed);
        changed
    }

    /// Reports a change from `old`, and requests paint and semantics.
    fn report(&mut self, cx: &mut ControlContext<'_>, old: CivilDateTime, changed: bool) {
        if changed {
            self.before.get_or_insert(old);
            cx.emit(ControlMessage::Changed(self.hosted_value()));
            // The text width and the section edges follow the value.
            cx.request_layout();
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Commits the typed digits and the range. Nothing waits for Escape
    /// after it. Returns true when the value or the shown text changed.
    fn hosted_commit(&mut self, cx: &mut ControlContext<'_>) -> bool {
        let pending = self.typed.is_some();
        let changed = self.hosted_keys(cx, [Key::Enter]);
        self.before = None;
        changed || pending
    }

    /// Escape: drops the typed digits and restores the value from before
    /// the first change. Returns false when there is nothing to revert.
    fn revert(&mut self, cx: &mut ControlContext<'_>) -> bool {
        let pending = self.typed.take().is_some();
        let Some(before) = self.before.take() else {
            cx.request_paint();
            return pending;
        };
        let changed = before != self.value;
        let old = self.value;
        self.value = before;
        self.report(cx, old, changed);
        self.before = None;
        true
    }

    /// The section under `position`: the nearest one by its text.
    fn section_at(&self, bounds: Rect, position: Point) -> usize {
        let start = bounds.origin.x + self.text_box.origin.x;
        let offset = if self.rtl {
            start + self.text_box.size.width - position.x
        } else {
            position.x - start
        };
        self.edges
            .iter()
            .position(|edge| offset < *edge)
            .unwrap_or(self.fields.len() - 1)
    }

    /// The stepper rectangle at the end of `bounds`.
    fn steppers(bounds: Rect, rtl: bool) -> Rect {
        let width = STEPPER_WIDTH.min(bounds.size.width);
        let x = if rtl {
            bounds.origin.x
        } else {
            bounds.max_x() - width
        };
        Rect::from_xywh(x, bounds.origin.y, width, bounds.size.height)
    }
}

/// A click on a section focuses it, and a click on the steppers steps the
/// focused section. Enter and a loss of focus commit; Escape reverts.
impl CustomControl for DateTimeEdit {
    /// The text, the padding, and the steppers, at least the `Input`
    /// height.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let width =
            cx.text.width(&self.text(), &style.text) + style.padding.horizontal() + STEPPER_WIDTH;
        let height = style
            .min_height
            .max(style.text.line_height + style.padding.vertical());
        Measured::new(cx.constraints.constrain(Size::new(width, height)))
    }

    /// Records the direction, the text box, and the end of each section.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let inner = content_rect(bounds, &style, self.rtl);
        self.text_box = inner.translate(-bounds.origin.x, -bounds.origin.y);
        let (text, ranges) = self.sections(false);
        self.edges = ranges
            .iter()
            .map(|range| {
                let end = text
                    .char_indices()
                    .nth(range.end)
                    .map_or(text.len(), |(at, _)| at);
                cx.text.width(&text[..end], &style.text)
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                // A press on the steppers steps the focused field.
                let steppers = Self::steppers(cx.bounds(), self.rtl);
                if steppers.contains(*position) {
                    let key = if position.y < steppers.center().y {
                        Key::ArrowUp
                    } else {
                        Key::ArrowDown
                    };
                    self.hosted_keys(cx, [key]);
                } else {
                    let old = self.value;
                    let changed = self.move_to(self.section_at(cx.bounds(), *position));
                    self.report(cx, old, changed);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Escape, ..
            }) => {
                if self.revert(cx) {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Enter, ..
            }) => {
                if self.hosted_commit(cx) {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if matches!(
                    key,
                    Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                        | Key::Backspace
                        | Key::Delete
                ) =>
            {
                self.hosted_keys(cx, [*key]);
                EventReply::Stop
            }
            ControlEvent::Text(text) => {
                self.hosted_keys(cx, text.chars().map(Key::Character));
                EventReply::Stop
            }
            // The value cannot be empty, so Cut copies only.
            ControlEvent::Edit(command) => {
                let value = self.text();
                if let Some(ValueEdit::Paste(text)) = value_edit(cx, *command, &value, false) {
                    self.hosted_keys(cx, text.chars().map(Key::Character));
                }
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                if !focused {
                    self.hosted_commit(cx);
                }
                self.before = None;
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state;
        DateTimeEdit::paint(self, cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        DateTimeEdit::semantics(self, strings)
    }

    /// `Increment` and `Decrement` step the focused field as ArrowUp and
    /// ArrowDown do.
    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let key = match action {
            SemanticAction::Increment => Key::ArrowUp,
            SemanticAction::Decrement => Key::ArrowDown,
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        self.hosted_keys(cx, [key]);
        true
    }

    /// Takes `Date` for a date editor, and ISO 8601 text for a time
    /// (`HH:MM` or `HH:MM:SS`) or a date and time (`YYYY-MM-DDTHH:MM`).
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let next = match (self.kind, value) {
            (DateTimeKind::Date, ControlValue::Date(Some(date))) => CivilDateTime {
                date: *date,
                time: self.value.time,
            },
            (DateTimeKind::Time, ControlValue::Text(text)) => match CivilTime::parse(text) {
                Ok(time) => CivilDateTime {
                    date: self.value.date,
                    time,
                },
                Err(_) => return false,
            },
            (DateTimeKind::DateTime, ControlValue::Text(text)) => {
                let parsed = text.split_once('T').and_then(|(date, time)| {
                    Some(CivilDateTime {
                        date: CivilDate::parse_iso(date).ok()?,
                        time: CivilTime::parse(time).ok()?,
                    })
                });
                match parsed {
                    Some(value) => value,
                    None => return false,
                }
            }
            _ => return false,
        };
        // The bound value comes back after each change; it keeps the
        // typed digits and the value for Escape.
        if next == self.value {
            return true;
        }
        self.typed = None;
        self.before = None;
        self.value = next;
        true
    }
}

/// The caption dropdowns as `Calendar` and `DatePicker` drive them. A
/// choice in a dropdown shows its month.
impl Calendar {
    /// Places the caption dropdowns in the caption row of `layout`.
    pub(crate) fn arrange_caption(&mut self, cx: &MeasureContext<'_>, layout: &CalendarLayout) {
        self.rtl = cx.rtl();
        if let Some(caption) = &mut self.caption {
            caption.arrange(cx, layout.caption);
        }
    }

    /// The tab stops inside the calendar, in order.
    pub(crate) fn stops(&self) -> &'static [CalendarFocus] {
        if self.caption.is_some() {
            &[
                CalendarFocus::Month,
                CalendarFocus::Year,
                CalendarFocus::Grid,
            ]
        } else {
            &[CalendarFocus::Grid]
        }
    }

    /// The dropdown that `focus` names, when the caption has dropdowns.
    pub(crate) fn caption_part(&self, focus: CalendarFocus) -> Option<CaptionPart> {
        self.caption.as_ref().and(focus.caption_part())
    }

    /// The dropdown whose list is open.
    pub(crate) fn caption_open(&self) -> Option<CaptionPart> {
        self.caption.as_ref()?.open_part()
    }

    /// The rectangle of the open dropdown list.
    pub(crate) fn caption_frame(&self) -> Option<Rect> {
        self.caption.as_ref()?.frame()
    }

    /// The shown month and the range, as the dropdowns read them.
    fn caption_context(&self) -> CaptionContext {
        CaptionContext {
            shown: self.shown,
            minimum: self.minimum,
            maximum: self.maximum,
        }
    }

    /// Shows the month of a `Show` reply.
    fn apply_caption(&mut self, reply: CaptionReply) -> CaptionReply {
        if let CaptionReply::Show(year, month) = reply {
            self.show(year, month);
        }
        reply
    }

    /// Opens the list of `part`.
    pub(crate) fn open_caption(&mut self, part: CaptionPart) {
        let at = self.caption_context();
        if let Some(caption) = &mut self.caption {
            caption.open(part, at);
        }
    }

    /// Closes the open dropdown list.
    pub(crate) fn close_caption(&mut self) {
        if let Some(caption) = &mut self.caption {
            caption.close();
        }
    }

    /// A key for the focused dropdown `part`.
    pub(crate) fn caption_key(&mut self, part: CaptionPart, key: Key) -> CaptionReply {
        let at = self.caption_context();
        let reply = match &mut self.caption {
            Some(caption) => caption.key(part, key, at),
            None => CaptionReply::Ignored,
        };
        self.apply_caption(reply)
    }

    /// A pointer event at `time` for the dropdowns.
    pub(crate) fn caption_pointer(&mut self, event: &PointerEvent, time: f64) -> CaptionReply {
        let at = self.caption_context();
        let reply = match &mut self.caption {
            Some(caption) => caption.pointer(event, time, at),
            None => CaptionReply::Ignored,
        };
        self.apply_caption(reply)
    }

    /// Typed text for the open dropdown list.
    pub(crate) fn caption_text(&mut self, text: &str, time: f64) -> CaptionReply {
        match &mut self.caption {
            Some(caption) => caption.text(text, time),
            None => CaptionReply::Ignored,
        }
    }

    /// Stops the hover scroll of the open list.
    pub(crate) fn caption_stop_scroll(&mut self) {
        if let Some(caption) = &mut self.caption {
            caption.stop_scroll();
        }
    }

    /// The time of the open list's next hover scroll step.
    pub(crate) fn caption_deadline(&self) -> Option<f64> {
        self.caption.as_ref()?.deadline()
    }

    /// Runs the hover scroll step due at `time`. Returns true when the
    /// list scrolled.
    pub(crate) fn caption_step(&mut self, time: f64) -> bool {
        self.caption
            .as_mut()
            .is_some_and(|caption| caption.step_scroll(time))
    }

    /// The text that names the focused dropdown `part`.
    pub(crate) fn caption_description(&self, part: CaptionPart) -> String {
        self.caption
            .as_ref()
            .map(|caption| caption.description(part, self.shown))
            .unwrap_or_default()
    }

    /// Follows hover and press of the month buttons and the enabled days
    /// in `layout`. An open dropdown list covers the parts below it.
    pub(crate) fn track_parts(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        layout: Option<&CalendarLayout>,
    ) {
        let mut parts = self.parts;
        parts.track(cx, event, |point| {
            let covered = self
                .caption_frame()
                .is_some_and(|list| list.contains(point));
            let layout = layout.filter(|_| !covered)?;
            if layout.previous.contains(point) {
                return Some(CalendarPart::Previous);
            }
            if layout.next.contains(point) {
                return Some(CalendarPart::Next);
            }
            let index = layout.days.iter().position(|day| day.contains(point))?;
            let enabled = self.cells().get(index).is_some_and(|cell| cell.enabled);
            enabled.then_some(CalendarPart::Day(index))
        });
        self.parts = parts;
    }

    /// Paints the open dropdown list.
    pub(crate) fn paint_caption_list(&self, cx: &mut PaintContext<'_>) {
        if let Some(caption) = &self.caption {
            caption.paint_list(cx, self.shown);
        }
    }
}

impl Calendar {
    /// Names the calendar for its hosted semantics.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The day a hosted calendar marks as today.
    #[must_use]
    pub fn with_today(mut self, today: CivilDate) -> Self {
        self.today = Some(today);
        self
    }

    /// Reports the chosen days: `Date` in `Single` mode, else `Dates`.
    fn report(&self, cx: &mut ControlContext<'_>) {
        let value = match self.mode {
            CalendarMode::Single => ControlValue::Date(Some(self.selected)),
            _ => ControlValue::Dates(self.dates()),
        };
        cx.emit(ControlMessage::Changed(value));
        cx.request_semantics();
    }

    fn hosted_press(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        let Some(layout) = &self.layout else {
            return EventReply::Continue;
        };
        self.focus = CalendarFocus::Grid;
        if layout.previous.contains(position) {
            self.show_month(-1);
            cx.request_semantics();
        } else if layout.next.contains(position) {
            self.show_month(1);
            cx.request_semantics();
        } else if let Some(index) = layout.days.iter().position(|day| day.contains(position)) {
            let cell = self.cells().get(index).copied();
            if let Some(cell) = cell.filter(|cell| cell.enabled)
                && self.activate(cell.date)
            {
                self.report(cx);
            }
        }
        cx.request_paint();
        EventReply::Stop
    }

    /// Routes a pointer event to the caption dropdowns first.
    fn hosted_pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        match self.caption_pointer(event, cx.time()) {
            CaptionReply::Ignored => {}
            CaptionReply::Pressed(part) => {
                self.focus = CalendarFocus::from_part(part);
                cx.request_focus();
                cx.request_paint();
                cx.request_semantics();
                return EventReply::Stop;
            }
            reply => {
                cx.request_paint();
                if matches!(reply, CaptionReply::Show(..)) {
                    cx.request_semantics();
                }
                if !matches!(event, PointerEvent::Move { .. }) {
                    return EventReply::Stop;
                }
            }
        }
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => self.hosted_press(cx, position),
            _ => EventReply::Continue,
        }
    }

    /// Tab and Shift+Tab move between the dropdowns and the grid, and
    /// leave the calendar at the ends.
    fn hosted_tab(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let stops = self.stops();
        let current = stops
            .iter()
            .position(|stop| *stop == self.focus)
            .unwrap_or(0);
        self.close_caption();
        cx.request_paint();
        match crate::hosted::tab_stop(current, stops.len(), cx.modifiers().shift) {
            Some(next) => {
                self.focus = stops[next];
                cx.request_semantics();
                EventReply::Stop
            }
            None => EventReply::Continue,
        }
    }

    fn hosted_key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if key == Key::Tab && self.caption.is_some() {
            return self.hosted_tab(cx);
        }
        if let Some(part) = self.focus.caption_part() {
            let reply = self.caption_key(part, key);
            if reply == CaptionReply::Ignored {
                return EventReply::Continue;
            }
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Stop;
        }
        match key {
            // A single calendar leaves choosing to the application.
            Key::Enter | Key::Space if self.mode == CalendarMode::Single => {
                cx.emit(ControlMessage::Activated);
            }
            Key::Enter | Key::Space => {
                if self.handle_key(key) {
                    self.report(cx);
                }
            }
            Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::PageUp
            | Key::PageDown
            | Key::Home
            | Key::End => {
                if self.handle_key(key) {
                    if self.mode == CalendarMode::Single {
                        self.report(cx);
                    } else {
                        cx.request_semantics();
                    }
                }
            }
            _ => return EventReply::Continue,
        }
        cx.request_paint();
        EventReply::Stop
    }
}

impl CustomControl for Calendar {
    /// The day grid inside the `Calendar` root padding.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Root));
        let grid = Calendar::size(cx.styles, cx.tokens);
        let size = Size::new(
            grid.width + root.padding.horizontal(),
            grid.height + root.padding.vertical(),
        );
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Root));
        let inner = content_rect(bounds, &root, cx.rtl());
        let layout = Calendar::layout(inner.origin, cx.styles, cx.tokens, cx.rtl());
        self.arrange_caption(cx, &layout);
        self.layout = Some(layout);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let layout = self.layout.take();
        self.track_parts(cx, event, layout.as_ref());
        self.layout = layout;
        match event {
            ControlEvent::Pointer(pointer) => self.hosted_pointer(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.hosted_key(cx, *key),
            ControlEvent::Text(text) => {
                if self.caption_text(text, cx.time()) == CaptionReply::Ignored {
                    return EventReply::Continue;
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                // Tab enters at the first stop, and Shift+Tab at the last.
                let stops = self.stops();
                self.focus = match (focused, cx.modifiers().shift) {
                    (true, false) => stops[0],
                    (true, true) => stops[stops.len() - 1],
                    (false, _) => CalendarFocus::None,
                };
                if !focused {
                    self.close_caption();
                }
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Dismiss => {
                self.close_caption();
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Hover(false) => {
                self.caption_stop_scroll();
                EventReply::Continue
            }
            ControlEvent::Timer => {
                if self.caption_step(cx.time()) {
                    cx.request_paint();
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let root = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Root));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        // The grid rings for keyboard focus; a dropdown rings while focused.
        let focus = match self.focus {
            CalendarFocus::Month | CalendarFocus::Year if cx.state.focused() => self.focus,
            CalendarFocus::Grid if cx.state.focus_visible() => CalendarFocus::Grid,
            _ => CalendarFocus::None,
        };
        if let Some(layout) = &self.layout {
            Calendar::paint(self, cx, layout, self.today, focus);
        }
    }

    fn overlay(&self) -> Option<Rect> {
        self.caption_frame()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.paint_caption_list(cx);
    }

    fn deadline(&self) -> Option<f64> {
        self.caption_deadline()
    }

    /// A focused dropdown names itself in the description, takes
    /// `ShowMenu`, and reports whether its list is open.
    fn semantics(&self) -> SemanticNode {
        let mut node = Calendar::semantics(self, &self.name);
        if let Some(part) = self.focus.caption_part() {
            let open = self.caption_open() == Some(part);
            node.description = self.caption_description(part);
            node.state.expanded = Some(open);
            node.actions.extend([
                SemanticAction::ShowMenu,
                if open {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
            ]);
        }
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let part = self.focus.caption_part();
        match (action, part) {
            (SemanticAction::Focus, _) => cx.request_focus(),
            (SemanticAction::ShowMenu | SemanticAction::Expand, Some(part)) => {
                self.open_caption(part);
            }
            (SemanticAction::Collapse, Some(_)) => self.close_caption(),
            _ => return false,
        }
        cx.request_paint();
        cx.request_semantics();
        true
    }

    /// Takes `Date` in `Single` mode and `Dates` in the other modes.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match (self.mode, value) {
            (CalendarMode::Single, ControlValue::Date(Some(date))) if self.allowed(*date) => {
                self.select(*date);
                true
            }
            (CalendarMode::Multiple | CalendarMode::Range, ControlValue::Dates(dates)) => {
                self.set_dates(dates);
                if let Some(date) = self.chosen.first() {
                    self.selected = *date;
                    self.shown = (date.year(), date.month());
                }
                true
            }
            _ => false,
        }
    }
}
