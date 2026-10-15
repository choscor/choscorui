//! Click counting, scroll conversion, and pinch conversion, for both
//! display servers.

use choscorui_core::{Point, PointerButton};
use std::time::{Duration, Instant};

/// The longest time between the presses of one multi-click.
const CLICK_INTERVAL: Duration = Duration::from_millis(400);
/// The farthest distance, in points, between the presses of one
/// multi-click.
const CLICK_DISTANCE: f64 = 4.0;

/// Counts the presses of a multi-click.
#[derive(Default)]
pub(crate) struct ClickCounter {
    last: Option<(Instant, Point, PointerButton, u32)>,
}

impl ClickCounter {
    /// The click count of a press at `position`.
    pub(crate) fn press(&mut self, now: Instant, position: Point, button: PointerButton) -> u32 {
        let count = match self.last {
            Some((time, at, last_button, count))
                if last_button == button
                    && now.saturating_duration_since(time) <= CLICK_INTERVAL
                    && (position.x - at.x).abs() <= CLICK_DISTANCE
                    && (position.y - at.y).abs() <= CLICK_DISTANCE =>
            {
                count + 1
            }
            _ => 1,
        };
        self.last = Some((now, position, button, count));
        count
    }

    /// The click count of the release that follows a press.
    pub(crate) fn release(&self) -> u32 {
        self.last.map_or(1, |(.., count)| count)
    }

    pub(crate) fn reset(&mut self) {
        self.last = None;
    }
}

/// Converts the cumulative scale of a touchpad pinch into the scale
/// change of each event. `GestureEvent::Magnify` carries the change since
/// the previous event, as on macOS, where it is one plus the AppKit
/// magnification. The Wayland pinch reports the scale since the start of
/// the gesture.
#[derive(Default)]
pub(crate) struct PinchScale {
    /// The cumulative scale of the previous event, while a pinch is active.
    last: Option<f64>,
}

impl PinchScale {
    /// Starts a pinch at scale 1.
    pub(crate) fn begin(&mut self) {
        self.last = Some(1.0);
    }

    /// The scale change of an update with the cumulative `scale`. Returns
    /// `None` outside a pinch, for an invalid scale, and when the scale
    /// did not change.
    pub(crate) fn update(&mut self, scale: f64) -> Option<f64> {
        let last = self.last?;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        self.last = Some(scale);
        let change = scale / last;
        (change.is_finite() && change != 1.0).then_some(change)
    }

    /// Ends or cancels the pinch. The changes already sent stay.
    pub(crate) fn end(&mut self) {
        self.last = None;
    }
}

/// The core button of a Linux input button code (`BTN_*`).
pub(crate) fn button(code: u32) -> PointerButton {
    match code {
        0x110 => PointerButton::Primary,
        0x111 => PointerButton::Secondary,
        0x112 => PointerButton::Middle,
        // BTN_SIDE, BTN_EXTRA, and the others follow macOS numbering:
        // 3 and up.
        code => PointerButton::Other(usize::try_from(code.saturating_sub(0x110)).unwrap_or(3)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_near_presses_count_up_and_others_restart() {
        let mut counter = ClickCounter::default();
        let start = Instant::now();
        let at = Point::new(10.0, 10.0);
        assert_eq!(counter.press(start, at, PointerButton::Primary), 1);
        let soon = start + Duration::from_millis(100);
        assert_eq!(counter.press(soon, at, PointerButton::Primary), 2);
        assert_eq!(counter.release(), 2);
        let far = Point::new(30.0, 10.0);
        assert_eq!(counter.press(soon, far, PointerButton::Primary), 1);
        let late = soon + Duration::from_secs(1);
        assert_eq!(counter.press(late, far, PointerButton::Primary), 1);
        assert_eq!(counter.press(late, far, PointerButton::Secondary), 1);
    }

    #[test]
    fn pinch_scales_become_changes_since_the_previous_event() {
        let mut pinch = PinchScale::default();
        assert_eq!(pinch.update(1.5), None, "no pinch is active");
        pinch.begin();
        assert_eq!(pinch.update(1.25), Some(1.25));
        assert_eq!(pinch.update(1.25), None, "the scale did not change");
        assert_eq!(pinch.update(2.5), Some(2.0));
        assert_eq!(pinch.update(1.25), Some(0.5));
        assert_eq!(pinch.update(0.0), None);
        assert_eq!(pinch.update(f64::NAN), None);
        assert_eq!(pinch.update(0.625), Some(0.5));
        pinch.end();
        assert_eq!(pinch.update(2.0), None, "the pinch ended");
    }

    #[test]
    fn pinch_changes_multiply_to_the_cumulative_scale() {
        let mut pinch = PinchScale::default();
        pinch.begin();
        let total: f64 = [1.1, 1.3, 0.9, 1.7]
            .into_iter()
            .filter_map(|scale| pinch.update(scale))
            .product();
        assert!((total - 1.7).abs() < 1e-12);
        pinch.begin();
        assert_eq!(pinch.update(0.5), Some(0.5), "a new pinch starts at 1");
    }

    #[test]
    fn button_codes_map_to_core_buttons() {
        assert_eq!(button(0x110), PointerButton::Primary);
        assert_eq!(button(0x111), PointerButton::Secondary);
        assert_eq!(button(0x112), PointerButton::Middle);
        assert_eq!(button(0x113), PointerButton::Other(3));
    }
}
