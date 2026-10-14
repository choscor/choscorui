//! The dropdown caption of a calendar: a month list and a year list in
//! place of the caption text. `Calendar` and `DatePicker` share it.

use crate::custom::MeasureContext;
use crate::listbox::{ListEntry, ListPointer, ListPopup};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    CivilDate, Icon, Key, Locale, PointerButton, PointerEvent, Rect, SemanticParts, SemanticRole,
    days_in_month, month_name,
};

/// One of the two caption dropdowns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptionPart {
    Month,
    Year,
}

/// The calendar state that the dropdowns read: the shown month and the
/// range of days.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaptionContext {
    /// The shown year and month.
    pub shown: (i32, u8),
    pub minimum: Option<CivilDate>,
    pub maximum: Option<CivilDate>,
}

impl CaptionContext {
    /// True when `month` of `year` has a day in the range.
    fn has_month(&self, year: i32, month: u8) -> bool {
        let (Ok(first), Ok(last)) = (
            CivilDate::new(year, month, 1),
            CivilDate::new(year, month, days_in_month(year, month)),
        ) else {
            return false;
        };
        self.minimum.is_none_or(|minimum| last >= minimum)
            && self.maximum.is_none_or(|maximum| first <= maximum)
    }
}

/// What the caption did with an input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CaptionReply {
    /// The input is not for the caption.
    Ignored,
    /// The caption used the input, and its look changed.
    Used,
    /// A press on this dropdown's trigger: it takes focus, and its list
    /// opened or closed.
    Pressed(CaptionPart),
    /// The user chose this month to show, as a year and a month. The
    /// list closed.
    Show(i32, u8),
}

/// The month and year dropdowns of a calendar caption, their trigger
/// rectangles, and the open list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptionDropdowns {
    first_year: i32,
    months: ListPopup,
    years: ListPopup,
    month: Rect,
    year: Rect,
    open: Option<CaptionPart>,
}

impl CaptionDropdowns {
    /// Dropdowns for the localized month names and the years from
    /// `first_year` to `last_year`, in either order, inside 1..=9999.
    pub fn new(locale: &Locale, first_year: i32, last_year: i32) -> Self {
        let (first, last) = (
            first_year.min(last_year).clamp(1, 9999),
            first_year.max(last_year).clamp(1, 9999),
        );
        let months = (1..=12)
            .map(|month| ListEntry::option(month_name(locale, month)))
            .collect();
        let years = (first..=last)
            .map(|year| ListEntry::option(year.to_string()))
            .collect();
        Self {
            first_year: first,
            months: ListPopup::new(ControlKind::Select, months),
            years: ListPopup::new(ControlKind::Select, years),
            month: Rect::default(),
            year: Rect::default(),
            open: None,
        }
    }

    fn list(&self, part: CaptionPart) -> &ListPopup {
        match part {
            CaptionPart::Month => &self.months,
            CaptionPart::Year => &self.years,
        }
    }

    fn list_mut(&mut self, part: CaptionPart) -> &mut ListPopup {
        match part {
            CaptionPart::Month => &mut self.months,
            CaptionPart::Year => &mut self.years,
        }
    }

    /// The list entry of the shown month or year, if the list has it.
    fn current(&self, part: CaptionPart, shown: (i32, u8)) -> Option<usize> {
        let index = match part {
            CaptionPart::Month => usize::from(shown.1).checked_sub(1)?,
            CaptionPart::Year => usize::try_from(shown.0 - self.first_year).ok()?,
        };
        (index < self.list(part).entries.len()).then_some(index)
    }

    /// The trigger text: the month name or the year.
    pub fn label(&self, part: CaptionPart, shown: (i32, u8)) -> String {
        match part {
            CaptionPart::Month => self.months.entries[usize::from(shown.1.clamp(1, 12) - 1)]
                .label()
                .to_owned(),
            CaptionPart::Year => shown.0.to_string(),
        }
    }

    /// Adds the month and year triggers as `PopUpButton` parts under
    /// `parent`, each with its open list. The trigger of the `focus`
    /// dropdown has the focus, and the highlighted option of an open list
    /// takes it.
    pub fn semantic_parts(
        &self,
        shown: (i32, u8),
        focus: Option<CaptionPart>,
        parent: Option<usize>,
        parts: &mut SemanticParts,
    ) {
        for (part, rect) in [
            (CaptionPart::Month, self.month),
            (CaptionPart::Year, self.year),
        ] {
            let label = self.label(part, shown);
            let trigger = crate::hosted::pressable(
                parts,
                parent,
                SemanticRole::PopUpButton,
                &label,
                rect,
                true,
            );
            trigger.node.value = Some(label.clone());
            trigger.node.state.expanded = Some(self.open == Some(part));
            let position = parts.len() - 1;
            if focus == Some(part) {
                parts.set_focused(Some(position));
            }
            if self.open == Some(part) {
                self.list(part).semantic_parts(
                    &label,
                    self.current(part, shown),
                    Some(position),
                    parts,
                );
            }
        }
    }

    pub fn open_part(&self) -> Option<CaptionPart> {
        self.open
    }

    /// The rectangle of the open list.
    pub fn frame(&self) -> Option<Rect> {
        self.open.map(|part| self.list(part).frame)
    }

    /// Places the triggers side by side in the middle of `caption`, the
    /// month first in reading order, and each list below its trigger.
    /// The month trigger narrows when both do not fit.
    pub fn arrange(&mut self, cx: &MeasureContext<'_>, caption: Rect) {
        let trigger = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Trigger));
        let icon = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Icon));
        let gap = cx
            .style(StyleQuery::new(ControlKind::Calendar, Slot::Caption))
            .gap;
        let width = |list: &ListPopup| {
            let text = list
                .entries
                .iter()
                .map(|entry| cx.text.width(entry.label(), &trigger.text))
                .fold(0.0, f64::max);
            text + trigger.padding.horizontal() + trigger.gap + icon.icon_size
        };
        let year_width = width(&self.years).min(caption.size.width);
        let month_width = width(&self.months).min((caption.size.width - gap - year_width).max(0.0));
        let total = month_width + gap + year_width;
        let start = caption.center().x - total / 2.0;
        let height = trigger.min_height.min(caption.size.height);
        let y = caption.origin.y + (caption.size.height - height) / 2.0;
        let (month_x, year_x) = if cx.rtl() {
            (start + year_width + gap, start)
        } else {
            (start, start + month_width + gap)
        };
        self.month = Rect::from_xywh(month_x, y, month_width, height);
        self.year = Rect::from_xywh(year_x, y, year_width, height);
        let spacing = cx.tokens.spacing;
        self.months.place(cx, self.month, month_width, spacing);
        self.years.place(cx, self.year, year_width, spacing);
    }

    /// Opens the list of `part` at the shown month or year. Months and
    /// years with no day in the range are disabled.
    pub fn open(&mut self, part: CaptionPart, at: CaptionContext) {
        let shown = at.shown;
        for (index, entry) in self.months.entries.iter_mut().enumerate() {
            set_enabled(entry, at.has_month(shown.0, index as u8 + 1));
        }
        let first = self.first_year;
        for (index, entry) in self.years.entries.iter_mut().enumerate() {
            let year = first + index as i32;
            set_enabled(entry, (1..=12).any(|month| at.has_month(year, month)));
        }
        self.close();
        self.open = Some(part);
        let current = self
            .current(part, shown)
            .filter(|index| self.list(part).entries[*index].is_enabled_option());
        let list = self.list_mut(part);
        let start = current.or_else(|| list.options().next());
        list.highlight(start);
    }

    /// Closes the open list.
    pub fn close(&mut self) {
        if let Some(part) = self.open.take() {
            self.list_mut(part).stop_scroll();
        }
    }

    /// The month that choosing `index` in the list of `part` shows.
    fn choose(&mut self, part: CaptionPart, index: usize, shown: (i32, u8)) -> CaptionReply {
        self.close();
        match part {
            CaptionPart::Month => CaptionReply::Show(shown.0, index as u8 + 1),
            CaptionPart::Year => CaptionReply::Show(self.first_year + index as i32, shown.1),
        }
    }

    /// Handles a key while the trigger of `part` has focus. Enter, Space,
    /// ArrowDown, or ArrowUp opens its list. In the open list the arrows,
    /// Home, and End move the highlight, Enter or Space chooses, and
    /// Escape closes. Tab is left to the calendar.
    pub fn key(&mut self, part: CaptionPart, key: Key, at: CaptionContext) -> CaptionReply {
        if self.open != Some(part) {
            return match key {
                Key::Enter | Key::Space | Key::ArrowDown | Key::ArrowUp => {
                    self.open(part, at);
                    CaptionReply::Used
                }
                _ => CaptionReply::Ignored,
            };
        }
        match key {
            Key::Enter | Key::Space => match self.list(part).highlighted {
                Some(index) => self.choose(part, index, at.shown),
                None => {
                    self.close();
                    CaptionReply::Used
                }
            },
            Key::Escape => {
                self.close();
                CaptionReply::Used
            }
            key if self.list_mut(part).step(key) => CaptionReply::Used,
            _ => CaptionReply::Ignored,
        }
    }

    /// Typed text highlights the first matching option of the open list.
    pub fn text(&mut self, text: &str, time: f64) -> CaptionReply {
        let Some(part) = self.open else {
            return CaptionReply::Ignored;
        };
        let list = self.list_mut(part);
        if let Some(index) = list.type_ahead(text, time) {
            list.highlight(Some(index));
        }
        CaptionReply::Used
    }

    /// Handles a pointer event at `time`. The open list gets it first. A
    /// press on a trigger opens or closes its list, and a press anywhere
    /// else closes the open list.
    pub fn pointer(&mut self, event: &PointerEvent, time: f64, at: CaptionContext) -> CaptionReply {
        if let Some(part) = self.open {
            match self.list_mut(part).pointer(event, time) {
                ListPointer::Chosen(index) => return self.choose(part, index, at.shown),
                ListPointer::Used { .. } => return CaptionReply::Used,
                ListPointer::Ignored => {}
            }
        }
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = *event
        else {
            return CaptionReply::Ignored;
        };
        let pressed = if self.month.contains(position) {
            CaptionPart::Month
        } else if self.year.contains(position) {
            CaptionPart::Year
        } else if self.open.is_some() {
            self.close();
            return CaptionReply::Used;
        } else {
            return CaptionReply::Ignored;
        };
        if self.open == Some(pressed) {
            self.close();
        } else {
            self.open(pressed, at);
        }
        CaptionReply::Pressed(pressed)
    }

    /// Stops the hover scroll, for example when the pointer leaves.
    pub fn stop_scroll(&mut self) {
        if let Some(part) = self.open {
            self.list_mut(part).stop_scroll();
        }
    }

    /// The host time of the open list's next hover scroll step.
    pub fn deadline(&self) -> Option<f64> {
        self.open.and_then(|part| self.list(part).deadline())
    }

    /// Runs the hover scroll step that is due at `time`. Returns true
    /// when the list scrolled.
    pub fn step_scroll(&mut self, time: f64) -> bool {
        match self.open {
            Some(part) => self.list_mut(part).step_scroll(time),
            None => false,
        }
    }

    /// Paints the two triggers. `focus` rings one of them.
    pub fn paint_triggers(
        &self,
        cx: &mut PaintContext<'_>,
        shown: (i32, u8),
        focus: Option<CaptionPart>,
    ) {
        for (part, rect) in [
            (CaptionPart::Month, self.month),
            (CaptionPart::Year, self.year),
        ] {
            let state = VisualState::NONE
                .set(VisualState::FOCUSED, focus == Some(part))
                .set(VisualState::OPEN, self.open == Some(part));
            let trigger =
                cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Trigger).state(state));
            let icon = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Icon).state(state));
            let text = self.label(part, shown);
            cx.slot(rect, &trigger, |cx, inner| {
                let size = icon.icon_size;
                let text_width = (inner.size.width - size - trigger.gap).max(0.0);
                let (text_x, icon_x) = if cx.rtl {
                    (inner.max_x() - text_width, inner.origin.x)
                } else {
                    (inner.origin.x, inner.max_x() - size)
                };
                let line = Rect::from_xywh(text_x, inner.origin.y, text_width, inner.size.height);
                cx.label(line, &text, &trigger, TextAlign::Start);
                let mark = Rect::from_xywh(icon_x, inner.origin.y, size, inner.size.height);
                cx.icon(mark, Icon::ChevronDown, &icon);
            });
        }
    }

    /// Paints the open list with a check mark at the shown month or year.
    pub fn paint_list(&self, cx: &mut PaintContext<'_>, shown: (i32, u8)) {
        if let Some(part) = self.open {
            let current = self.current(part, shown);
            self.list(part)
                .paint(cx, &|index| current == Some(index), Icon::Check);
        }
    }
}

fn set_enabled(entry: &mut ListEntry, enabled: bool) {
    if let ListEntry::Option { enabled: slot, .. } = entry {
        *slot = enabled;
    }
}
