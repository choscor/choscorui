//! Shared behavior for the controls that mount through [`ControlHost`]:
//! press tracking for button-like parts, roving focus over a row of
//! items, the semantic node builder, and the popup reveal animation.
//!
//! [`ControlHost`]: crate::ControlHost

use crate::custom::{ControlContext, ControlEvent};
use crate::style::PaintContext;
use choscorui_core::{
    DesignTokens, EventPhase, EventReply, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Transform,
};

/// What [`Press::event`] did with an event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PressOutcome {
    /// Not a press event; the control may handle it.
    Ignored,
    /// The press state changed, or the event was consumed.
    Handled,
    /// A click or an activation key completed at this point.
    Activated(Option<Point>),
}

/// Press tracking for a button-like control. A primary press inside the
/// bounds captures the pointer, and a release inside activates. Space and
/// Enter activate on key-down without repeat.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Press {
    pub pressed: bool,
}

impl Press {
    pub fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> PressOutcome {
        if cx.phase() != EventPhase::Target {
            return PressOutcome::Ignored;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                self.pressed = true;
                cx.capture_pointer(true);
                cx.request_paint();
                PressOutcome::Handled
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                cx.request_paint();
                if cx.bounds().contains(*position) {
                    PressOutcome::Activated(Some(*position))
                } else {
                    PressOutcome::Handled
                }
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => PressOutcome::Activated(None),
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                cx.request_paint();
                PressOutcome::Handled
            }
            _ => PressOutcome::Ignored,
        }
    }
}

/// The reply for a press outcome that the control has acted on.
pub(crate) fn reply(outcome: PressOutcome) -> EventReply {
    match outcome {
        PressOutcome::Ignored => EventReply::Continue,
        PressOutcome::Handled | PressOutcome::Activated(_) => EventReply::Stop,
    }
}

/// The key-down key of `event`, repeats included.
pub(crate) fn key_down(event: &ControlEvent) -> Option<Key> {
    match event {
        ControlEvent::Key(KeyEvent::Down { key, .. }) => Some(*key),
        _ => None,
    }
}

/// The item a roving-focus key moves to among `count` items, or `None`
/// for other keys. Arrow keys along `horizontal` follow reading order
/// and wrap; Home and End go to the ends.
pub(crate) fn rove(
    key: Key,
    current: usize,
    count: usize,
    horizontal: bool,
    rtl: bool,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let (next, previous) = match (horizontal, rtl) {
        (true, false) => (Key::ArrowRight, Key::ArrowLeft),
        (true, true) => (Key::ArrowLeft, Key::ArrowRight),
        (false, _) => (Key::ArrowDown, Key::ArrowUp),
    };
    match key {
        Key::Home => Some(0),
        Key::End => Some(count - 1),
        key if key == next => Some((current + 1) % count),
        key if key == previous => Some((current + count - 1) % count),
        _ => None,
    }
}

/// The tab stop that Tab, or Shift+Tab with `back`, moves to among
/// `count` stops inside one control. `None` means focus leaves the
/// control.
pub(crate) fn tab_stop(current: usize, count: usize, back: bool) -> Option<usize> {
    if back {
        current.checked_sub(1)
    } else {
        (current + 1 < count).then_some(current + 1)
    }
}

/// The index of the first rectangle that contains `point`.
pub(crate) fn hit(rects: &[Rect], point: Point) -> Option<usize> {
    rects.iter().position(|rect| rect.contains(point))
}

/// A semantic node with `role`, `name`, and `actions`, and default state.
pub(crate) fn node(
    role: SemanticRole,
    name: impl Into<String>,
    actions: Vec<SemanticAction>,
) -> SemanticNode {
    SemanticNode {
        role,
        name: name.into(),
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions,
        text: None,
    }
}

/// The open and close animation of a popup: fade, zoom from 95%, and a
/// slide from the side it opens toward. Reduced motion skips it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reveal {
    pub open: bool,
    /// When `open` last changed, in host time.
    pub changed_at: f64,
}

impl Default for Reveal {
    /// Closed since long ago: nothing animates.
    fn default() -> Self {
        Self {
            open: false,
            changed_at: f64::NEG_INFINITY,
        }
    }
}

impl Reveal {
    pub fn set(&mut self, open: bool, time: f64) {
        if self.open != open {
            self.open = open;
            self.changed_at = time;
        }
    }

    /// How far the popup is shown at `time`, from 0 (hidden) to 1, after
    /// easing.
    pub fn progress(&self, tokens: &DesignTokens, duration_ms: u32, time: f64) -> f64 {
        let linear = if tokens.motion.reduced || duration_ms == 0 {
            1.0
        } else {
            ((time - self.changed_at) * 1000.0 / f64::from(duration_ms)).clamp(0.0, 1.0)
        };
        let eased = tokens.motion.easing.solve(linear);
        if self.open { eased } else { 1.0 - eased }
    }

    /// True while the animation runs at `time`.
    pub fn animating(&self, duration_ms: u32, time: f64) -> bool {
        (time - self.changed_at) * 1000.0 < f64::from(duration_ms)
    }

    /// Paints `content` for `rect` at `progress`, sliding `slide` points
    /// in from the offset `(dx, dy)` direction.
    pub fn paint(
        cx: &mut PaintContext<'_>,
        rect: Rect,
        progress: f64,
        slide: (f64, f64),
        content: impl FnOnce(&mut PaintContext<'_>),
    ) {
        if progress >= 1.0 {
            content(cx);
            return;
        }
        let scale = 0.95 + 0.05 * progress;
        let center = rect.center();
        let rest = 1.0 - progress;
        let transform = Transform::translate(-center.x, -center.y)
            .then(Transform::scale(scale, scale))
            .then(Transform::translate(
                center.x + slide.0 * rest,
                center.y + slide.1 * rest,
            ));
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        let _ = cx.list.push_transform(transform);
        content(cx);
        let _ = cx.list.pop_transform();
        let _ = cx.list.pop_opacity();
    }
}

/// What a built-in control's own `handle` takes from a host event: the
/// pointer or key event at the target phase, or a cancel when the control
/// loses focus or a popup above it opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Routed {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Cancel,
}

/// The routed form of `event` for a built-in control, or `None` for an
/// event that the control ignores.
pub(crate) fn route(cx: &ControlContext<'_>, event: &ControlEvent) -> Option<Routed> {
    if cx.phase() != EventPhase::Target {
        return None;
    }
    match event {
        ControlEvent::Pointer(event) => Some(Routed::Pointer(*event)),
        ControlEvent::Key(event) => Some(Routed::Key(*event)),
        ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => Some(Routed::Cancel),
        _ => None,
    }
}

/// The reply for a routed event: pointer presses and releases and the
/// activation keys stop; moves and other keys continue.
pub(crate) fn routed_reply(routed: Routed) -> EventReply {
    match routed {
        Routed::Pointer(PointerEvent::Down { .. } | PointerEvent::Up { .. })
        | Routed::Key(KeyEvent::Down {
            key: Key::Space | Key::Enter,
            ..
        })
        | Routed::Key(KeyEvent::Up { key: Key::Space }) => EventReply::Stop,
        _ => EventReply::Continue,
    }
}
