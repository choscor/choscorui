//! Shared behavior for the controls that mount through [`ControlHost`]:
//! press tracking for button-like parts, roving focus over a row of
//! items, the semantic node builder, and the popup reveal animation.
//!
//! [`ControlHost`]: crate::ControlHost

use crate::custom::{ControlContext, ControlEvent};
use crate::style::{PaintContext, VisualState};
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

/// Hover and press of the inner parts of a composite control, such as
/// the buttons of a panel or the rows of a list. `K` names a part, by
/// default its index. Move and the end of the hover set the hovered part.
/// A primary press on a part captures the pointer and presses the part
/// until the release. The pressed look shows only while the pointer is
/// over the pressed part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Parts<K = usize> {
    pub hovered: Option<K>,
    pub pressed: Option<K>,
}

impl<K> Default for Parts<K> {
    fn default() -> Self {
        Self {
            hovered: None,
            pressed: None,
        }
    }
}

impl<K: Copy + PartialEq> Parts<K> {
    /// Follows `event` for the parts that `hit` finds, and requests a
    /// paint when a part changes. The control still handles the event.
    pub fn track(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        hit: impl Fn(Point) -> Option<K>,
    ) {
        if cx.phase() != EventPhase::Target {
            return;
        }
        let before = *self;
        match event {
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                self.hovered = hit(*position);
            }
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                self.hovered = hit(*position);
                self.pressed = self.hovered;
                if self.pressed.is_some() {
                    cx.capture_pointer(true);
                }
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) => {
                self.hovered = hit(*position);
                if self.pressed.take().is_some() {
                    cx.capture_pointer(false);
                }
            }
            ControlEvent::Hover(false) => self.hovered = None,
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.pressed.is_some() => {
                self.pressed = None;
                cx.capture_pointer(false);
            }
            _ => {}
        }
        if *self != before {
            cx.request_paint();
        }
    }

    /// Forgets the parts, for example when the parts change.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The parts renamed by `local`, for example the buttons of one row
    /// out of the parts of a whole list. A part that `local` drops is not
    /// hovered or pressed.
    pub fn map<L>(&self, local: impl Fn(K) -> Option<L>) -> Parts<L> {
        Parts {
            hovered: self.hovered.and_then(&local),
            pressed: self.pressed.and_then(&local),
        }
    }

    /// The pressed part while the pointer is still over it.
    pub fn shown_pressed(&self) -> Option<K> {
        self.pressed.filter(|part| self.hovered == Some(*part))
    }

    /// The state of `part`: hovered, pressed while the pointer is over it,
    /// and the disabled state of the `host` control.
    pub fn state(&self, part: K, host: VisualState) -> VisualState {
        let enabled = !host.disabled();
        VisualState::NONE
            .set(VisualState::DISABLED, !enabled)
            .set(VisualState::HOVERED, enabled && self.hovered == Some(part))
            .set(
                VisualState::PRESSED,
                enabled && self.shown_pressed() == Some(part),
            )
    }
}

/// Reports a popup that closed without a choice (an outside press,
/// Escape, focus loss, or disabling): emits
/// [`ControlMessage::Dismissed`](crate::ControlMessage::Dismissed) when
/// `closed` is true. The reply is `Stop` when the popup closed, so that the
/// host consumes an outside press.
pub(crate) fn dismissed(cx: &mut ControlContext<'_>, closed: bool) -> EventReply {
    if closed {
        cx.emit(crate::ControlMessage::Dismissed);
        EventReply::Stop
    } else {
        EventReply::Continue
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
    let (next, previous) = if horizontal {
        (Key::ArrowRight, Key::ArrowLeft)
    } else {
        (Key::ArrowDown, Key::ArrowUp)
    };
    match key.in_reading_order(rtl) {
        Key::Home => Some(0),
        Key::End => Some(count - 1),
        key if key == next => Some((current + 1) % count),
        key if key == previous => Some((current + count - 1) % count),
        _ => None,
    }
}

/// The part that takes the roving focus when focus enters a group of
/// `count` parts: the first enabled part that is checked, else the first
/// enabled part.
pub(crate) fn roving_entry(
    count: usize,
    enabled: &dyn Fn(usize) -> bool,
    checked: &dyn Fn(usize) -> bool,
) -> Option<usize> {
    (0..count)
        .find(|index| enabled(*index) && checked(*index))
        .or_else(|| (0..count).find(|index| enabled(*index)))
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
pub struct Reveal {
    /// Whether the popup is open, or opens.
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
    /// Opens or closes the popup at host `time`.
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
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        let _ = cx
            .list
            .push_transform(Self::transform(rect, progress, slide));
        content(cx);
        let _ = cx.list.pop_transform();
        let _ = cx.list.pop_opacity();
    }

    /// The transform of `rect` at `progress`: a zoom from 95% about its
    /// center, and `slide` points of offset that shrink to zero.
    pub fn transform(rect: Rect, progress: f64, slide: (f64, f64)) -> Transform {
        let scale = 0.95 + 0.05 * progress;
        let center = rect.center();
        let rest = 1.0 - progress;
        Transform::translate(-center.x, -center.y)
            .then(Transform::scale(scale, scale))
            .then(Transform::translate(
                center.x + slide.0 * rest,
                center.y + slide.1 * rest,
            ))
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
