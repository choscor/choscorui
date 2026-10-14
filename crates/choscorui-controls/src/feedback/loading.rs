//! Loading placeholders: a spinning indicator and pulsing skeletons.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{
    ControlKind, ControlSize, Corners, PaintContext, ResolvedStyle, Slot, StyleQuery,
};
use choscorui_core::{
    ColorRole, EventReply, Icon, Measured, Rect, SemanticNode, SemanticRole, Size, Transform,
};
use std::cell::Cell;
use std::f64::consts::TAU;

/// The rotation, in radians, of a spinner `time` seconds into a turn of
/// `turn_ms` milliseconds. A zero turn, as with reduced motion, does not
/// rotate.
pub(crate) fn spin(time: f64, turn_ms: u32) -> f64 {
    if turn_ms == 0 {
        return 0.0;
    }
    let turn = f64::from(turn_ms) / 1000.0;
    (time / turn).fract() * TAU
}

/// Paints the loader icon in `rect` with the colors of `style`, turned
/// for the spinner motion of `turn_ms` milliseconds a turn at the context
/// time. Spinners, attachments, and loading buttons share it.
pub(crate) fn paint_loader(
    cx: &mut PaintContext<'_>,
    rect: Rect,
    style: &ResolvedStyle,
    turn_ms: u32,
) {
    let center = rect.center();
    let rotation = Transform::translate(-center.x, -center.y)
        .then(Transform::rotate(spin(cx.time, turn_ms)))
        .then(Transform::translate(center.x, center.y));
    let pushed = cx.list.push_transform(rotation).is_ok();
    cx.icon(rect, Icon::LoaderCircle, style);
    if pushed {
        let _ = cx.list.pop_transform();
    }
}

/// The opacity of a skeleton `time` seconds into a pulse of `pulse_ms`
/// milliseconds: 1 at the ends of the cycle and 0.5 halfway. A zero
/// pulse, as with reduced motion, stays at 1.
fn pulse(time: f64, pulse_ms: u32) -> f64 {
    if pulse_ms == 0 {
        return 1.0;
    }
    let phase = (time * 1000.0 / f64::from(pulse_ms)).fract();
    1.0 - 0.5 * (1.0 - (phase * TAU).cos()) / 2.0
}

/// A spinning loader icon. It takes no focus; its accessible name says
/// what is loading. With reduced motion, it does not spin.
pub struct Spinner {
    label: String,
    size: ControlSize,
    /// The icon color in place of the recipe foreground.
    color: Option<ColorRole>,
    /// The turn time that the last paint read from the recipe, in
    /// milliseconds.
    turn_ms: Cell<u32>,
}

impl Spinner {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            size: ControlSize::Default,
            color: None,
            turn_ms: Cell::new(0),
        }
    }

    /// The spinner with the icon of `size`: `Xs`, `Sm`, `Default`, or `Lg`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Paints the icon in `role`, for example the text color of the
    /// button or the row that holds the spinner, as `currentColor` does.
    #[must_use]
    pub fn with_color(mut self, role: ColorRole) -> Self {
        self.color = Some(role);
        self
    }

    fn query(&self) -> StyleQuery {
        StyleQuery::new(ControlKind::Spinner, Slot::Root).size(self.size)
    }
}

impl CustomControl for Spinner {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(self.query());
        Measured {
            size: Size::new(style.icon_size, style.icon_size),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.turn_ms.set(cx.style(self.query()).motion_ms);
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let mut style = cx.style(self.query());
        if let Some(role) = self.color {
            style.foreground = cx.tokens.color(role);
        }
        // The motion comes from the paint, so a reduced-motion change
        // stops the turn without a new arrange.
        self.turn_ms.set(style.motion_ms);
        paint_loader(cx, cx.bounds, &style, style.motion_ms);
    }

    fn animating(&self, _time: f64) -> bool {
        self.turn_ms.get() > 0
    }

    fn semantics(&self) -> SemanticNode {
        hosted::node(SemanticRole::ProgressIndicator, &self.label, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// A placeholder block that pulses while content loads. It takes no
/// focus, and assistive technology skips it: mark the container busy
/// while it loads. With reduced motion, it does not pulse.
pub struct Skeleton {
    label: String,
    size: Size,
    circle: bool,
    pulse_ms: u32,
}

impl Skeleton {
    /// A block of `width` by `height` points. A width of zero fills the
    /// width it is given, and is 64 pt without a bounded width.
    pub fn new(label: impl Into<String>, width: f64, height: f64) -> Self {
        Self {
            label: label.into(),
            size: Size::new(width, height),
            circle: false,
            pulse_ms: 0,
        }
    }

    /// A circle, such as an avatar placeholder.
    #[must_use]
    pub fn circle(mut self) -> Self {
        self.circle = true;
        self
    }
}

impl CustomControl for Skeleton {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max.width;
        let width = if self.size.width > 0.0 {
            self.size.width
        } else if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 16.0
        };
        Measured {
            size: Size::new(width, self.size.height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        let style = cx.style(StyleQuery::new(ControlKind::Skeleton, Slot::Root));
        self.pulse_ms = style.motion_ms;
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let mut style = cx.style(StyleQuery::new(ControlKind::Skeleton, Slot::Root));
        if self.circle {
            let bounds = cx.bounds;
            style.radius = Corners::all(bounds.size.width.min(bounds.size.height) / 2.0);
        }
        style.opacity = pulse(cx.time, style.motion_ms);
        cx.slot(cx.bounds, &style, |_, _| {});
    }

    fn animating(&self, _time: f64) -> bool {
        self.pulse_ms > 0
    }

    fn semantics(&self) -> SemanticNode {
        // A placeholder is decoration: the busy container speaks for it.
        let mut node = hosted::node(SemanticRole::ProgressIndicator, &self.label, Vec::new());
        node.state.hidden = true;
        node
    }

    fn focusable(&self) -> bool {
        false
    }
}
