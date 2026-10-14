//! A slider with two thumbs that choose a range.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::range_controls::{snap_to_step, track_fraction, track_point};
use crate::style::{
    ControlKind, Orientation, PaintContext, Slot, StyleQuery, VisualState, fill_shape,
};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Localizer, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRange, SemanticRole, Size,
};

/// A range slider: a track with a low and a high thumb. Each thumb is a
/// tab stop: Tab moves from the low thumb to the high thumb, and Shift+Tab
/// back. The thumbs do not cross. When the thumbs meet, a drag from them
/// moves the low thumb toward the start and the high thumb toward the end.
#[derive(Clone, Debug)]
pub struct RangeSlider {
    name: String,
    minimum: f64,
    maximum: f64,
    step: f64,
    values: [f64; 2],
    orientation: Orientation,
    /// The thumb that keys move: 0 for low, 1 for high.
    active: usize,
    dragging: bool,
    /// A press on the thumbs where they meet: the drag direction picks
    /// the thumb.
    meeting: bool,
    /// The thumb under the pointer.
    hovered: Option<usize>,
    /// The thumb length along the track, and the bounds, from the last
    /// arrange.
    thumb: f64,
    bounds: Rect,
    enabled: bool,
    rtl: bool,
}

impl RangeSlider {
    /// A range over `minimum..=maximum` in `step` increments, starting at
    /// `low` and `high`. Values are clamped and snapped to the step.
    pub fn new(
        name: impl Into<String>,
        minimum: f64,
        maximum: f64,
        step: f64,
        low: f64,
        high: f64,
    ) -> Self {
        let maximum = maximum.max(minimum);
        let mut slider = Self {
            name: name.into(),
            minimum,
            maximum,
            step: if step > 0.0 { step } else { 1.0 },
            values: [minimum, maximum],
            orientation: Orientation::Horizontal,
            active: 0,
            dragging: false,
            meeting: false,
            hovered: None,
            thumb: 0.0,
            bounds: Rect::default(),
            enabled: true,
            rtl: false,
        };
        slider.values = [slider.snap(low), slider.snap(high)];
        if slider.values[0] > slider.values[1] {
            slider.values.swap(0, 1);
        }
        slider
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// The low and high values.
    pub fn values(&self) -> (f64, f64) {
        (self.values[0], self.values[1])
    }

    /// The thumb that keys move: 0 for low, 1 for high.
    pub fn active(&self) -> usize {
        self.active
    }

    fn snap(&self, value: f64) -> f64 {
        snap_to_step(value, self.minimum, self.maximum, self.step)
    }

    fn fraction(&self, value: f64) -> f64 {
        let span = self.maximum - self.minimum;
        if span <= 0.0 {
            0.0
        } else {
            ((value - self.minimum) / span).clamp(0.0, 1.0)
        }
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Slider, slot).state(state)
    }

    fn vertical(&self) -> bool {
        self.orientation == Orientation::Vertical
    }

    /// The thumb center of `value` on the thumb travel in `bounds`.
    fn point(&self, bounds: Rect, value: f64) -> Point {
        let fraction = self.fraction(value);
        track_point(bounds, self.vertical(), self.rtl, self.thumb, fraction)
    }

    /// The value at `point` on the thumb travel in `bounds`.
    fn value_at(&self, bounds: Rect, point: Point) -> f64 {
        let fraction = track_fraction(bounds, self.vertical(), self.rtl, self.thumb, point);
        self.snap(self.minimum + fraction * (self.maximum - self.minimum))
    }

    /// The thumb under `point`, the nearer one when both are.
    fn thumb_at(&self, bounds: Rect, point: Point) -> Option<usize> {
        let distance = |value: f64| {
            let center = self.point(bounds, value);
            if self.vertical() {
                (center.y - point.y).abs()
            } else {
                (center.x - point.x).abs()
            }
        };
        let (low, high) = (distance(self.values[0]), distance(self.values[1]));
        let index = usize::from(high < low);
        (bounds.contains(point) && low.min(high) <= self.thumb / 2.0).then_some(index)
    }

    /// Moves the active thumb to `value`, kept between the other thumb and
    /// the end. Returns whether it moved.
    fn set_active(&mut self, cx: &mut ControlContext<'_>, value: f64) -> bool {
        let value = self.snap(value);
        let value = if self.active == 0 {
            value.min(self.values[1])
        } else {
            value.max(self.values[0])
        };
        if value == self.values[self.active] {
            return false;
        }
        self.values[self.active] = value;
        cx.emit(ControlMessage::Changed(ControlValue::Range(
            self.values[0],
            self.values[1],
        )));
        cx.request_paint();
        cx.request_semantics();
        true
    }
}

impl CustomControl for RangeSlider {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let size = match self.orientation {
            Orientation::Horizontal => Size::new(root.min_width, root.min_height),
            Orientation::Vertical => Size::new(root.min_height, root.min_width),
        };
        Measured {
            size,
            baseline: None,
        }
    }

    /// Records the direction and the thumb length, so that the pointer
    /// maps onto the travel that paint draws.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.bounds = bounds;
        self.rtl = cx.rtl();
        let thumb = cx.style(Self::query(Slot::Thumb, VisualState::NONE));
        self.thumb = if self.vertical() {
            thumb.min_height
        } else {
            thumb.min_width
        };
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let bounds = cx.bounds();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if bounds.contains(*position) => {
                cx.request_focus();
                // The nearer thumb follows the pointer. On the point where
                // the thumbs meet, the drag direction picks it.
                let value = self.value_at(bounds, *position);
                let (low, high) = (self.values[0], self.values[1]);
                self.meeting = low == high && value == low;
                self.active = if (value - low).abs() <= (value - high).abs() && value <= high {
                    0
                } else {
                    1
                };
                self.set_active(cx, value);
                self.dragging = true;
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                let value = self.value_at(bounds, *position);
                if self.meeting {
                    if value == self.values[0] {
                        return EventReply::Stop;
                    }
                    self.meeting = false;
                    self.active = usize::from(value > self.values[0]);
                }
                self.set_active(cx, value);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = self.thumb_at(bounds, *position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) if self.hovered.is_some() => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.dragging => {
                self.dragging = false;
                self.meeting = false;
                cx.capture_pointer(false);
                EventReply::Stop
            }
            // Command and Control chords go on to the shortcuts.
            ControlEvent::Key(KeyEvent::Down { key, .. }) if !cx.modifiers().is_chord() => {
                let shift = cx.modifiers().shift;
                let horizontal = self.orientation == Orientation::Horizontal;
                let current = self.values[self.active];
                let big = self.step * 10.0;
                let target = match key.in_reading_order(self.rtl) {
                    Key::Tab if !shift && self.active == 0 => {
                        self.active = 1;
                        cx.request_paint();
                        cx.request_semantics();
                        return EventReply::Stop;
                    }
                    Key::Tab if shift && self.active == 1 => {
                        self.active = 0;
                        cx.request_paint();
                        cx.request_semantics();
                        return EventReply::Stop;
                    }
                    Key::ArrowUp => current + self.step,
                    Key::ArrowDown => current - self.step,
                    Key::ArrowRight if horizontal => current + self.step,
                    Key::ArrowLeft if horizontal => current - self.step,
                    Key::PageUp => current + big,
                    Key::PageDown => current - big,
                    Key::Home => self.minimum,
                    Key::End => self.maximum,
                    _ => return EventReply::Continue,
                };
                self.set_active(cx, target);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                // Shift+Tab arrives at the high thumb, Tab at the low one.
                if *focused && !self.dragging {
                    self.active = usize::from(cx.modifiers().shift);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let base = VisualState::NONE.set(VisualState::DISABLED, host.disabled());
        let track = cx.style(Self::query(Slot::Track, base));
        let range = cx.style(Self::query(Slot::Range, base));
        let bounds = cx.bounds;
        let (low, high) = (
            self.point(bounds, self.values[0]),
            self.point(bounds, self.values[1]),
        );
        let (rail, band) = match self.orientation {
            Orientation::Horizontal => {
                let t = track.min_height;
                let y = bounds.center().y - t / 2.0;
                let (a, b) = (low.x.min(high.x), low.x.max(high.x));
                (
                    Rect::from_xywh(bounds.origin.x, y, bounds.size.width, t),
                    Rect::from_xywh(a, y, b - a, t),
                )
            }
            Orientation::Vertical => {
                let t = track.min_width;
                let x = bounds.center().x - t / 2.0;
                (
                    Rect::from_xywh(x, bounds.origin.y, t, bounds.size.height),
                    Rect::from_xywh(x, high.y, t, low.y - high.y),
                )
            }
        };
        cx.slot(rail, &track, |_, _| {});
        if range.opacity < 1.0 {
            let _ = cx.list.push_opacity(range.opacity);
        }
        fill_shape(cx.list, band, &range.radius, cx.rtl, &range.background);
        if range.opacity < 1.0 {
            let _ = cx.list.pop_opacity();
        }
        for (index, center) in [low, high].into_iter().enumerate() {
            let active = index == self.active;
            let hovered = self.hovered == Some(index) || self.dragging && active;
            let state = base
                .set(VisualState::FOCUS_VISIBLE, host.focus_visible() && active)
                .set(VisualState::HOVERED, host.hovered() && hovered)
                .set(VisualState::PRESSED, self.dragging && active);
            let thumb = cx.style(Self::query(Slot::Thumb, state));
            let rect = Rect::from_xywh(
                center.x - thumb.min_width / 2.0,
                center.y - thumb.min_height / 2.0,
                thumb.min_width,
                thumb.min_height,
            );
            cx.slot(rect, &thumb, |_, _| {});
        }
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
        let mut node = hosted::node(SemanticRole::Slider, &self.name, actions);
        node.value = Some(format!("{} – {}", self.values[0], self.values[1]));
        node.state.disabled = !self.enabled;
        node
    }

    /// A `Slider` part for each thumb, named low and high, with its range.
    /// The active thumb is the focused part.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        for (index, id) in ["choscorui.range.low", "choscorui.range.high"]
            .into_iter()
            .enumerate()
        {
            let value = self.values[index];
            let center = self.point(self.bounds, value);
            let size = Size::new(self.thumb, self.thumb);
            let rect = Rect::new(
                Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
                size,
            );
            let name = strings.message(id, &[]);
            let part = parts.push(None, SemanticRole::Slider, &name, rect);
            part.node.value = Some(value.to_string());
            part.node.state.range = Some(SemanticRange {
                value,
                minimum: self.minimum,
                maximum: self.maximum,
                step: self.step,
            });
            part.node.actions = vec![SemanticAction::Increment, SemanticAction::Decrement];
            if index == self.active {
                parts.set_focused(Some(index));
            }
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let current = self.values[self.active];
        match action {
            SemanticAction::Increment => self.set_active(cx, current + self.step),
            SemanticAction::Decrement => self.set_active(cx, current - self.step),
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    /// Disabling ends a drag and the hover.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.dragging = false;
            self.meeting = false;
            self.hovered = None;
        }
    }

    /// Takes `Range`, clamped to the bounds and snapped to the step.
    /// Rejects a value that is not finite and a low value above the high.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Range(low, high) = *value else {
            return false;
        };
        if !low.is_finite() || !high.is_finite() || low > high {
            return false;
        }
        self.values = [self.snap(low), self.snap(high)];
        true
    }
}
