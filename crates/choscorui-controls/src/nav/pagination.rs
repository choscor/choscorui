//! Page links with previous and next buttons.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// The most page slots, ellipses included, before pages collapse.
const MAX_SLOTS: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Previous,
    Page(usize),
    Ellipsis,
    Next,
}

/// Widths read from the `Pagination` recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Metrics {
    page: f64,
    previous: f64,
    next: f64,
    gap: f64,
}

/// Pagination: a previous button, page links, and a next button. At most
/// seven page slots show; ellipses stand for the pages between. Each
/// enabled part is a tab stop inside the control.
pub struct Pagination {
    name: String,
    previous: String,
    next: String,
    pages: usize,
    current: usize,
    parts: Vec<(Part, Rect)>,
    metrics: Metrics,
    bounds: Rect,
    focus: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    enabled: bool,
    rtl: bool,
}

impl Pagination {
    /// `pages` pages with page `current` (from 0) shown. `previous` and
    /// `next` label the buttons.
    pub fn new(
        name: impl Into<String>,
        previous: impl Into<String>,
        next: impl Into<String>,
        pages: usize,
        current: usize,
    ) -> Self {
        Self {
            name: name.into(),
            previous: previous.into(),
            next: next.into(),
            pages,
            current: current.min(pages.saturating_sub(1)),
            parts: Vec::new(),
            metrics: Metrics::default(),
            bounds: Rect::default(),
            focus: 0,
            hovered: None,
            pressed: None,
            enabled: true,
            rtl: false,
        }
    }

    /// The shown page, from 0.
    pub fn current(&self) -> usize {
        self.current
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Pagination, slot).state(state)
    }

    /// The page slots: every page when they fit, else the first and the
    /// last page around the current page and its neighbors.
    fn slots(&self) -> Vec<Part> {
        let n = self.pages;
        if n <= MAX_SLOTS {
            return (0..n).map(Part::Page).collect();
        }
        let c = self.current;
        let (start, end) = if c < 4 {
            (0, 4)
        } else if c + 4 >= n {
            (n - 5, n - 1)
        } else {
            (c - 1, c + 1)
        };
        let mut slots = Vec::new();
        if start > 0 {
            slots.push(Part::Page(0));
            slots.push(Part::Ellipsis);
        }
        slots.extend((start..=end).map(Part::Page));
        if end < n - 1 {
            slots.push(Part::Ellipsis);
            slots.push(Part::Page(n - 1));
        }
        slots
    }

    fn enabled_part(&self, part: Part) -> bool {
        match part {
            Part::Previous => self.current > 0,
            Part::Next => self.current + 1 < self.pages,
            Part::Page(_) => true,
            Part::Ellipsis => false,
        }
    }

    fn width(&self, part: Part) -> f64 {
        match part {
            Part::Previous => self.metrics.previous,
            Part::Next => self.metrics.next,
            Part::Page(_) | Part::Ellipsis => self.metrics.page,
        }
    }

    /// Places the parts in reading order inside the bounds.
    fn place(&mut self) {
        let mut sequence = vec![Part::Previous];
        sequence.extend(self.slots());
        sequence.push(Part::Next);
        let bounds = self.bounds;
        let gap = self.metrics.gap;
        let mut x = if self.rtl {
            bounds.max_x()
        } else {
            bounds.origin.x
        };
        self.parts = sequence
            .into_iter()
            .map(|part| {
                let width = self.width(part);
                let left = if self.rtl { x - width } else { x };
                x = if self.rtl {
                    x - width - gap
                } else {
                    x + width + gap
                };
                (
                    part,
                    Rect::from_xywh(left, bounds.origin.y, width, bounds.size.height),
                )
            })
            .collect();
    }

    fn stops(&self) -> Vec<usize> {
        self.parts
            .iter()
            .enumerate()
            .filter(|(_, (part, _))| self.enabled_part(*part))
            .map(|(index, _)| index)
            .collect()
    }

    fn part_at(&self, point: Point) -> Option<usize> {
        self.parts
            .iter()
            .position(|(part, rect)| self.enabled_part(*part) && rect.contains(point))
    }

    fn label(&self, part: Part) -> String {
        match part {
            Part::Previous => self.previous.clone(),
            Part::Next => self.next.clone(),
            Part::Page(page) => (page + 1).to_string(),
            Part::Ellipsis => String::new(),
        }
    }

    /// Shows `page`, keeping focus on the part that shows it.
    fn go(&mut self, cx: &mut ControlContext<'_>, page: usize, focus: Part) {
        if page >= self.pages || page == self.current {
            return;
        }
        self.current = page;
        cx.emit(ControlMessage::Changed(ControlValue::Index(Some(page))));
        self.place();
        let stops = self.stops();
        let target = self.parts.iter().position(|(part, _)| *part == focus);
        self.focus = target
            .and_then(|target| stops.iter().position(|stop| *stop == target))
            .or_else(|| {
                let shown = self
                    .parts
                    .iter()
                    .position(|(part, _)| *part == Part::Page(page))?;
                stops.iter().position(|stop| *stop == shown)
            })
            .unwrap_or(0);
        cx.request_paint();
        cx.request_semantics();
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, part: usize) {
        match self.parts.get(part).map(|(part, _)| *part) {
            Some(Part::Previous) if self.current > 0 => {
                self.go(cx, self.current - 1, Part::Previous);
            }
            Some(Part::Next) => self.go(cx, self.current + 1, Part::Next),
            Some(Part::Page(page)) => self.go(cx, page, Part::Page(page)),
            _ => {}
        }
    }
}

impl CustomControl for Pagination {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.arrange_metrics(cx);
        let slots = self.pages.min(MAX_SLOTS) as f64;
        let m = self.metrics;
        let width = m.previous + m.next + slots * m.page + (slots + 1.0) * m.gap;
        let height = cx
            .style(Self::query(Slot::Link, VisualState::NONE))
            .min_height;
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.arrange_metrics(cx);
        self.rtl = cx.rtl();
        self.bounds = bounds;
        self.place();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let stops = self.stops();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(part) = self.part_at(*position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(part);
                if let Some(stop) = stops.iter().position(|stop| *stop == part) {
                    self.focus = stop;
                }
                cx.request_focus();
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = self.part_at(*position);
                if let Some(part) = self.pressed.take().filter(|part| hit == Some(*part)) {
                    self.activate(cx, part);
                }
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = self.part_at(*position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => match key {
                Key::Tab => match hosted::tab_stop(self.focus, stops.len(), cx.modifiers().shift) {
                    Some(next) => {
                        self.focus = next;
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                },
                Key::Enter | Key::Space if !repeat => {
                    if let Some(part) = stops.get(self.focus) {
                        self.activate(cx, *part);
                    }
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            },
            ControlEvent::FocusChanged(focused) => {
                if *focused {
                    self.focus = if cx.modifiers().shift {
                        stops.len().saturating_sub(1)
                    } else {
                        0
                    };
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let stops = self.stops();
        let focused = stops.get(self.focus).copied();
        for (index, (part, rect)) in self.parts.iter().enumerate() {
            let state = VisualState::NONE
                .set(VisualState::HOVERED, self.hovered == Some(index))
                .set(VisualState::PRESSED, self.pressed == Some(index))
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && focused == Some(index),
                )
                .set(
                    VisualState::DISABLED,
                    host.disabled() || !self.enabled_part(*part),
                )
                .set(VisualState::SELECTED, *part == Part::Page(self.current));
            match part {
                Part::Page(_) => {
                    let style = cx.style(Self::query(Slot::Link, state));
                    let label = self.label(*part);
                    cx.slot(*rect, &style, |cx, inner| {
                        cx.label(inner, &label, &style, TextAlign::Center);
                    });
                }
                Part::Ellipsis => {
                    let style = cx.style(Self::query(Slot::Ellipsis, state));
                    cx.icon(*rect, Icon::Ellipsis, &style);
                }
                Part::Previous | Part::Next => {
                    let slot = if *part == Part::Previous {
                        Slot::Previous
                    } else {
                        Slot::Next
                    };
                    let style = cx.style(Self::query(slot, state));
                    let label = self.label(*part);
                    // The chevron points toward the page it opens.
                    let back = (*part == Part::Previous) != cx.rtl;
                    let icon = if back {
                        Icon::ChevronLeft
                    } else {
                        Icon::ChevronRight
                    };
                    let (leading, trailing) = if *part == Part::Previous {
                        (Some(icon), None)
                    } else {
                        (None, Some(icon))
                    };
                    let content = crate::style::Content {
                        leading,
                        label: &label,
                        trailing,
                    };
                    cx.slot(*rect, &style, |cx, inner| {
                        content.draw(cx, inner, &style, TextAlign::Center);
                    });
                }
            }
        }
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::Activate,
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = Some((self.current + 1).to_string());
        if let Some(stop) = self.stops().get(self.focus) {
            node.description = self.label(self.parts[*stop].0);
        }
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
            SemanticAction::Activate => match self.stops().get(self.focus) {
                Some(part) => {
                    self.activate(cx, *part);
                    true
                }
                None => false,
            },
            SemanticAction::Increment => {
                self.go(cx, self.current + 1, Part::Next);
                true
            }
            SemanticAction::Decrement if self.current > 0 => {
                self.go(cx, self.current - 1, Part::Previous);
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
    }
}

impl Pagination {
    fn arrange_metrics(&mut self, cx: &MeasureContext<'_>) {
        let style = |slot| cx.style(Self::query(slot, VisualState::NONE));
        let (root, link, previous, next) = (
            style(Slot::Root),
            style(Slot::Link),
            style(Slot::Previous),
            style(Slot::Next),
        );
        let button = |style: &crate::style::ResolvedStyle, label: &str| {
            style.padding.horizontal()
                + style.icon_size
                + style.gap
                + cx.text.width(label, &style.text)
        };
        self.metrics = Metrics {
            page: link.min_width.max(link.min_height),
            previous: button(&previous, &self.previous),
            next: button(&next, &self.next),
            gap: root.gap,
        };
    }
}
