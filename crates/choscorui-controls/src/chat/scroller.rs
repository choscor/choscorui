//! A virtualized scroll area of chat messages that sticks to the bottom.

use super::message::Message;
use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    Constraints, EventPhase, EventReply, Key, KeyEvent, LayoutDirection, Measured, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, WheelUnit,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The height that a message has before it is measured.
const ESTIMATE: f64 = 64.0;

/// How far an arrow key scrolls, in points.
const LINE: f64 = 40.0;

/// The handle that the app adds messages through. Clones share one
/// queue. A [`MessageScroller`] made with the handle shows the messages
/// at the next [`ControlHost::advance`](crate::ControlHost::advance).
#[derive(Clone, Debug, Default)]
pub struct Conversation(Rc<RefCell<Vec<Message>>>);

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `message` at the end of the conversation.
    pub fn push(&self, message: Message) {
        self.0.borrow_mut().push(message);
    }

    fn pending(&self) -> bool {
        !self.0.borrow().is_empty()
    }

    fn take(&self) -> Vec<Message> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

/// Message heights in a Fenwick tree, so that finding a message by its
/// offset and changing one height take logarithmic time.
#[derive(Debug, Default)]
struct Heights {
    tree: Vec<f64>,
    values: Vec<f64>,
    /// The width generation that each height was measured for.
    measured: Vec<u64>,
}

impl Heights {
    fn len(&self) -> usize {
        self.values.len()
    }

    fn push(&mut self, height: f64) {
        let index = self.values.len();
        self.values.push(0.0);
        self.measured.push(0);
        self.tree.push(0.0);
        // The new node covers the range that ends at it; gather the
        // nodes inside that range.
        let one = index + 1;
        let low = one & one.wrapping_neg();
        let mut sum = 0.0;
        let mut child = 1;
        while child < low {
            sum += self.tree[index - child];
            child <<= 1;
        }
        self.tree[index] = sum;
        self.set(index, height);
    }

    fn set(&mut self, index: usize, height: f64) {
        let delta = height - self.values[index];
        self.values[index] = height;
        let mut one = index + 1;
        while one <= self.tree.len() {
            self.tree[one - 1] += delta;
            one += one & one.wrapping_neg();
        }
    }

    /// The total height of the messages before `index`.
    fn before(&self, index: usize) -> f64 {
        let mut one = index;
        let mut sum = 0.0;
        while one > 0 {
            sum += self.tree[one - 1];
            one -= one & one.wrapping_neg();
        }
        sum
    }

    fn total(&self) -> f64 {
        self.before(self.len())
    }

    /// The message at `offset` from the top, or the last one.
    fn at(&self, offset: f64) -> usize {
        let mut index = 0;
        let mut left = offset;
        let mut step = self.tree.len().next_power_of_two();
        while step > 0 {
            let next = index + step;
            if next <= self.tree.len() && self.tree[next - 1] <= left {
                index = next;
                left -= self.tree[next - 1];
            }
            step >>= 1;
        }
        index.min(self.len().saturating_sub(1))
    }
}

/// A scroll area of messages. Only the messages in view are measured
/// and painted. It sticks to the bottom: while it shows the last message,
/// new messages and size changes keep the last message in view. The
/// wheel and the arrow, Page Up, Page Down, Home, and End keys scroll it.
pub struct MessageScroller {
    name: String,
    conversation: Conversation,
    messages: Vec<Message>,
    heights: Heights,
    generation: u64,
    width: f64,
    gap: f64,
    viewport: Rect,
    offset: f64,
    stuck: bool,
    rtl: bool,
    enabled: bool,
}

impl MessageScroller {
    pub fn new(name: impl Into<String>, conversation: Conversation) -> Self {
        Self {
            name: name.into(),
            conversation,
            messages: Vec::new(),
            heights: Heights::default(),
            generation: 1,
            width: 0.0,
            gap: 0.0,
            viewport: Rect::default(),
            offset: 0.0,
            stuck: true,
            rtl: false,
            enabled: true,
        }
    }

    fn bottom(&self) -> f64 {
        (self.heights.total() - self.viewport.size.height).max(0.0)
    }

    /// The messages in view, as a range of indices.
    fn visible(&self) -> std::ops::Range<usize> {
        if self.messages.is_empty() {
            return 0..0;
        }
        let first = self.heights.at(self.offset);
        let last = self
            .heights
            .at(self.offset + self.viewport.size.height)
            .max(first);
        first..last + 1
    }

    /// Measures the messages in view and keeps the offset in range. Each
    /// pass measures only messages in view, so it does not scale with
    /// the conversation.
    fn settle(&mut self, cx: &MeasureContext<'_>) {
        for _ in 0..4 {
            if self.stuck {
                self.offset = self.bottom();
            }
            self.offset = self.offset.clamp(0.0, self.bottom());
            let mut changed = false;
            for index in self.visible() {
                if self.heights.measured[index] == self.generation {
                    continue;
                }
                let layout =
                    self.messages[index].layout(cx, Rect::from_xywh(0.0, 0.0, self.width, 0.0));
                self.heights.set(index, layout.height + self.gap);
                self.heights.measured[index] = self.generation;
                changed = true;
            }
            if !changed {
                break;
            }
        }
        if self.stuck {
            self.offset = self.bottom();
        }
    }

    fn scroll_to(&mut self, cx: &mut ControlContext<'_>, offset: f64) {
        let bottom = self.bottom();
        let offset = offset.clamp(0.0, bottom);
        self.stuck = offset >= bottom - 0.5;
        if offset != self.offset {
            self.offset = offset;
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn receive(&mut self, cx: &mut ControlContext<'_>) {
        for message in self.conversation.take() {
            self.messages.push(message);
            self.heights.push(ESTIMATE);
        }
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }
}

impl CustomControl for MessageScroller {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max;
        // An unbounded scroller is a medium container wide and as tall
        // as a small one.
        let width = if max.width.is_finite() {
            max.width
        } else {
            cx.tokens.spacing * 120.0
        };
        let height = if max.height.is_finite() {
            max.height
        } else {
            cx.tokens.spacing * 96.0
        };
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let viewport = cx.style(StyleQuery::new(
            ControlKind::MessageScroller,
            Slot::Viewport,
        ));
        self.rtl = cx.rtl();
        if bounds.size.width != self.width || viewport.gap != self.gap {
            // Every height is now stale; messages are measured again when
            // they come into view.
            self.generation += 1;
            self.width = bounds.size.width;
            self.gap = viewport.gap;
        }
        self.viewport = bounds;
        self.settle(cx);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if let ControlEvent::Timer = event {
            self.receive(cx);
            return EventReply::Stop;
        }
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Wheel { delta, unit, .. }) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => LINE,
                };
                if delta.y == 0.0 {
                    return EventReply::Continue;
                }
                self.scroll_to(cx, self.offset + delta.y * scale);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let page = self.viewport.size.height * 0.9;
                let offset = match key {
                    Key::ArrowUp => self.offset - LINE,
                    Key::ArrowDown => self.offset + LINE,
                    Key::PageUp => self.offset - page,
                    Key::PageDown => self.offset + page,
                    Key::Home => 0.0,
                    Key::End => f64::INFINITY,
                    _ => return EventReply::Continue,
                };
                self.scroll_to(cx, offset);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let viewport = self.viewport;
        let measure = MeasureContext {
            constraints: Constraints {
                min: Size::new(0.0, 0.0),
                max: viewport.size,
            },
            viewport,
            direction: if self.rtl {
                LayoutDirection::Rtl
            } else {
                LayoutDirection::Ltr
            },
            tokens: cx.tokens,
            styles: cx.styles,
            text: cx.text,
        };
        let mut layouts = Vec::new();
        for index in self.visible() {
            let top = viewport.origin.y + self.heights.before(index) - self.offset;
            let rect = Rect::from_xywh(viewport.origin.x, top, viewport.size.width, 0.0);
            layouts.push((index, self.messages[index].layout(&measure, rect)));
        }
        cx.list.push_clip(viewport);
        for (index, layout) in &layouts {
            self.messages[*index].paint_layout(cx, layout, None);
        }
        let _ = cx.list.pop_clip();
        if cx.state.focus_visible() {
            let style =
                cx.style(StyleQuery::new(ControlKind::ScrollArea, Slot::Root).state(cx.state));
            cx.slot(viewport, &style, |_, _| {});
        }
    }

    fn deadline(&self) -> Option<f64> {
        self.conversation.pending().then_some(f64::NEG_INFINITY)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::ScrollArea, &self.name, actions);
        node.value = self
            .messages
            .last()
            .map(|message| format!("{}: {}", message.author(), message.text()));
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let page = self.viewport.size.height * 0.9;
        match action {
            SemanticAction::Increment => self.scroll_to(cx, self.offset + page),
            SemanticAction::Decrement => self.scroll_to(cx, self.offset - page),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::Heights;

    #[test]
    fn heights_find_messages_by_offset() {
        let mut heights = Heights::default();
        for height in [10.0, 20.0, 30.0, 40.0, 50.0] {
            heights.push(height);
        }
        assert_eq!(heights.total(), 150.0);
        assert_eq!(heights.before(3), 60.0);
        assert_eq!(heights.at(0.0), 0);
        assert_eq!(heights.at(29.9), 1);
        assert_eq!(heights.at(30.0), 2);
        assert_eq!(heights.at(149.0), 4);
        assert_eq!(heights.at(500.0), 4);
        heights.set(1, 5.0);
        assert_eq!(heights.before(3), 45.0);
        assert_eq!(heights.at(15.0), 2);
    }
}
