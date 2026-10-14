//! A button that opens a calendar in a popover.

use super::input::default_width;
use crate::calendar_caption::CaptionReply;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::datetime_controls::{Calendar, CalendarFocus, CalendarLayout, CalendarMode};
use crate::hosted::{self, Reveal};
use crate::popup::{Placement, gap_anchor, place_popup};
use crate::style::{
    Content, ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign, VisualState,
    content_rect,
};
use choscorui_core::{
    CivilDate, EventPhase, EventReply, Icon, Key, KeyEvent, Locale, Localizer, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole,
    Size,
};

/// A date picker: an outline button that shows the chosen date, and a
/// calendar popover below it. A single-date picker closes when a day is
/// chosen; multiple and range pickers stay open.
pub struct DatePicker {
    name: String,
    placeholder: String,
    /// The calendar. A single-date picker runs it in `Multiple` mode with
    /// at most one chosen day, so arrow keys move a cursor and do not
    /// choose.
    calendar: Calendar,
    mode: CalendarMode,
    today: Option<CivilDate>,
    invalid: bool,
    enabled: bool,
    reveal: Reveal,
    motion_ms: u32,
    focus: CalendarFocus,
    frame: Rect,
    layout: Option<CalendarLayout>,
    /// How far the calendar scrolls up in a popover that the window
    /// limits, and whether the next arrange scrolls the cursor day into
    /// view.
    scroll: f64,
    reveal_cursor: bool,
}

impl DatePicker {
    /// A picker that opens on `shown`'s month with no day chosen.
    pub fn new(name: impl Into<String>, locale: Locale, shown: CivilDate) -> Self {
        Self {
            name: name.into(),
            placeholder: String::new(),
            calendar: Calendar::new(locale, shown).with_mode(CalendarMode::Multiple),
            mode: CalendarMode::Single,
            today: None,
            invalid: false,
            enabled: true,
            reveal: Reveal::default(),
            motion_ms: 0,
            focus: CalendarFocus::None,
            frame: Rect::default(),
            layout: None,
            scroll: 0.0,
            reveal_cursor: false,
        }
    }

    #[must_use]
    pub fn with_mode(mut self, mode: CalendarMode) -> Self {
        self.mode = mode;
        let inner = if mode == CalendarMode::Single {
            CalendarMode::Multiple
        } else {
            mode
        };
        self.calendar = self.calendar.with_mode(inner);
        self
    }

    pub fn mode(&self) -> CalendarMode {
        self.mode
    }

    /// Shows the month and the year as dropdowns in the calendar caption.
    /// See [`Calendar::with_dropdown_caption`].
    #[must_use]
    pub fn with_dropdown_caption(mut self, first_year: i32, last_year: i32) -> Self {
        self.calendar = self.calendar.with_dropdown_caption(first_year, last_year);
        self
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// The day the calendar marks as today.
    #[must_use]
    pub fn with_today(mut self, today: CivilDate) -> Self {
        self.today = Some(today);
        self
    }

    /// Limits the days that can be chosen to `minimum` through
    /// `maximum`. See [`Calendar::set_range`].
    #[must_use]
    pub fn with_range(mut self, minimum: Option<CivilDate>, maximum: Option<CivilDate>) -> Self {
        self.calendar.set_range(minimum, maximum);
        self
    }

    /// Disables each day for which `disabled` is true. See
    /// [`Calendar::with_disabled_days`].
    #[must_use]
    pub fn with_disabled_days(mut self, disabled: impl Fn(CivilDate) -> bool + 'static) -> Self {
        self.calendar = self.calendar.with_disabled_days(disabled);
        self
    }

    /// The picker with the invalid state from the start.
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The chosen days; empty while nothing is chosen.
    pub fn dates(&self) -> Vec<CivilDate> {
        self.calendar.dates()
    }

    /// Chooses `dates` as the application's value. A single-date picker
    /// keeps the first.
    pub fn set_dates(&mut self, dates: &[CivilDate]) {
        let dates = if self.mode == CalendarMode::Single {
            &dates[..dates.len().min(1)]
        } else {
            dates
        };
        self.calendar.set_dates(dates);
        if let Some(date) = dates.first() {
            self.calendar.select(*date);
        }
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    /// The flags the trigger recipe reads: `host` with open and invalid.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::OPEN, self.reveal.open)
            .set(VisualState::INVALID, host.invalid() || self.invalid)
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::DatePicker, slot).state(state)
    }

    /// The chosen days as text, or `None` while nothing is chosen.
    fn text(&self) -> Option<String> {
        (!self.dates().is_empty()).then(|| self.calendar.value_text())
    }

    /// The trigger label: the placeholder, the chosen days, or the count
    /// of several days that a multiple picker chose.
    fn label(&self, strings: &Localizer) -> String {
        let count = self.dates().len();
        if self.mode == CalendarMode::Multiple && count > 1 {
            let number = count.to_string();
            let args = [("count", number.as_str())];
            if let Ok(summary) = strings.plural("choscorui.date_picker.count", count as u64, &args)
            {
                return summary;
            }
        }
        self.text().unwrap_or_else(|| self.placeholder.clone())
    }

    /// Clears the chosen days and reports it. Returns false when none
    /// were chosen.
    fn clear(&mut self, cx: &mut ControlContext<'_>) -> bool {
        if self.dates().is_empty() {
            return false;
        }
        self.calendar.set_dates(&[]);
        cx.emit(ControlMessage::Changed(match self.mode {
            CalendarMode::Single => ControlValue::Date(None),
            _ => ControlValue::Dates(Vec::new()),
        }));
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
        true
    }

    /// Opens or closes the popover at `time`. Returns true when it
    /// changed.
    fn apply_open(&mut self, open: bool, time: f64) -> bool {
        if self.reveal.open == open {
            return false;
        }
        self.reveal.set(open, time);
        self.calendar.close_caption();
        if open {
            self.calendar.parts.clear();
            // The popover opens at its top, scrolled to the chosen day.
            self.scroll = 0.0;
            self.reveal_cursor = true;
        }
        self.focus = if open {
            CalendarFocus::Grid
        } else {
            CalendarFocus::None
        };
        true
    }

    /// Opens or closes the popover and reports it. Returns true when it
    /// changed.
    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) -> bool {
        let changed = self.apply_open(open, cx.time());
        if changed {
            cx.emit(ControlMessage::OpenChanged(open));
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
        }
        changed
    }

    /// Closes the popover without a choice, and reports `Dismissed`.
    fn dismiss(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let closed = self.change_open(cx, false);
        hosted::dismissed(cx, closed)
    }

    /// Opens a closed popover, and closes an open one without a choice.
    fn toggle(&mut self, cx: &mut ControlContext<'_>) {
        if self.reveal.open {
            self.dismiss(cx);
        } else {
            self.change_open(cx, true);
        }
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, date: CivilDate) {
        if self.mode == CalendarMode::Single {
            // A second choice of the chosen day clears it.
            if self.calendar.dates() == [date] {
                self.clear(cx);
            } else {
                self.calendar.set_dates(&[date]);
                cx.emit(ControlMessage::Changed(ControlValue::Date(Some(date))));
                cx.request_semantics();
            }
            self.calendar.select(date);
            self.change_open(cx, false);
        } else if self.calendar.activate(date) {
            cx.emit(ControlMessage::Changed(ControlValue::Dates(
                self.calendar.dates(),
            )));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    /// Routes a pointer event in the open popover to the caption
    /// dropdowns first. Returns `None` when they did not use it.
    fn caption_pointer(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &PointerEvent,
    ) -> Option<EventReply> {
        if !self.reveal.open {
            return None;
        }
        match self.calendar.caption_pointer(event, cx.time()) {
            CaptionReply::Ignored => return None,
            CaptionReply::Pressed(part) => self.focus = CalendarFocus::from_part(part),
            CaptionReply::Used | CaptionReply::Show(..) => {}
        }
        cx.request_paint();
        cx.request_semantics();
        Some(if matches!(event, PointerEvent::Move { .. }) {
            EventReply::Continue
        } else {
            EventReply::Stop
        })
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        if self.reveal.open && self.frame.contains(position) {
            self.focus = CalendarFocus::Grid;
            let Some(layout) = &self.layout else {
                return EventReply::Stop;
            };
            if layout.previous.contains(position) {
                self.calendar.show_month(-1);
                cx.request_semantics();
            } else if layout.next.contains(position) {
                self.calendar.show_month(1);
                cx.request_semantics();
            } else if let Some(index) = layout.days.iter().position(|day| day.contains(position)) {
                let cell = self.calendar.cells().get(index).copied();
                if let Some(cell) = cell.filter(|cell| cell.enabled) {
                    self.activate(cx, cell.date);
                }
            }
            cx.request_paint();
            return EventReply::Stop;
        }
        if cx.bounds().contains(position) {
            cx.request_focus();
            self.toggle(cx);
            return EventReply::Stop;
        }
        EventReply::Continue
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if !self.reveal.open {
            return match key {
                Key::Enter | Key::Space | Key::ArrowDown | Key::ArrowUp => {
                    self.change_open(cx, true);
                    EventReply::Stop
                }
                Key::Backspace | Key::Delete if self.clear(cx) => EventReply::Stop,
                _ => EventReply::Continue,
            };
        }
        if key == Key::Escape && self.calendar.caption_open().is_some() {
            // Escape closes the dropdown list first.
            self.calendar.close_caption();
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Stop;
        }
        if key == Key::Tab {
            return self.tab(cx);
        }
        if matches!(key, Key::Enter | Key::Space)
            && matches!(self.focus, CalendarFocus::Previous | CalendarFocus::Next)
        {
            let mut focus = self.focus;
            self.calendar.press_button(&mut focus);
            self.focus = focus;
            self.reveal_cursor = true;
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Stop;
        }
        if let Some(part) = self.calendar.caption_part(self.focus) {
            if self.calendar.caption_key(part, key) == CaptionReply::Ignored {
                return EventReply::Continue;
            }
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Stop;
        }
        match key {
            Key::Escape => {
                self.dismiss(cx);
                EventReply::Stop
            }
            Key::Enter | Key::Space => {
                let date = self.calendar.selected();
                if self
                    .calendar
                    .cells()
                    .iter()
                    .any(|c| c.date == date && c.enabled)
                {
                    self.activate(cx, date);
                }
                EventReply::Stop
            }
            _ if self.focus != CalendarFocus::Grid => EventReply::Continue,
            key => {
                // Arrow keys, Page Up and Down, Home, and End move the
                // cursor; Enter or Space chooses.
                if self.calendar.handle_key(key) {
                    self.reveal_cursor = true;
                    cx.request_layout();
                    cx.request_paint();
                    cx.request_semantics();
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
        }
    }
}

impl DatePicker {
    /// The calendar parts in a popover at `frame` with all of its `size`,
    /// moved up by the scroll offset.
    fn calendar_layout(
        &self,
        cx: &MeasureContext<'_>,
        calendar: &ResolvedStyle,
        size: Size,
    ) -> CalendarLayout {
        let full = Rect::new(
            Point::new(self.frame.origin.x, self.frame.origin.y - self.scroll),
            size,
        );
        let inner = content_rect(full, calendar, cx.rtl());
        Calendar::layout(inner.origin, cx.styles, cx.tokens, cx.rtl())
    }

    /// Tab and Shift+Tab move between the month buttons, the caption
    /// dropdowns, and the grid. Past the ends they close the popover, and
    /// focus moves on.
    fn tab(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let stops = self.calendar.stops();
        let current = stops
            .iter()
            .position(|stop| *stop == self.focus)
            .unwrap_or(stops.len() - 1);
        self.calendar.close_caption();
        match hosted::tab_stop(current, stops.len(), cx.modifiers().shift) {
            Some(next) => {
                self.focus = stops[next];
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            None => {
                self.change_open(cx, false);
                EventReply::Continue
            }
        }
    }
}

impl CustomControl for DatePicker {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        // A count summary is short, so several days measure as the
        // placeholder does.
        let text = match self.text() {
            Some(text) if self.mode != CalendarMode::Multiple || self.dates().len() == 1 => text,
            _ => self.placeholder.clone(),
        };
        let content = Content {
            leading: Some(Icon::Calendar),
            label: &text,
            trailing: None,
        };
        let size = content.size(&root, cx.text);
        let unbounded = MeasureContext {
            constraints: choscorui_core::Constraints::UNBOUNDED,
            ..*cx
        };
        // A narrower width truncates the label.
        let width = size.width.max(default_width(&unbounded, &root));
        Measured {
            size: cx.constraints.constrain(Size::new(width, size.height)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let calendar = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Root));
        let content = cx.style(Self::query(Slot::Content, VisualState::OPEN));
        self.motion_ms = content.motion_ms;
        let grid = Calendar::size(cx.styles, cx.tokens);
        let size = Size::new(
            grid.width + calendar.padding.horizontal(),
            grid.height + calendar.padding.vertical(),
        );
        // A window that is too short limits the popover, and the
        // calendar scrolls in it.
        let placed = place_popup(
            gap_anchor(bounds, cx.tokens.spacing, None),
            size,
            cx.popup_bounds(),
            Placement::Below,
            cx.direction,
        );
        self.frame = placed.rect;
        let room = (size.height - self.frame.size.height).max(0.0);
        self.scroll = self.scroll.clamp(0.0, room);
        let mut layout = self.calendar_layout(cx, &calendar, size);
        if std::mem::take(&mut self.reveal_cursor) {
            let index = (self.calendar.cells().iter())
                .position(|cell| cell.date == self.calendar.selected());
            if let Some(day) = index.and_then(|index| layout.days.get(index)) {
                let frame = self.frame;
                if day.origin.y < frame.origin.y {
                    self.scroll -= frame.origin.y - day.origin.y;
                } else if day.max_y() > frame.max_y() {
                    self.scroll += day.max_y() - frame.max_y();
                }
                self.scroll = self.scroll.clamp(0.0, room);
                layout = self.calendar_layout(cx, &calendar, size);
            }
        }
        self.calendar.arrange_caption(cx, &layout);
        self.layout = Some(layout);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if let ControlEvent::Pointer(PointerEvent::Wheel {
            position, delta, ..
        }) = event
            && self.reveal.open
            && self.frame.contains(*position)
            && self.calendar.caption_frame().is_none()
        {
            self.scroll += delta.y;
            cx.request_layout();
            cx.request_paint();
            return EventReply::Stop;
        }
        let layout = self.layout.as_ref().filter(|_| self.reveal.open);
        self.calendar.track_parts(cx, event, layout);
        if let ControlEvent::Pointer(pointer) = event
            && let Some(reply) = self.caption_pointer(cx, pointer)
        {
            return reply;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => self.pointer(cx, *position),
            ControlEvent::Pointer(PointerEvent::Up { position, .. })
                if self.reveal.open && self.frame.contains(*position) =>
            {
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Text(text) if self.reveal.open => {
                if self.calendar.caption_text(text, cx.time()) == CaptionReply::Ignored {
                    return EventReply::Continue;
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => self.dismiss(cx),
            ControlEvent::Hover(false) => {
                self.calendar.caption_stop_scroll();
                EventReply::Continue
            }
            ControlEvent::Timer => {
                if self.calendar.caption_step(cx.time()) {
                    cx.request_paint();
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.visual_state(cx.state);
        let root = cx.style(Self::query(Slot::Root, state));
        let chosen = !self.dates().is_empty();
        let label = self.label(cx.strings);
        let placeholder = cx.style(Self::query(Slot::Placeholder, state));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let style = if chosen { &root } else { &placeholder };
            let content = Content {
                leading: Some(Icon::Calendar),
                label: &label,
                trailing: None,
            };
            let mut style = style.clone();
            style.gap = root.gap;
            style.icon_size = root.icon_size;
            content.draw(cx, inner, &style, TextAlign::Start);
        });
    }

    /// The popover, and the open dropdown list that can reach below it.
    fn overlay(&self) -> Option<Rect> {
        let list = self.calendar.caption_frame();
        self.reveal
            .open
            .then(|| list.map_or(self.frame, |list| self.frame.union(list)))
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        let Some(layout) = &self.layout else {
            return;
        };
        if progress <= 0.0 {
            return;
        }
        let content = cx.style(Self::query(Slot::Content, VisualState::OPEN));
        let calendar = cx.style(StyleQuery::new(ControlKind::Calendar, Slot::Root));
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        let frame = self.frame;
        // The day or caption ring shows only while the last input is the
        // keyboard: a pointer open shows no ring.
        let focus = if cx.state.focus_visible() {
            self.focus
        } else {
            CalendarFocus::None
        };
        Reveal::paint(cx, frame, progress, slide, |cx| {
            cx.slot(frame, &content, |cx, _| {
                cx.slot(frame, &calendar, |_, _| {});
                cx.list.push_clip(frame);
                self.calendar.paint(cx, layout, self.today, focus);
                let _ = cx.list.pop_clip();
            });
        });
        self.calendar.paint_caption_list(cx);
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn deadline(&self) -> Option<f64> {
        self.calendar
            .caption_deadline()
            .filter(|_| self.reveal.open)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::Activate,
                if self.reveal.open {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Button, &self.name, actions);
        node.value = Some(self.text().unwrap_or_else(|| self.placeholder.clone()));
        node.state.expanded = Some(self.reveal.open);
        node.state.invalid = self.invalid;
        node.state.disabled = !self.enabled;
        node
    }

    /// The open calendar is a `Dialog` part with the parts of a calendar.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let Some(layout) = self.layout.as_ref().filter(|_| self.reveal.open) else {
            return;
        };
        let frame = self.overlay().unwrap_or(layout.caption);
        parts.push(None, SemanticRole::Dialog, &self.name, frame);
        let dialog = Some(parts.len() - 1);
        self.calendar
            .semantic_parts(layout, self.today, self.focus, strings, dialog, parts);
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                self.toggle(cx);
                true
            }
            SemanticAction::Expand => {
                self.change_open(cx, true);
                true
            }
            SemanticAction::Collapse => {
                self.dismiss(cx);
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
        self.enabled = enabled;
        if !enabled {
            self.reveal = Reveal::default();
        }
    }

    /// Takes `Date` in single mode and `Dates` in multiple and range
    /// modes. Rejects more than two dates, or an end before the start, in
    /// range mode.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match (self.mode, value) {
            (CalendarMode::Single, ControlValue::Date(date)) => {
                self.set_dates(date.as_slice());
                true
            }
            (CalendarMode::Multiple, ControlValue::Dates(dates)) => {
                self.set_dates(dates);
                true
            }
            (CalendarMode::Range, ControlValue::Dates(dates))
                if dates.len() <= 2 && dates.is_sorted() =>
            {
                self.set_dates(dates);
                true
            }
            _ => false,
        }
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if open && !self.enabled {
            return false;
        }
        self.apply_open(open, time);
        true
    }
}
