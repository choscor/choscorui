//! A virtualized scroll area of chat messages that sticks to the bottom.

use super::message::{Message, MessageLayout};
use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Localizer, Measured, PointerEvent, Rect, SemanticAction,
    SemanticCollection, SemanticNode, SemanticParts, SemanticRole, Size, WheelUnit,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The height that a message has before it is measured.
const ESTIMATE: f64 = 64.0;

/// How far an arrow key scrolls, in points.
const LINE: f64 = 40.0;

/// The handle that the app adds messages through. Clones share one
/// conversation: the queue, the shown messages, and the scroll position.
/// A [`MessageScroller`] made with the handle shows queued messages at
/// the next [`ControlHost::advance`](crate::ControlHost::advance). A new
/// scroller with the same handle, such as after a view builds again,
/// keeps the messages, the offset, and whether it sticks to the bottom.
#[derive(Clone, Debug, Default)]
pub struct Conversation(Rc<RefCell<Thread>>);

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `message` at the end of the conversation.
    pub fn push(&self, message: Message) {
        self.0.borrow_mut().pending.push(message);
    }

    /// The count of messages, shown and queued.
    pub fn len(&self) -> usize {
        let thread = self.0.borrow();
        thread.messages.len() + thread.pending.len()
    }

    /// True when the conversation has no messages.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn pending(&self) -> bool {
        !self.0.borrow().pending.is_empty()
    }
}

/// The messages of a conversation and where its scrollers show them.
#[derive(Debug)]
struct Thread {
    pending: Vec<Message>,
    messages: Vec<Message>,
    heights: Heights,
    generation: u64,
    width: f64,
    /// The space between messages. Half of it is above the first message
    /// and below the last one.
    gap: f64,
    offset: f64,
    stuck: bool,
}

impl Default for Thread {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            messages: Vec::new(),
            heights: Heights::default(),
            generation: 1,
            width: 0.0,
            gap: 0.0,
            offset: 0.0,
            stuck: true,
        }
    }
}

impl Thread {
    fn bottom(&self, view: f64) -> f64 {
        (self.heights.total() - view).max(0.0)
    }

    /// The messages in a view `view` tall, as a range of indices.
    fn visible(&self, view: f64) -> std::ops::Range<usize> {
        if self.messages.is_empty() {
            return 0..0;
        }
        let first = self.heights.at(self.offset);
        let last = self.heights.at(self.offset + view).max(first);
        first..last + 1
    }

    /// The top of message `index` in content coordinates.
    fn top(&self, index: usize) -> f64 {
        self.heights.before(index) + self.gap / 2.0
    }

    /// Measures the messages in view and keeps the offset in range. Each
    /// pass measures only messages in view, so it does not scale with
    /// the conversation. A view that does not stick to the bottom keeps
    /// its first measured message in place while the messages that come
    /// into view above it get their heights.
    fn settle(&mut self, cx: &MeasureContext<'_>, view: f64) {
        let anchor = (!self.stuck).then(|| {
            let visible = self.visible(view);
            let first = visible
                .clone()
                .find(|index| self.heights.measured[*index] == self.generation)
                .unwrap_or(visible.start);
            (first, self.offset - self.heights.before(first))
        });
        let place = |thread: &mut Self| {
            if let Some((first, within)) = anchor {
                thread.offset = thread.heights.before(first) + within;
            }
            let bottom = thread.bottom(view);
            thread.offset = if thread.stuck {
                bottom
            } else {
                thread.offset.clamp(0.0, bottom)
            };
        };
        for _ in 0..4 {
            place(self);
            let mut changed = false;
            for index in self.visible(view) {
                if self.heights.measured[index] == self.generation {
                    continue;
                }
                let rect = Rect::from_xywh(0.0, 0.0, self.width, 0.0);
                let layout = self.messages[index].layout(cx, rect);
                self.heights.set(index, layout.height + self.gap);
                self.heights.measured[index] = self.generation;
                changed = true;
            }
            if !changed {
                break;
            }
        }
        place(self);
    }

    /// Moves to `offset`, clamped. Returns false when the offset stays.
    fn scroll_to(&mut self, offset: f64, view: f64) -> bool {
        let bottom = self.bottom(view);
        let offset = offset.clamp(0.0, bottom);
        self.stuck = offset >= bottom - 0.5;
        let moved = offset != self.offset;
        self.offset = offset;
        moved
    }

    fn receive(&mut self) {
        for message in std::mem::take(&mut self.pending) {
            self.messages.push(message);
            self.heights.push(ESTIMATE);
        }
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
/// Command+ArrowUp and Command+ArrowDown go to the top and the bottom.
pub struct MessageScroller {
    name: String,
    conversation: Conversation,
    viewport: Rect,
    enabled: bool,
    /// The messages in view and where they are, from the last arrange.
    shown: Vec<(usize, MessageLayout)>,
    /// The message and the bubble under the pointer.
    hovered: Option<(usize, usize)>,
    /// The overlay scroll bar at the trailing edge.
    bar: crate::OverlayScrollBar,
}

impl MessageScroller {
    pub fn new(name: impl Into<String>, conversation: Conversation) -> Self {
        Self {
            name: name.into(),
            conversation,
            viewport: Rect::default(),
            enabled: true,
            shown: Vec::new(),
            hovered: None,
            bar: crate::OverlayScrollBar::new(crate::ScrollOrientation::Vertical),
        }
    }

    /// Moves to `offset`, clamped. Returns false when the offset stays.
    fn scroll_to(&mut self, cx: &mut ControlContext<'_>, offset: f64) -> bool {
        let view = self.viewport.size.height;
        let moved = self.conversation.0.borrow_mut().scroll_to(offset, view);
        if moved {
            self.hovered = None;
            cx.request_layout();
            cx.request_paint();
        }
        moved
    }

    /// The content height and the offset, for the scroll bar.
    fn scroll(&self) -> (f64, f64) {
        let thread = self.conversation.0.borrow();
        (thread.heights.total(), thread.offset)
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
        self.viewport = bounds;
        let mut thread = self.conversation.0.borrow_mut();
        if bounds.size.width != thread.width || viewport.gap != thread.gap {
            // Every height is now stale; messages are measured again when
            // they come into view.
            thread.generation += 1;
            thread.width = bounds.size.width;
            thread.gap = viewport.gap;
        }
        let view = bounds.size.height;
        thread.settle(cx, view);
        self.shown = thread
            .visible(view)
            .map(|index| {
                let top = bounds.origin.y + thread.top(index) - thread.offset;
                let rect = Rect::from_xywh(bounds.origin.x, top, bounds.size.width, 0.0);
                (index, thread.messages[index].layout(cx, rect))
            })
            .collect();
        drop(thread);
        self.bar.arrange(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if let ControlEvent::Timer = event {
            self.conversation.0.borrow_mut().receive();
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
            return EventReply::Stop;
        }
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        // The scroll bar takes the pointer before the messages under it.
        if let ControlEvent::Pointer(pointer) = event {
            let (total, offset) = self.scroll();
            self.bar.set_scroll(total, offset);
            if let Some(scrolled) = self.bar.hosted_pointer(cx, pointer) {
                if scrolled {
                    self.scroll_to(cx, self.bar.offset());
                }
                return EventReply::Stop;
            }
        }
        let offset = self.scroll().1;
        match event {
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = self.shown.iter().find_map(|(index, layout)| {
                    Message::bubble_at(layout, *position).map(|bubble| (*index, bubble))
                });
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) | ControlEvent::Dismiss => {
                if self.bar.release() | self.hovered.take().is_some() {
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Wheel { delta, unit, .. }) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => LINE,
                };
                // At a limit the wheel goes on to an outer scroll view.
                if delta.y != 0.0 && self.scroll_to(cx, offset + delta.y * scale) {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let page = self.viewport.size.height * 0.9;
                let modifiers = cx.modifiers();
                let target = match key {
                    Key::ArrowUp if modifiers.command => 0.0,
                    Key::ArrowDown if modifiers.command => f64::INFINITY,
                    Key::ArrowUp => offset - LINE,
                    Key::ArrowDown => offset + LINE,
                    Key::Space if modifiers.shift => offset - page,
                    Key::PageUp => offset - page,
                    Key::PageDown | Key::Space => offset + page,
                    Key::Home => 0.0,
                    Key::End => f64::INFINITY,
                    _ => return EventReply::Continue,
                };
                self.scroll_to(cx, target);
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
        let thread = self.conversation.0.borrow();
        cx.list.push_clip(viewport);
        for (index, layout) in &self.shown {
            let hovered = self
                .hovered
                .filter(|(message, _)| message == index)
                .map(|(_, bubble)| bubble);
            if let Some(message) = thread.messages.get(*index) {
                message.paint_layout(cx, layout, hovered);
            }
        }
        let _ = cx.list.pop_clip();
        let mut bar = self.bar.clone();
        bar.set_scroll(thread.heights.total(), thread.offset);
        bar.paint(cx);
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
        node.state.disabled = !self.enabled;
        node
    }

    /// A Group part for each message in view, with its index among all
    /// messages, and its bubbles under it. The collection counts every
    /// message.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        let thread = self.conversation.0.borrow();
        parts.set_collection(SemanticCollection {
            item_count: thread.messages.len(),
            column_count: None,
        });
        for (index, layout) in &self.shown {
            let Some(message) = thread.messages.get(*index) else {
                continue;
            };
            let bounds = layout.bubbles.iter().fold(
                layout.avatar.union(layout.header).union(layout.footer),
                |all, bubble| all.union(bubble.frame),
            );
            let part = parts.push(None, SemanticRole::Group, "", bounds);
            part.node = message.node();
            part.item(*index);
            let parent = Some(parts.len() - 1);
            message.push_parts(layout, parts, parent);
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let page = self.viewport.size.height * 0.9;
        let offset = self.scroll().1;
        match action {
            SemanticAction::Increment => {
                self.scroll_to(cx, offset + page);
            }
            SemanticAction::Decrement => {
                self.scroll_to(cx, offset - page);
            }
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
