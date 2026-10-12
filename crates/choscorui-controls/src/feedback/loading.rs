//! Loading placeholders: a spinning indicator and pulsing skeletons.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{ControlKind, Corners, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    EventReply, Icon, Measured, Rect, SemanticNode, SemanticRole, Size, Transform,
};
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
    /// The turn time from the recipe, in milliseconds.
    turn_ms: u32,
}

impl Spinner {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            turn_ms: 0,
        }
    }
}

impl CustomControl for Spinner {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Spinner, Slot::Root));
        Measured {
            size: Size::new(style.icon_size, style.icon_size),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        let style = cx.style(StyleQuery::new(ControlKind::Spinner, Slot::Root));
        self.turn_ms = style.motion_ms;
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::Spinner, Slot::Root));
        let center = cx.bounds.center();
        let angle = spin(cx.time, style.motion_ms);
        let rotation = Transform::translate(-center.x, -center.y)
            .then(Transform::rotate(angle))
            .then(Transform::translate(center.x, center.y));
        let pushed = cx.list.push_transform(rotation).is_ok();
        cx.icon(cx.bounds, Icon::LoaderCircle, &style);
        if pushed {
            let _ = cx.list.pop_transform();
        }
    }

    fn animating(&self, _time: f64) -> bool {
        self.turn_ms > 0
    }

    fn semantics(&self) -> SemanticNode {
        hosted::node(SemanticRole::ProgressIndicator, &self.label, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// A placeholder block that pulses while content loads. It takes no
/// focus; its accessible name says what is loading. With reduced motion,
/// it does not pulse.
pub struct Skeleton {
    label: String,
    size: Size,
    circle: bool,
    pulse_ms: u32,
}

impl Skeleton {
    /// A block of `width` by `height` points. A width of zero fills the
    /// width it is given.
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
        let width = if self.size.width > 0.0 || !max.is_finite() {
            self.size.width
        } else {
            max
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
        hosted::node(SemanticRole::ProgressIndicator, &self.label, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}
