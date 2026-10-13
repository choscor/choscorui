//! An overlay scroll bar along one edge of a viewport whose scroll offset
//! another layer keeps, such as a layout engine.

use super::{ScrollBar, ScrollBarInput, ScrollOrientation};
use crate::custom::{ControlContext, MeasureContext};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, VisualState};
use choscorui_core::{EventReply, Point, PointerButton, PointerEvent, Rect, WheelUnit};

/// The content must be longer than the viewport by more than this, in
/// points, before the bar shows. Rounding in layout stays below it.
const OVERFLOW: f64 = 0.5;

/// What one pointer event did to an [`OverlayScrollBar`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverlayScrollReply {
    /// The bar used the event, so the content under it must not get it.
    pub consumed: bool,
    /// The offset changed. Read it with [`OverlayScrollBar::offset`].
    pub scrolled: bool,
    /// The hover or press state of the thumb changed.
    pub repaint: bool,
}

/// One overlay scroll bar of a scrolling viewport. A [`ScrollBar`] gives
/// the thumb geometry, the track paging, and the thumb drag. This type
/// adds the bar position at the viewport edge, the thumb hover, and the
/// `ScrollArea` look. All work is constant in the content size.
///
/// A vertical bar sits at the trailing edge: the right edge, or the left
/// edge in a right-to-left layout. A horizontal bar sits at the bottom.
/// The bar is as thick as the `ScrollArea` `Scrollbar` slot.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayScrollBar {
    bar: ScrollBar,
    viewport: Rect,
    thickness: f64,
    rtl: bool,
    hovered: bool,
}

impl OverlayScrollBar {
    pub fn new(orientation: ScrollOrientation) -> Self {
        Self {
            bar: ScrollBar::new("", orientation),
            viewport: Rect::default(),
            thickness: 0.0,
            rtl: false,
            hovered: false,
        }
    }

    pub fn orientation(&self) -> ScrollOrientation {
        self.bar.orientation()
    }

    /// Records the viewport bounds, the direction, and the bar thickness
    /// from the `ScrollArea` recipe. Call it from `CustomControl::arrange`.
    pub fn arrange(&mut self, cx: &MeasureContext<'_>, viewport: Rect) {
        let bar = cx.style(StyleQuery::new(ControlKind::ScrollArea, Slot::Scrollbar));
        self.thickness = match self.orientation() {
            ScrollOrientation::Vertical => bar.min_width,
            ScrollOrientation::Horizontal => bar.min_height,
        };
        self.viewport = viewport;
        self.rtl = cx.rtl();
        let (content, offset) = (self.bar.content, self.bar.value());
        self.set_scroll(content, offset);
    }

    /// Sets the content length and the offset along the bar. The offset
    /// clamps to `0..=maximum`.
    pub fn set_scroll(&mut self, content: f64, offset: f64) {
        let length = self.length();
        self.bar.set_range(content, length, length);
        self.bar.set_value(offset);
    }

    /// The offset along the bar.
    pub fn offset(&self) -> f64 {
        self.bar.value()
    }

    /// The largest offset: the content length less the viewport length.
    pub fn maximum(&self) -> f64 {
        self.bar.maximum()
    }

    /// The viewport length along the bar.
    pub fn length(&self) -> f64 {
        match self.orientation() {
            ScrollOrientation::Vertical => self.viewport.size.height,
            ScrollOrientation::Horizontal => self.viewport.size.width,
        }
    }

    /// True when the content is longer than the viewport.
    pub fn is_needed(&self) -> bool {
        self.length() > 0.0 && self.bar.content > self.length() + OVERFLOW
    }

    /// True while the thumb follows a drag.
    pub fn dragging(&self) -> bool {
        self.bar.drag.is_some()
    }

    /// The bar along its viewport edge, or `None` when it is not needed.
    pub fn track(&self) -> Option<Rect> {
        if !self.is_needed() {
            return None;
        }
        let (view, t) = (self.viewport, self.thickness);
        Some(match self.orientation() {
            ScrollOrientation::Vertical => {
                let x = if self.rtl {
                    view.min_x()
                } else {
                    view.max_x() - t
                };
                Rect::from_xywh(x, view.min_y(), t, view.size.height)
            }
            ScrollOrientation::Horizontal => {
                Rect::from_xywh(view.min_x(), view.max_y() - t, view.size.width, t)
            }
        })
    }

    /// The thumb across the full bar thickness, or `None` when the bar is
    /// not needed. The painted thumb is inset by the bar padding.
    pub fn thumb(&self) -> Option<Rect> {
        self.track().map(|track| self.bar.thumb_rect(track))
    }

    /// The offset of `point` along the bar from the track start.
    fn along(&self, track: Rect, point: Point) -> f64 {
        match self.orientation() {
            ScrollOrientation::Vertical => point.y - track.min_y(),
            ScrollOrientation::Horizontal => point.x - track.min_x(),
        }
    }

    /// Handles one pointer event in the viewport's coordinates. A primary
    /// press on the thumb starts a drag, and a press on the track pages one
    /// viewport toward the press. A move updates the thumb hover and
    /// follows a drag. A primary release ends the drag. The caller holds
    /// the pointer capture while [`OverlayScrollBar::dragging`] is true.
    pub fn pointer(&mut self, event: &PointerEvent) -> OverlayScrollReply {
        let mut reply = OverlayScrollReply::default();
        let Some(track) = self.track() else {
            reply.repaint = self.release();
            return reply;
        };
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if track.contains(position) => {
                let before = self.dragging();
                reply.scrolled = self.bar.handle(ScrollBarInput::Press {
                    at: self.along(track, position),
                });
                reply.consumed = true;
                reply.repaint = reply.scrolled || before != self.dragging();
            }
            PointerEvent::Move { position } => {
                let hovered = self.thumb().is_some_and(|thumb| thumb.contains(position));
                reply.repaint = hovered != self.hovered;
                self.hovered = hovered;
                if self.dragging() {
                    reply.scrolled = self.bar.handle(ScrollBarInput::Drag {
                        at: self.along(track, position),
                    });
                    reply.consumed = true;
                }
            }
            PointerEvent::Up {
                button: PointerButton::Primary,
                ..
            } if self.dragging() => {
                self.bar.handle(ScrollBarInput::Release);
                reply.consumed = true;
                reply.repaint = true;
            }
            _ => {}
        }
        reply
    }

    /// Ends a drag and the thumb hover, for example when the pointer
    /// leaves the viewport or the input is cancelled. Returns true when
    /// the look changed.
    pub fn release(&mut self) -> bool {
        let changed = self.hovered || self.dragging();
        self.hovered = false;
        self.bar.handle(ScrollBarInput::Release);
        changed
    }

    /// Paints the bar fill and the thumb with the `ScrollArea` `Scrollbar`
    /// and `Thumb` slots. The thumb hover and press replace those flags
    /// of the context state. Paint it after the content.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let Some(track) = self.track() else {
            return;
        };
        let state = cx
            .state
            .set(VisualState::HOVERED, self.hovered)
            .set(VisualState::PRESSED, self.dragging());
        self.bar.paint_track(cx, track, state);
    }

    /// Moves the offset to `offset`, clamped. Returns false when the
    /// offset stays, for example at a limit.
    pub(crate) fn scroll_to(&mut self, offset: f64) -> bool {
        self.bar.set_value(offset)
    }

    /// Sets the content length and keeps the offset, clamped.
    pub(crate) fn set_content(&mut self, content: f64) {
        self.set_scroll(content, self.offset());
    }

    /// Scrolls the least distance that shows `start..end` of the content.
    pub(crate) fn reveal(&mut self, start: f64, end: f64) -> bool {
        let (offset, length) = (self.offset(), self.length());
        if start < offset {
            self.scroll_to(start)
        } else if end > offset + length {
            self.scroll_to(end.min(start + length) - length)
        } else {
            false
        }
    }

    /// Scrolls a vertical bar by a wheel event in a control host, where a
    /// wheel line is `line` points. At a limit the event goes on to an
    /// outer scroll view.
    pub(crate) fn wheel(
        &mut self,
        cx: &mut ControlContext<'_>,
        delta: Point,
        unit: WheelUnit,
        line: f64,
    ) -> EventReply {
        let scale = match unit {
            WheelUnit::Points => 1.0,
            WheelUnit::Lines => line,
        };
        if delta.y != 0.0 && self.scroll_to(self.offset() + delta.y * scale) {
            cx.request_paint();
            EventReply::Stop
        } else {
            EventReply::Continue
        }
    }

    /// Gives a pointer event to the bar in a control host. The host holds
    /// the pointer capture during a thumb drag and repaints. Returns
    /// `Some(scrolled)` when the bar used the event.
    pub(crate) fn hosted_pointer(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &PointerEvent,
    ) -> Option<bool> {
        let dragging = self.dragging();
        let reply = self.pointer(event);
        if dragging != self.dragging() {
            cx.capture_pointer(self.dragging());
        }
        if reply.repaint || reply.scrolled {
            cx.request_paint();
        }
        reply.consumed.then_some(reply.scrolled)
    }
}
