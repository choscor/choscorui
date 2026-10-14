//! A row or column of slides that scrolls one slide at a time and snaps.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Parts};
use crate::style::{
    ControlKind, ControlSize, Orientation, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, MotionTokens, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size, WheelUnit,
};

/// How long the carousel waits after the last wheel event before it snaps
/// to the nearest slide, in seconds.
const SNAP_DELAY: f64 = 0.15;

/// The release speed of a drag, in points per second, from which the
/// carousel moves one slide in the drag direction.
const FLICK_SPEED: f64 = 300.0;

/// A drag that rests longer than this before the release, in seconds, is
/// not a flick.
const FLICK_REST: f64 = 0.1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Button {
    Previous,
    Next,
}

/// A carousel of slides. Each slide is a card surface that shows its
/// label. Previous and Next buttons sit outside the slides, and arrow
/// keys along the orientation move one slide. A drag or a wheel scrolls
/// the slides, and the carousel snaps to the nearest slide when it ends.
/// Each move emits `Changed(Index(slide))`.
pub struct Carousel {
    name: String,
    labels: [String; 2],
    items: Vec<String>,
    orientation: Orientation,
    per_view: usize,
    /// The first visible slide.
    current: usize,
    /// The scroll position in slides at `changed_at`, where the animation
    /// to `current` starts.
    from: f64,
    changed_at: f64,
    /// The scroll position in slides while a drag or a wheel moves it.
    free: Option<f64>,
    /// The pointer position and the scroll position where a drag started.
    drag: Option<(Point, f64)>,
    /// The scroll position and the time of the last drag move, and the
    /// drag speed there in slides per second.
    flick: (f64, f64, f64),
    motion: MotionTokens,
    snap_at: Option<f64>,
    /// The focused button, and the hovered and pressed one.
    focus: Button,
    parts: Parts<Button>,
    viewport: Rect,
    buttons: [Rect; 2],
    /// One slide and its gap along the orientation.
    step: f64,
    line: f64,
    duration_ms: u32,
    enabled: bool,
    rtl: bool,
}

impl Carousel {
    /// A carousel with `items` as slide labels. `previous` and `next` name
    /// the buttons.
    pub fn new(
        name: impl Into<String>,
        previous: impl Into<String>,
        next: impl Into<String>,
        items: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            labels: [previous.into(), next.into()],
            items,
            orientation: Orientation::Horizontal,
            per_view: 1,
            current: 0,
            from: 0.0,
            changed_at: f64::NEG_INFINITY,
            free: None,
            drag: None,
            flick: (0.0, 0.0, 0.0),
            motion: MotionTokens::default(),
            snap_at: None,
            focus: Button::Next,
            parts: Parts::default(),
            viewport: Rect::default(),
            buttons: [Rect::default(); 2],
            step: 0.0,
            line: 0.0,
            duration_ms: 0,
            enabled: true,
            rtl: false,
        }
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Shows `count` slides at once, at least one.
    #[must_use]
    pub fn with_per_view(mut self, count: usize) -> Self {
        self.per_view = count.max(1);
        self
    }

    /// The first visible slide.
    pub fn current(&self) -> usize {
        self.current
    }

    fn horizontal(&self) -> bool {
        self.orientation == Orientation::Horizontal
    }

    fn last(&self) -> usize {
        self.items.len().saturating_sub(self.per_view)
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Carousel, slot)
    }

    fn can(&self, button: Button) -> bool {
        match button {
            Button::Previous => self.current > 0,
            Button::Next => self.current < self.last(),
        }
    }

    fn label(&self, button: Button) -> &str {
        &self.labels[button as usize]
    }

    /// The shown scroll position in slides at `time`.
    fn position(&self, motion: &MotionTokens, time: f64) -> f64 {
        if let Some(free) = self.free {
            return free;
        }
        let linear = if motion.reduced || self.duration_ms == 0 {
            1.0
        } else {
            ((time - self.changed_at) * 1000.0 / f64::from(self.duration_ms)).clamp(0.0, 1.0)
        };
        let eased = motion.easing.solve(linear);
        self.from + (self.current as f64 - self.from) * eased
    }

    /// Moves to `slide`, animating from the scroll position `from` at
    /// `time`. Returns true when the slide changed.
    fn move_to(&mut self, slide: usize, from: f64, time: f64) -> bool {
        let slide = slide.min(self.last());
        self.from = from;
        self.changed_at = time;
        self.free = None;
        self.snap_at = None;
        let changed = slide != self.current;
        self.current = slide;
        if !self.can(self.focus) {
            self.focus = match self.focus {
                Button::Previous => Button::Next,
                Button::Next => Button::Previous,
            };
        }
        changed
    }

    /// Shows `slide`, animating from the scroll position `from`.
    fn show(&mut self, cx: &mut ControlContext<'_>, slide: usize, from: f64) {
        if self.move_to(slide, from, cx.time()) {
            cx.emit(ControlMessage::Changed(ControlValue::Index(Some(
                self.current,
            ))));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    /// The shown scroll position now, where a new move starts.
    fn shown(&self, cx: &ControlContext<'_>) -> f64 {
        self.position(&self.motion, cx.time())
    }

    fn step_by(&mut self, cx: &mut ControlContext<'_>, forward: bool) -> bool {
        let button = if forward {
            Button::Next
        } else {
            Button::Previous
        };
        if !self.can(button) {
            return false;
        }
        let target = if forward {
            self.current + 1
        } else {
            self.current - 1
        };
        let from = self.shown(cx);
        self.show(cx, target, from);
        true
    }

    fn snap(&mut self, cx: &mut ControlContext<'_>) {
        let from = self.shown(cx);
        let slide = from.round().clamp(0.0, self.last() as f64) as usize;
        self.show(cx, slide, from);
    }

    /// Ends a drag that started at slide position `start`: a flick moves
    /// one slide in its direction, else the nearest slide shows.
    fn release(&mut self, cx: &mut ControlContext<'_>, start: f64) {
        let (_, at, speed) = self.flick;
        let fast = cx.time() - at <= FLICK_REST && speed.abs() * self.step >= FLICK_SPEED;
        if !fast {
            self.snap(cx);
            return;
        }
        let target = if speed > 0.0 {
            start.floor() + 1.0
        } else {
            start.ceil() - 1.0
        };
        let from = self.shown(cx);
        self.show(cx, target.clamp(0.0, self.last() as f64) as usize, from);
    }

    fn button_at(&self, point: Point) -> Option<Button> {
        [Button::Previous, Button::Next]
            .into_iter()
            .find(|button| self.buttons[*button as usize].contains(point))
    }

    /// The scroll distance along the orientation, in slides, that moving
    /// the pointer by `delta` points makes. Content follows the pointer.
    fn slides_for(&self, delta: Point) -> f64 {
        if self.step <= 0.0 {
            return 0.0;
        }
        let along = if self.horizontal() {
            if self.rtl { delta.x } else { -delta.x }
        } else {
            -delta.y
        };
        along / self.step
    }

    fn keys(&self) -> (Key, Key) {
        match (self.horizontal(), self.rtl) {
            (true, false) => (Key::ArrowLeft, Key::ArrowRight),
            (true, true) => (Key::ArrowRight, Key::ArrowLeft),
            (false, _) => (Key::ArrowUp, Key::ArrowDown),
        }
    }

    fn icons(&self) -> [Icon; 2] {
        match (self.horizontal(), self.rtl) {
            (true, false) => [Icon::ArrowLeft, Icon::ArrowRight],
            (true, true) => [Icon::ArrowRight, Icon::ArrowLeft],
            (false, _) => [Icon::ArrowUp, Icon::ArrowDown],
        }
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, &ControlEvent::Pointer(*event), |point| {
            self.button_at(point).filter(|button| self.can(*button))
        });
        self.parts = parts;
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if self.button_at(*position).is_some() {
                    if let Some(button) = self.parts.pressed {
                        self.focus = button;
                        cx.request_focus();
                    }
                    return EventReply::Stop;
                }
                if !self.viewport.contains(*position) {
                    return EventReply::Continue;
                }
                let from = self.shown(cx);
                self.drag = Some((*position, from));
                self.free = Some(from);
                self.flick = (from, cx.time(), 0.0);
                cx.capture_pointer(true);
                EventReply::Stop
            }
            PointerEvent::Move { position } => {
                let Some((start, from)) = self.drag else {
                    return EventReply::Continue;
                };
                let moved = Point::new(position.x - start.x, position.y - start.y);
                let free = (from + self.slides_for(moved)).clamp(0.0, self.last() as f64);
                self.free = Some(free);
                let (last, at, speed) = self.flick;
                let elapsed = cx.time() - at;
                let speed = if elapsed > 0.0 {
                    (free - last) / elapsed
                } else {
                    speed
                };
                self.flick = (free, cx.time(), speed);
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Up { .. } => {
                if let Some(button) = pressed {
                    if self.parts.hovered == Some(button) {
                        self.step_by(cx, button == Button::Next);
                    }
                    return EventReply::Stop;
                }
                cx.capture_pointer(false);
                if let Some((_, start)) = self.drag.take() {
                    self.release(cx, start);
                    return EventReply::Stop;
                }
                EventReply::Continue
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.viewport.contains(*position) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => self.line,
                };
                // A right-to-left carousel runs its slides toward the left.
                let along = match (self.horizontal(), self.rtl) {
                    (true, false) => delta.x,
                    (true, true) => -delta.x,
                    (false, _) => delta.y,
                };
                if along == 0.0 || self.step <= 0.0 {
                    return EventReply::Continue;
                }
                // A wheel during an animation starts from the shown position.
                let base = self.shown(cx);
                let free = (base + along * scale / self.step).clamp(0.0, self.last() as f64);
                // At the first or last slide the wheel goes on to an outer
                // scroll view.
                if free == base {
                    return EventReply::Continue;
                }
                self.free = Some(free);
                self.snap_at = Some(cx.time() + SNAP_DELAY);
                cx.request_paint();
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint_slides(&self, cx: &mut PaintContext<'_>) {
        let item = cx.style(Self::query(Slot::Item));
        let surface = cx.style(StyleQuery::new(ControlKind::Card, Slot::Root));
        let title = cx.style(StyleQuery::new(ControlKind::Card, Slot::Title));
        let position = self.position(&cx.tokens.motion, cx.time);
        let viewport = self.viewport;
        let pad = item.padding.inline_start;
        let extent = self.step - pad;
        cx.list.push_clip(viewport);
        let first = position.floor().max(0.0) as usize;
        let end = (position + self.per_view as f64).ceil() as usize;
        let shown = first..end.min(self.items.len());
        for index in shown {
            let offset = (index as f64 - position) * self.step;
            let rect = if self.horizontal() {
                let x = if self.rtl {
                    viewport.max_x() - offset - extent
                } else {
                    viewport.origin.x + offset
                };
                Rect::from_xywh(x, viewport.origin.y, extent, viewport.size.height)
            } else {
                Rect::from_xywh(
                    viewport.origin.x,
                    viewport.origin.y + offset,
                    viewport.size.width,
                    extent,
                )
            };
            let label = &self.items[index];
            cx.slot(rect, &surface, |cx, inner| {
                cx.label(inner, label, &title, TextAlign::Center);
            });
        }
        let _ = cx.list.pop_clip();
    }
}

impl CustomControl for Carousel {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root));
        let item = cx.style(Self::query(Slot::Item));
        let button = cx.style(Self::query(Slot::Previous)).min_height;
        let pad = item.padding.inline_start;
        let max = cx.constraints.max.width;
        // An unbounded carousel is as wide as an extra small container.
        let natural = cx.tokens.spacing * 80.0;
        let size = if self.horizontal() {
            let outer = button * 2.0 + root.gap * 2.0;
            let viewport = if max.is_finite() {
                (max - outer).max(0.0)
            } else {
                natural
            };
            // Slides are square.
            let slide = (viewport + pad) / self.per_view as f64 - pad;
            Size::new(viewport + outer, slide.max(0.0))
        } else {
            let width = if max.is_finite() { max } else { natural };
            // Slides are half as tall as they are wide.
            let viewport = (width / 2.0 + pad) * self.per_view as f64 - pad;
            Size::new(width, viewport + button * 2.0 + root.gap * 2.0)
        };
        Measured {
            size,
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root));
        let item = cx.style(Self::query(Slot::Item));
        let side = cx.style(Self::query(Slot::Previous)).min_height;
        self.rtl = cx.rtl();
        self.duration_ms = root.motion_ms;
        self.motion = cx.tokens.motion;
        self.line = root.text.line_height;
        let pad = item.padding.inline_start;
        let outer = side + root.gap;
        if self.horizontal() {
            self.viewport = Rect::from_xywh(
                bounds.origin.x + outer,
                bounds.origin.y,
                (bounds.size.width - outer * 2.0).max(0.0),
                bounds.size.height,
            );
            let y = bounds.center().y - side / 2.0;
            let start = Rect::from_xywh(bounds.origin.x, y, side, side);
            let end = Rect::from_xywh(bounds.max_x() - side, y, side, side);
            self.buttons = if self.rtl { [end, start] } else { [start, end] };
            self.step = (self.viewport.size.width + pad) / self.per_view as f64;
        } else {
            self.viewport = Rect::from_xywh(
                bounds.origin.x,
                bounds.origin.y + outer,
                bounds.size.width,
                (bounds.size.height - outer * 2.0).max(0.0),
            );
            let x = bounds.center().x - side / 2.0;
            self.buttons = [
                Rect::from_xywh(x, bounds.origin.y, side, side),
                Rect::from_xywh(x, bounds.max_y() - side, side, side),
            ];
            self.step = (self.viewport.size.height + pad) / self.per_view as f64;
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let (previous, next) = self.keys();
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Timer => {
                if self.snap_at.take().is_some() {
                    self.snap(cx);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => match *key {
                key if key == previous => {
                    self.step_by(cx, false);
                    EventReply::Stop
                }
                key if key == next => {
                    self.step_by(cx, true);
                    EventReply::Stop
                }
                Key::Home | Key::End => {
                    let slide = if *key == Key::Home { 0 } else { self.last() };
                    let from = self.shown(cx);
                    self.show(cx, slide, from);
                    EventReply::Stop
                }
                Key::Tab => {
                    let back = cx.modifiers().shift;
                    let other = match (self.focus, back) {
                        (Button::Previous, false) => Some(Button::Next),
                        (Button::Next, true) => Some(Button::Previous),
                        _ => None,
                    };
                    match other.filter(|button| self.can(*button)) {
                        Some(button) => {
                            self.focus = button;
                            cx.request_paint();
                            cx.request_semantics();
                            EventReply::Stop
                        }
                        None => EventReply::Continue,
                    }
                }
                Key::Enter | Key::Space if !repeat => {
                    self.step_by(cx, self.focus == Button::Next);
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            },
            ControlEvent::FocusChanged(true) => {
                // Shift+Tab arrives at Next, Tab at Previous, when enabled.
                let preferred = if cx.modifiers().shift {
                    Button::Next
                } else {
                    Button::Previous
                };
                self.focus = if self.can(preferred) {
                    preferred
                } else {
                    match preferred {
                        Button::Previous => Button::Next,
                        Button::Next => Button::Previous,
                    }
                };
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            ControlEvent::Hover(false) => {
                self.parts.track(cx, event, |_| None);
                EventReply::Continue
            }
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                self.parts.track(cx, event, |_| None);
                if self.drag.take().is_some() {
                    cx.capture_pointer(false);
                    if self.free.is_some() {
                        self.snap(cx);
                    }
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn deadline(&self) -> Option<f64> {
        self.snap_at
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_slides(cx);
        let host = cx.state;
        let icons = self.icons();
        for button in [Button::Previous, Button::Next] {
            let slot = match button {
                Button::Previous => Slot::Previous,
                Button::Next => Slot::Next,
            };
            let state = self
                .parts
                .state(button, host)
                .set(VisualState::DISABLED, host.disabled() || !self.can(button))
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && self.focus == button,
                );
            let style = cx.style(Self::query(slot).size(ControlSize::IconSm).state(state));
            let icon = icons[button as usize];
            cx.slot(self.buttons[button as usize], &style, |cx, inner| {
                cx.icon(inner, icon, &style);
            });
        }
    }

    fn animating(&self, time: f64) -> bool {
        (time - self.changed_at) * 1000.0 < f64::from(self.duration_ms)
    }

    fn semantics(&self) -> SemanticNode {
        let mut actions = Vec::new();
        if self.enabled {
            actions.push(SemanticAction::Focus);
            if self.can(Button::Previous) || self.can(Button::Next) {
                actions.push(SemanticAction::Activate);
            }
            if self.can(Button::Next) {
                actions.push(SemanticAction::Increment);
            }
            if self.can(Button::Previous) {
                actions.push(SemanticAction::Decrement);
            }
        }
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = self.items.get(self.current).cloned();
        node.description = self.label(self.focus).to_string();
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
            SemanticAction::Activate => self.step_by(cx, self.focus == Button::Next),
            SemanticAction::Increment => self.step_by(cx, true),
            SemanticAction::Decrement => self.step_by(cx, false),
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        self.last() > 0
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Takes `Index`, the first visible slide, and shows it without an
    /// animation. Rejects no slide and a slide past the last first slide.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Index(Some(slide)) = *value else {
            return false;
        };
        if slide > self.last() {
            return false;
        }
        self.move_to(slide, slide as f64, f64::NEG_INFINITY);
        true
    }
}
