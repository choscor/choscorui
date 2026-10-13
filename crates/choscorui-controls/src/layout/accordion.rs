//! Stacked sections whose content opens under their headers.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Reveal};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, Transform,
};

/// An accordion. Each item has a header and content text. A single
/// accordion keeps at most one item open; `multiple()` lets items open
/// together. Each header is a tab stop, and ArrowDown, ArrowUp, Home, and
/// End move between the headers. Enter, Space, or a click toggles an item
/// and emits `Changed(Indices(open))`.
pub struct Accordion {
    name: String,
    items: Vec<(String, String)>,
    multiple: bool,
    reveals: Vec<Reveal>,
    focus: usize,
    pressed: Option<usize>,
    hovered: Option<usize>,
    /// Each item's header and content rectangles.
    rows: Vec<(Rect, Rect)>,
    motion_ms: u32,
    enabled: bool,
    rtl: bool,
}

impl Accordion {
    pub fn new(name: impl Into<String>, items: Vec<(String, String)>) -> Self {
        let count = items.len();
        Self {
            name: name.into(),
            items,
            multiple: false,
            reveals: vec![Reveal::default(); count],
            focus: 0,
            pressed: None,
            hovered: None,
            rows: Vec::new(),
            motion_ms: 0,
            enabled: true,
            rtl: false,
        }
    }

    /// Lets several items open at once.
    #[must_use]
    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }

    /// Opens `index` at the start.
    #[must_use]
    pub fn with_open(mut self, index: usize) -> Self {
        if let Some(reveal) = self.reveals.get_mut(index) {
            reveal.open = true;
        }
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Accordion, slot)
    }

    fn open(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|index| self.reveals[*index].open)
            .collect()
    }

    fn toggle(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        let time = cx.time();
        let open = !self.reveals[index].open;
        if open && !self.multiple {
            for reveal in &mut self.reveals {
                reveal.set(false, time);
            }
        }
        self.reveals[index].set(open, time);
        cx.emit(ControlMessage::Changed(ControlValue::Indices(self.open())));
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    /// The header height of `index` in `width`: the title wraps beside
    /// the chevron.
    fn header_height(&self, cx: &MeasureContext<'_>, index: usize, width: f64) -> f64 {
        let trigger = cx.style(Self::query(Slot::Trigger));
        let room = (width - trigger.padding.horizontal() - trigger.icon_size).max(0.0);
        let title = wrapped_height(&self.items[index].0, &trigger.text, room, cx.text);
        title.max(trigger.text.line_height) + trigger.padding.vertical()
    }

    /// The content height of `index` when open, with its bottom padding.
    fn content_height(&self, cx: &MeasureContext<'_>, index: usize, width: f64) -> f64 {
        if !self.reveals[index].open {
            return 0.0;
        }
        let content = cx.style(Self::query(Slot::Content));
        wrapped_height(&self.items[index].1, &content.text, width, cx.text)
            + content.padding.block_end
    }

    fn header_at(&self, point: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|(header, _)| header.contains(point))
    }
}

impl CustomControl for Accordion {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let item = cx.style(Self::query(Slot::Item));
        let max = cx.constraints.max.width;
        // An unbounded accordion takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let height: f64 = (0..self.items.len())
            .map(|index| {
                self.header_height(cx, index, width)
                    + self.content_height(cx, index, width)
                    + item.border_width
            })
            .sum();
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let item = cx.style(Self::query(Slot::Item));
        let content = cx.style(Self::query(Slot::Content));
        self.rtl = cx.rtl();
        self.motion_ms = content.motion_ms;
        let width = bounds.size.width;
        let mut y = bounds.origin.y;
        self.rows = (0..self.items.len())
            .map(|index| {
                let header = self.header_height(cx, index, width);
                let head = Rect::from_xywh(bounds.origin.x, y, width, header);
                let height = self.content_height(cx, index, width);
                let body = Rect::from_xywh(bounds.origin.x, head.max_y(), width, height);
                y = body.max_y() + item.border_width;
                (head, body)
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let count = self.items.len();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(index) = self.header_at(*position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(index);
                self.focus = index;
                cx.request_focus();
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = self.header_at(*position);
                if let Some(index) = self.pressed.take().filter(|index| hit == Some(*index)) {
                    self.toggle(cx, index);
                }
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = self.header_at(*position);
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
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => match *key {
                Key::Tab => match hosted::tab_stop(self.focus, count, cx.modifiers().shift) {
                    Some(next) => {
                        self.focus = next;
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                },
                Key::Enter | Key::Space if !repeat => {
                    if self.focus < count {
                        self.toggle(cx, self.focus);
                    }
                    EventReply::Stop
                }
                key => match hosted::rove(key, self.focus, count, false, self.rtl) {
                    Some(next) => {
                        self.focus = next;
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                },
            },
            ControlEvent::FocusChanged(focused) => {
                if *focused && self.pressed.is_none() {
                    self.focus = if cx.modifiers().shift {
                        count.saturating_sub(1)
                    } else {
                        0
                    };
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let item = cx.style(Self::query(Slot::Item));
        let content = cx.style(Self::query(Slot::Content));
        let icon_style = cx.style(Self::query(Slot::Icon));
        for (index, ((title, text), (head, body))) in self.items.iter().zip(&self.rows).enumerate()
        {
            let state = VisualState::NONE
                .set(VisualState::HOVERED, self.hovered == Some(index))
                .set(VisualState::PRESSED, self.pressed == Some(index))
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && self.focus == index,
                )
                .set(VisualState::OPEN, self.reveals[index].open)
                .set(VisualState::DISABLED, host.disabled());
            let trigger = cx.style(Self::query(Slot::Trigger).state(state));
            let row = Rect::from_xywh(
                head.origin.x,
                head.origin.y,
                head.size.width,
                body.max_y() - head.origin.y + item.border_width,
            );
            cx.slot(row, &item, |_, _| {});
            cx.slot(*head, &trigger, |cx, inner| {
                let icon = trigger.icon_size;
                let label = Rect::from_xywh(
                    if cx.rtl {
                        inner.origin.x + icon
                    } else {
                        inner.origin.x
                    },
                    inner.origin.y,
                    (inner.size.width - icon).max(0.0),
                    inner.size.height,
                );
                paint_wrapped(cx, label, title, &trigger, TextAlign::Start);
                let x = if cx.rtl {
                    inner.origin.x
                } else {
                    inner.max_x() - icon
                };
                // The chevron stays beside the first title line.
                let line = trigger.text.line_height.min(inner.size.height);
                let mark = Rect::from_xywh(x, inner.origin.y, icon, line);
                // The chevron turns over while the item opens.
                let progress = self.reveals[index].progress(cx.tokens, self.motion_ms, cx.time);
                let center = mark.center();
                let turn = Transform::translate(-center.x, -center.y)
                    .then(Transform::rotate(progress * std::f64::consts::PI))
                    .then(Transform::translate(center.x, center.y));
                let pushed = cx.list.push_transform(turn).is_ok();
                cx.icon(mark, Icon::ChevronDown, &icon_style);
                if pushed {
                    let _ = cx.list.pop_transform();
                }
            });
            if body.size.height > 0.0 {
                let progress = self.reveals[index].progress(cx.tokens, self.motion_ms, cx.time);
                let faded =
                    progress < 1.0 && cx.list.push_opacity(progress.clamp(0.0, 1.0)).is_ok();
                let text_rect = Rect::from_xywh(
                    body.origin.x,
                    body.origin.y,
                    body.size.width,
                    body.size.height - content.padding.block_end,
                );
                paint_wrapped(cx, text_rect, text, &content, TextAlign::Start);
                if faded {
                    let _ = cx.list.pop_opacity();
                }
            }
        }
    }

    fn animating(&self, time: f64) -> bool {
        self.reveals
            .iter()
            .any(|reveal| reveal.animating(self.motion_ms, time))
    }

    fn semantics(&self) -> SemanticNode {
        let mut actions = Vec::new();
        if self.enabled && !self.items.is_empty() {
            actions.extend([SemanticAction::Activate, SemanticAction::Focus]);
            actions.push(if self.reveals[self.focus].open {
                SemanticAction::Collapse
            } else {
                SemanticAction::Expand
            });
        }
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        let open: Vec<&str> = self
            .open()
            .into_iter()
            .map(|index| self.items[index].0.as_str())
            .collect();
        node.value = (!open.is_empty()).then(|| open.join(", "));
        node.description = self
            .items
            .get(self.focus)
            .map(|(title, _)| title.clone())
            .unwrap_or_default();
        node.state.expanded = self.reveals.get(self.focus).map(|reveal| reveal.open);
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let Some(open) = self.reveals.get(self.focus).map(|reveal| reveal.open) else {
            return false;
        };
        match action {
            SemanticAction::Activate => self.toggle(cx, self.focus),
            SemanticAction::Expand if !open => self.toggle(cx, self.focus),
            SemanticAction::Collapse if open => self.toggle(cx, self.focus),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn focusable(&self) -> bool {
        !self.items.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Takes `Indices`, the open items, and shows them without an
    /// animation. Rejects an index past the items, and more than one index
    /// in a single accordion.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(indices) = value else {
            return false;
        };
        let fits = indices.iter().all(|index| *index < self.items.len())
            && (self.multiple || indices.len() <= 1);
        if fits {
            for (index, reveal) in self.reveals.iter_mut().enumerate() {
                *reveal = Reveal {
                    open: indices.contains(&index),
                    ..Reveal::default()
                };
            }
        }
        fits
    }
}
