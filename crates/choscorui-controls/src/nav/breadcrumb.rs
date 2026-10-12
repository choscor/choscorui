//! A trail of links to the current page, with an optional collapsed
//! middle behind an ellipsis menu.

use crate::CommandId;
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted;
use crate::menu::{MenuItem, MenuResponse};
use crate::overlay::menu_popup::MenuPopup;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Link(usize),
    Ellipsis,
    Current(usize),
    Separator,
}

/// A breadcrumb trail. Every item but the last is a link; the last is
/// the current page. Each link is a tab stop inside the control.
pub struct Breadcrumb {
    name: String,
    items: Vec<String>,
    /// Items hidden behind the ellipsis.
    hidden: std::ops::Range<usize>,
    parts: Vec<(Part, Rect)>,
    /// The focused tab stop.
    focus: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    menu: MenuPopup,
    enabled: bool,
    rtl: bool,
}

impl Breadcrumb {
    pub fn new(name: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            name: name.into(),
            items,
            hidden: 0..0,
            parts: Vec::new(),
            focus: 0,
            hovered: None,
            pressed: None,
            menu: MenuPopup::new(Vec::new()),
            enabled: true,
            rtl: false,
        }
    }

    /// Hides the items in `range` behind an ellipsis that opens a menu of
    /// them. The first and the last item always show.
    #[must_use]
    pub fn with_collapsed(mut self, range: std::ops::Range<usize>) -> Self {
        let last = self.items.len().saturating_sub(1);
        let start = range.start.clamp(1, last.max(1));
        let end = range.end.clamp(start, last.max(start));
        self.hidden = start..end;
        let items = self
            .hidden
            .clone()
            .map(|index| MenuItem::action(self.items[index].clone(), CommandId(index as u64)));
        self.menu = MenuPopup::new(items.collect());
        self
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Breadcrumb, slot).state(state)
    }

    /// The parts in reading order.
    fn sequence(&self) -> Vec<Part> {
        let mut parts = Vec::new();
        let last = self.items.len().saturating_sub(1);
        for index in 0..self.items.len() {
            if self.hidden.contains(&index) {
                if index == self.hidden.start {
                    parts.push(Part::Ellipsis);
                    parts.push(Part::Separator);
                }
                continue;
            }
            if index == last {
                parts.push(Part::Current(index));
            } else {
                parts.push(Part::Link(index));
                parts.push(Part::Separator);
            }
        }
        parts
    }

    /// The parts that take focus, as indices into `parts`.
    fn stops(&self) -> Vec<usize> {
        self.parts
            .iter()
            .enumerate()
            .filter(|(_, (part, _))| matches!(part, Part::Link(_) | Part::Ellipsis))
            .map(|(index, _)| index)
            .collect()
    }

    fn part_at(&self, point: Point) -> Option<usize> {
        self.parts.iter().position(|(part, rect)| {
            matches!(part, Part::Link(_) | Part::Ellipsis) && rect.contains(point)
        })
    }

    fn hidden_labels(&self) -> String {
        self.items[self.hidden.clone()].join("\n")
    }

    /// Opens the menu below the ellipsis at `time`. Returns false when no
    /// item is collapsed or the trail is not arranged yet.
    fn open_menu(&mut self, time: f64) -> bool {
        let Some(rect) = self
            .parts
            .iter()
            .find(|(part, _)| *part == Part::Ellipsis)
            .map(|(_, rect)| *rect)
        else {
            return false;
        };
        let x = if self.rtl {
            rect.max_x()
        } else {
            rect.origin.x
        };
        let anchor = Point::new(x, rect.max_y() + 4.0);
        self.menu.open(anchor, time, true);
        true
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, part: usize) {
        match self.parts.get(part).map(|(part, _)| *part) {
            Some(Part::Link(index)) => cx.emit(ControlMessage::ItemActivated(index)),
            Some(Part::Ellipsis) if self.open_menu(cx.time()) => {
                cx.emit(ControlMessage::OpenChanged(true));
            }
            _ => return,
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn menu_response(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) {
        match response {
            MenuResponse::None => {}
            MenuResponse::Activated(CommandId(index)) => {
                self.menu.close(cx.time());
                cx.emit(ControlMessage::ItemActivated(index as usize));
                cx.emit(ControlMessage::OpenChanged(false));
            }
            MenuResponse::Closed => self.close_menu(cx),
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn close_menu(&mut self, cx: &mut ControlContext<'_>) {
        if self.menu.is_open() {
            self.menu.close(cx.time());
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
        }
    }

    fn width(&self, cx: &MeasureContext<'_>, part: Part) -> f64 {
        match part {
            Part::Link(index) | Part::Current(index) => {
                let slot = if matches!(part, Part::Link(_)) {
                    Slot::Link
                } else {
                    Slot::Item
                };
                let style = cx.style(Self::query(slot, VisualState::NONE));
                cx.text.width(&self.items[index], &style.text)
            }
            Part::Ellipsis => {
                cx.style(Self::query(Slot::Ellipsis, VisualState::NONE))
                    .min_width
            }
            Part::Separator => {
                cx.style(Self::query(Slot::Separator, VisualState::NONE))
                    .icon_size
            }
        }
    }
}

impl CustomControl for Breadcrumb {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let ellipsis = cx.style(Self::query(Slot::Ellipsis, VisualState::NONE));
        let parts = self.sequence();
        let widths: f64 = parts.iter().map(|part| self.width(cx, *part)).sum();
        let gaps = root.gap * parts.len().saturating_sub(1) as f64;
        Measured {
            size: Size::new(
                widths + gaps,
                root.text.line_height.max(ellipsis.min_height),
            ),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        self.rtl = cx.rtl();
        let mut x = if self.rtl {
            bounds.max_x()
        } else {
            bounds.origin.x
        };
        self.parts = self
            .sequence()
            .into_iter()
            .map(|part| {
                let width = self.width(cx, part);
                let left = if self.rtl { x - width } else { x };
                x = if self.rtl {
                    x - width - root.gap
                } else {
                    x + width + root.gap
                };
                (
                    part,
                    Rect::from_xywh(left, bounds.origin.y, width, bounds.size.height),
                )
            })
            .collect();
        let content = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Content));
        self.menu.arrange(cx, content.min_width);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if self.menu.is_open() {
            match event {
                ControlEvent::Pointer(pointer) => {
                    let released = matches!(pointer, PointerEvent::Up { .. });
                    if let Some(response) = self.menu.pointer(pointer.position(), released) {
                        if released {
                            self.menu_response(cx, response);
                        } else {
                            cx.request_paint();
                        }
                        return EventReply::Stop;
                    }
                }
                ControlEvent::Key(KeyEvent::Down { key: Key::Tab, .. })
                | ControlEvent::Dismiss
                | ControlEvent::FocusChanged(false) => self.close_menu(cx),
                ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                    let response = self.menu.menu.handle_key(*key, &self.menu.commands);
                    self.menu_response(cx, response);
                    return EventReply::Stop;
                }
                _ => {}
            }
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
            ControlEvent::FocusChanged(true) => {
                // Shift+Tab arrives at the last link.
                self.focus = if cx.modifiers().shift {
                    stops.len().saturating_sub(1)
                } else {
                    0
                };
                cx.request_paint();
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
                .set(VisualState::DISABLED, host.disabled());
            match part {
                Part::Link(item) => {
                    let style = cx.style(Self::query(Slot::Link, state));
                    let label = &self.items[*item];
                    cx.slot(*rect, &style, |cx, inner| {
                        cx.label(inner, label, &style, TextAlign::Start);
                    });
                }
                Part::Current(item) => {
                    let style = cx.style(Self::query(Slot::Item, state));
                    cx.label(*rect, &self.items[*item], &style, TextAlign::Start);
                }
                Part::Separator => {
                    let style = cx.style(Self::query(Slot::Separator, state));
                    let icon = if cx.rtl {
                        Icon::ChevronLeft
                    } else {
                        Icon::ChevronRight
                    };
                    cx.icon(*rect, icon, &style);
                }
                Part::Ellipsis => {
                    let style = cx.style(Self::query(Slot::Ellipsis, state));
                    let box_rect = Rect::from_xywh(
                        rect.origin.x,
                        rect.center().y - style.min_height / 2.0,
                        rect.size.width,
                        style.min_height,
                    );
                    cx.slot(box_rect, &style, |cx, inner| {
                        cx.icon(inner, Icon::Ellipsis, &style);
                    });
                }
            }
        }
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

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = self.items.last().cloned();
        node.description = match self.stops().get(self.focus).map(|stop| self.parts[*stop].0) {
            Some(Part::Link(index)) => self.items[index].clone(),
            Some(Part::Ellipsis) => self.hidden_labels(),
            _ => String::new(),
        };
        node.state.expanded = (!self.hidden.is_empty()).then_some(self.menu.is_open());
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
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        self.items.len() > 1
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.menu.reveal = Default::default();
        }
    }

    /// Opens the ellipsis menu of the collapsed items.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !open {
            if self.hidden.is_empty() {
                return false;
            }
            self.menu.close(time);
            return true;
        }
        self.enabled && (self.menu.is_open() || self.open_menu(time))
    }
}
