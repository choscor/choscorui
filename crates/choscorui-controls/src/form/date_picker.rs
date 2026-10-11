//! A button that opens a calendar in a popover.

use super::input::default_width;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::datetime_controls::{Calendar, CalendarFocus, CalendarLayout, CalendarMode};
use crate::hosted::{self, Reveal};
use crate::style::{
    Content, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState, content_rect,
};
use choscorui_core::{
    CivilDate, EventPhase, EventReply, Icon, Key, KeyEvent, Locale, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
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
    enabled: bool,
    reveal: Reveal,
    motion_ms: u32,
    focus: CalendarFocus,
    frame: Rect,
    layout: Option<CalendarLayout>,
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
            enabled: true,
            reveal: Reveal::default(),
            motion_ms: 0,
            focus: CalendarFocus::None,
            frame: Rect::default(),
            layout: None,
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

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::DatePicker, slot).state(state)
    }

    /// The trigger text: the chosen days, or the placeholder.
    fn text(&self) -> Option<String> {
        (!self.dates().is_empty()).then(|| self.calendar.value_text())
    }

    fn set_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if self.reveal.open == open {
            return;
        }
        self.reveal.set(open, cx.time());
        self.focus = if open {
            CalendarFocus::Grid
        } else {
            CalendarFocus::None
        };
        cx.emit(ControlMessage::OpenChanged(open));
        cx.request_paint();
        cx.request_semantics();
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, date: CivilDate) {
        if self.mode == CalendarMode::Single {
            if self.calendar.dates() != [date] {
                self.calendar.set_dates(&[date]);
                cx.emit(ControlMessage::Changed(ControlValue::Date(Some(date))));
                cx.request_semantics();
            }
            self.calendar.select(date);
            self.set_open(cx, false);
        } else if self.calendar.activate(date) {
            cx.emit(ControlMessage::Changed(ControlValue::Dates(
                self.calendar.dates(),
            )));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        if self.reveal.open && self.frame.contains(position) {
            let Some(layout) = &self.layout else {
                return EventReply::Stop;
            };
            if layout.previous.contains(position) {
                self.calendar.show_month(-1);
            } else if layout.next.contains(position) {
                self.calendar.show_month(1);
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
            self.set_open(cx, !self.reveal.open);
            return EventReply::Stop;
        }
        EventReply::Continue
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if !self.reveal.open {
            return match key {
                Key::Enter | Key::Space | Key::ArrowDown => {
                    self.set_open(cx, true);
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            };
        }
        match key {
            Key::Escape => {
                self.set_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            Key::Tab => {
                self.set_open(cx, false);
                EventReply::Continue
            }
            Key::Enter | Key::Space => {
                let date = self.calendar.selected();
                self.activate(cx, date);
                EventReply::Stop
            }
            key => {
                // Arrow keys, Page Up and Down, Home, and End move the
                // cursor; Enter or Space chooses.
                if self.calendar.handle_key(key) {
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

impl CustomControl for DatePicker {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let text = self.text().unwrap_or_else(|| self.placeholder.clone());
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
        let width = size.width.max(default_width(&unbounded, &root));
        Measured {
            size: Size::new(width, size.height),
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
        let x = if cx.rtl() {
            bounds.max_x() - size.width
        } else {
            bounds.origin.x
        };
        self.frame = Rect::new(Point::new(x, bounds.max_y() + cx.tokens.spacing), size);
        let inner = content_rect(self.frame, &calendar, cx.rtl());
        self.layout = Some(Calendar::layout(
            inner.origin,
            cx.styles,
            cx.tokens,
            cx.rtl(),
        ));
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
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
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                self.set_open(cx, false);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.set(VisualState::OPEN, self.reveal.open);
        let root = cx.style(Self::query(Slot::Root, state));
        let text = self.text();
        let placeholder = cx.style(Self::query(Slot::Placeholder, state));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let style = if text.is_some() { root } else { placeholder };
            let label = text.as_deref().unwrap_or(&self.placeholder);
            let content = Content {
                leading: Some(Icon::Calendar),
                label,
                trailing: None,
            };
            let mut style = style;
            style.gap = root.gap;
            style.icon_size = root.icon_size;
            content.draw(cx, inner, &style, TextAlign::Start);
        });
    }

    fn overlay(&self) -> Option<Rect> {
        self.reveal.open.then_some(self.frame)
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
        Reveal::paint(cx, frame, progress, slide, |cx| {
            cx.slot(frame, &content, |cx, _| {
                cx.slot(frame, &calendar, |_, _| {});
                self.calendar.paint(cx, layout, self.today, self.focus);
            });
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
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
        if self.reveal.open {
            // The cursor day, as the grid would announce it.
            node.description = self
                .calendar
                .cells()
                .iter()
                .find(|cell| cell.date == self.calendar.selected())
                .map(|cell| self.calendar.cell_semantics(cell).name)
                .unwrap_or_default();
        }
        node.state.expanded = Some(self.reveal.open);
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                self.set_open(cx, !self.reveal.open);
                true
            }
            SemanticAction::Expand => {
                self.set_open(cx, true);
                true
            }
            SemanticAction::Collapse => {
                self.set_open(cx, false);
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
}
