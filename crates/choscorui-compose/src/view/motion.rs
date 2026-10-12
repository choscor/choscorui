//! The open and close motion of a presented panel. A centered panel
//! fades in and zooms from 95%. An anchored panel also slides in from
//! its anchor. An edge panel slides in from its edge and follows a drag.
//! The scrim fades with the panel. Reduced motion shows the end state at
//! once.

use choscorui_controls::{Reveal, SheetSide};
use choscorui_core::{DesignTokens, Rect, Transform};
use std::cell::Cell;

/// How a presented panel moves while it opens and closes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Movement {
    /// Fade and zoom about the center.
    Zoom,
    /// Fade, zoom, and a slide from the anchor.
    Anchored,
    /// A slide from the edge of `side`.
    Edge(SheetSide),
}

/// The motion of one presented layer. The layer reads it to fade the
/// scrim and to time the close; the painter reads it to move the panel.
pub(crate) struct Motion {
    reveal: Cell<Reveal>,
    /// The duration of the panel's recipe, in milliseconds.
    duration: Cell<u32>,
    movement: Movement,
    /// Whether an anchored panel sits above its anchor.
    above: Cell<bool>,
    /// Whether the layout is right-to-left, from the last arrange.
    rtl: Cell<bool>,
    /// How far a drag moved the panel toward its edge, in points.
    drag: Cell<f64>,
}

impl Motion {
    /// The motion of a panel that opens at host `time`.
    pub(crate) fn opening(movement: Movement, duration: u32, time: f64) -> Self {
        let mut reveal = Reveal::default();
        reveal.set(true, time);
        Self {
            reveal: Cell::new(reveal),
            duration: Cell::new(duration),
            movement,
            above: Cell::new(false),
            rtl: Cell::new(false),
            drag: Cell::new(0.0),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.reveal.get().open
    }

    /// Starts the close at host `time`, with `duration` from the recipe.
    pub(crate) fn close(&self, duration: u32, time: f64) {
        let mut reveal = self.reveal.get();
        reveal.set(false, time);
        self.reveal.set(reveal);
        self.duration.set(duration);
    }

    /// The host time at which the close ends, while it runs.
    pub(crate) fn closes_at(&self) -> Option<f64> {
        let reveal = self.reveal.get();
        (!reveal.open).then(|| reveal.changed_at + f64::from(self.duration.get()) / 1000.0)
    }

    pub(crate) fn animating(&self, time: f64) -> bool {
        self.reveal.get().animating(self.duration.get(), time)
    }

    /// How far the panel shows at `time`, from 0 to 1, after easing.
    pub(crate) fn progress(&self, tokens: &DesignTokens, time: f64) -> f64 {
        self.reveal
            .get()
            .progress(tokens, self.duration.get(), time)
    }

    pub(crate) fn set_above(&self, above: bool) {
        self.above.set(above);
    }

    pub(crate) fn set_rtl(&self, rtl: bool) {
        self.rtl.set(rtl);
    }

    pub(crate) fn set_drag(&self, offset: f64) {
        self.drag.set(offset);
    }

    /// The unit vector from an edge panel toward its edge, or `None` for
    /// a panel that does not sit at an edge. Left and right are the
    /// leading and trailing edges, so they swap in right-to-left layouts.
    pub(crate) fn outward(&self) -> Option<(f64, f64)> {
        let Movement::Edge(side) = self.movement else {
            return None;
        };
        let start = if self.rtl.get() { 1.0 } else { -1.0 };
        Some(match side {
            SheetSide::Top => (0.0, -1.0),
            SheetSide::Bottom => (0.0, 1.0),
            SheetSide::Left => (start, 0.0),
            SheetSide::Right => (-start, 0.0),
        })
    }

    /// The opacity and transform of the panel at `bounds` at `time`.
    pub(crate) fn effect(
        &self,
        tokens: &DesignTokens,
        bounds: Rect,
        time: f64,
    ) -> (f64, Transform) {
        let progress = self.progress(tokens, time);
        if let Some((ux, uy)) = self.outward() {
            let extent = if ux == 0.0 {
                bounds.size.height
            } else {
                bounds.size.width
            };
            let distance = (1.0 - progress) * extent + self.drag.get();
            return (1.0, Transform::translate(ux * distance, uy * distance));
        }
        if progress >= 1.0 {
            return (1.0, Transform::IDENTITY);
        }
        let slide = match self.movement {
            Movement::Anchored => {
                let distance = tokens.spacing * 2.0;
                (
                    0.0,
                    if self.above.get() {
                        distance
                    } else {
                        -distance
                    },
                )
            }
            _ => (0.0, 0.0),
        };
        (
            progress.clamp(0.0, 1.0),
            Reveal::transform(bounds, progress, slide),
        )
    }
}
