//! Click counting and scroll conversion, for both display servers.

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
    fn button_codes_map_to_core_buttons() {
        assert_eq!(button(0x110), PointerButton::Primary);
        assert_eq!(button(0x111), PointerButton::Secondary);
        assert_eq!(button(0x112), PointerButton::Middle);
        assert_eq!(button(0x113), PointerButton::Other(3));
    }
}
