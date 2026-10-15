//! Touch input as the primary pointer, for both display servers.
//!
//! Core has no touch event type, and the controls handle pointer events
//! only. Thus, the first touch point that goes down acts as the primary
//! button of the pointer. The touch points that go down while it is
//! down have no effect. A cancel of the sequence releases the button
//! outside the window, so that no control activates.

use choscorui_core::{Point, PointerButton, PointerEvent};

/// Turns the touch points of a seat into the pointer events of one
/// primary touch point.
#[derive(Debug, Default)]
pub(crate) struct TouchPointer {
    /// The id and the latest position of the primary touch point.
    primary: Option<(u32, Point)>,
}

impl TouchPointer {
    /// A touch point `id` goes down at `position`. Gives `Down` when no
    /// other touch point is the primary one.
    pub(crate) fn down(&mut self, id: u32, position: Point) -> Option<PointerEvent> {
        if self.primary.is_some() {
            return None;
        }
        self.primary = Some((id, position));
        Some(PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        })
    }

    /// The touch point `id` moves to `position`. Gives `Move` for the
    /// primary touch point when its position changes.
    pub(crate) fn motion(&mut self, id: u32, position: Point) -> Option<PointerEvent> {
        let (primary, latest) = self.primary.as_mut()?;
        if *primary != id || *latest == position {
            return None;
        }
        *latest = position;
        Some(PointerEvent::Move { position })
    }

    /// The touch point `id` goes up. Gives `Up` at the latest position of
    /// the primary touch point.
    pub(crate) fn up(&mut self, id: u32) -> Option<PointerEvent> {
        match self.primary {
            Some((primary, position)) if primary == id => {
                self.primary = None;
                Some(PointerEvent::Up {
                    position,
                    button: PointerButton::Primary,
                })
            }
            _ => None,
        }
    }

    /// The display server cancels all touch points of the seat, as when
    /// it takes the sequence for a gesture. Gives `Up` far outside the
    /// window: a control that captured the pointer releases it, but the
    /// release does not hit a control and thus activates nothing.
    pub(crate) fn cancel(&mut self) -> Option<PointerEvent> {
        self.primary.take()?;
        Some(PointerEvent::Up {
            position: cancelled_position(),
            button: PointerButton::Primary,
        })
    }

    /// True while the primary touch point is down.
    pub(crate) fn is_down(&self) -> bool {
        self.primary.is_some()
    }
}

/// The release position of a cancelled touch sequence. It is finite, so
/// the arithmetic of a control stays defined, and no window is this large.
fn cancelled_position() -> Point {
    Point::new(-1.0e9, -1.0e9)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: PointerButton = PointerButton::Primary;

    fn at(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_touch_sequence_is_a_press_a_drag_and_a_release() {
        let mut touch = TouchPointer::default();
        assert_eq!(
            touch.down(7, at(10.0, 20.0)),
            Some(PointerEvent::Down {
                position: at(10.0, 20.0),
                button: PRIMARY,
            })
        );
        assert!(touch.is_down());
        assert_eq!(
            touch.motion(7, at(12.0, 21.0)),
            Some(PointerEvent::Move {
                position: at(12.0, 21.0),
            })
        );
        assert_eq!(touch.motion(7, at(12.0, 21.0)), None, "no change");
        assert_eq!(
            touch.up(7),
            Some(PointerEvent::Up {
                position: at(12.0, 21.0),
                button: PRIMARY,
            })
        );
        assert!(!touch.is_down());
        assert_eq!(touch.up(7), None, "the sequence ended");
        assert_eq!(touch.motion(7, at(1.0, 1.0)), None);
    }

    #[test]
    fn only_the_first_touch_point_drives_the_pointer() {
        let mut touch = TouchPointer::default();
        assert!(touch.down(1, at(5.0, 5.0)).is_some());
        assert_eq!(touch.down(2, at(50.0, 50.0)), None, "a second finger");
        assert_eq!(touch.motion(2, at(60.0, 60.0)), None);
        assert_eq!(touch.up(2), None);
        assert_eq!(
            touch.up(1),
            Some(PointerEvent::Up {
                position: at(5.0, 5.0),
                button: PRIMARY,
            })
        );
        // A touch point that went down while another was primary does
        // not become primary later.
        assert_eq!(touch.motion(2, at(70.0, 70.0)), None);
        assert!(
            touch.down(3, at(8.0, 8.0)).is_some(),
            "a new touch point after the release is primary"
        );
    }

    #[test]
    fn a_cancel_releases_the_primary_touch_point_outside_the_window() {
        let mut touch = TouchPointer::default();
        assert_eq!(touch.cancel(), None, "nothing is down");
        assert!(touch.down(4, at(3.0, 4.0)).is_some());
        assert!(touch.motion(4, at(6.0, 8.0)).is_some());
        assert_eq!(
            touch.cancel(),
            Some(PointerEvent::Up {
                position: at(-1.0e9, -1.0e9),
                button: PRIMARY,
            }),
            "the release hits no control, so nothing activates"
        );
        assert!(!touch.is_down());
        assert_eq!(touch.up(4), None, "the cancel ended the sequence");
    }
}
