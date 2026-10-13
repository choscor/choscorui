//! The bounds, step, and page of a range view. The builders of
//! `Slider`, `SpinBox`, `RangeSlider`, `Dial`, and `Progress` set them
//! before the view mounts. A builder that gets an invalid value uses the
//! nearest valid value and keeps a note. The view reports each note as
//! [`ViewError::Clamped`](super::ViewError::Clamped) when it mounts.

use super::configured::Configured;
use choscorui_controls::{ControlValue, CustomControl, IntegerRange, RangeError};
use std::cell::Cell;
use std::rc::Rc;

/// The limits of a range view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Limits {
    pub(crate) minimum: f64,
    pub(crate) maximum: f64,
    pub(crate) step: f64,
    pub(crate) page: f64,
    /// A value past one end goes to the other end, as on a compass dial.
    pub(crate) wrapping: bool,
}

impl Limits {
    /// `minimum..=maximum` with a step of 1 and a page of 10.
    pub(crate) const fn new(minimum: f64, maximum: f64) -> Self {
        Self {
            minimum,
            maximum,
            step: 1.0,
            page: 10.0,
            wrapping: false,
        }
    }

    /// The integer range of these limits, with `value` clamped into it.
    /// The builders keep the minimum at most the maximum and the steps
    /// positive, so it fails only for limits outside the `i32` range.
    pub(crate) fn integers(&self, value: i32) -> Result<IntegerRange, RangeError> {
        let whole = |number: f64| {
            number
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX))
        };
        let step = |number: f64| number.round().clamp(1.0, f64::from(u32::MAX)) as u32;
        IntegerRange::new(
            whole(self.minimum) as i32,
            whole(self.maximum) as i32,
            value,
            step(self.step),
            step(self.page),
        )
    }

    /// The nearest value inside the limits, or `None` when `value` is
    /// inside them or is not a number.
    pub(crate) fn clamp(&self, value: &ControlValue) -> Option<ControlValue> {
        let inside = |number: f64| number.clamp(self.minimum, self.maximum);
        match value {
            ControlValue::Number(number) if inside(*number) != *number => {
                Some(ControlValue::Number(inside(*number)))
            }
            ControlValue::Range(low, high) => {
                let (low, high) = (inside(*low), inside(*high));
                let clamped = ControlValue::Range(low.min(high), high.max(low));
                (clamped != *value).then_some(clamped)
            }
            _ => None,
        }
    }
}

/// The limits that a range view and its build share.
pub(crate) type SharedLimits = Rc<Cell<Limits>>;

impl<C: CustomControl + 'static> Configured<C> {
    /// Sets the bounds to `minimum..=maximum`. Reversed bounds become
    /// `minimum..=minimum`, and bounds that are not finite keep the
    /// bounds that the view has. Both keep a note.
    pub(super) fn set_bounds(self, minimum: f64, maximum: f64) -> Self {
        self.change_limits(|limits| {
            if !minimum.is_finite() || !maximum.is_finite() {
                return Some(format!("the bounds {minimum}..={maximum} are not finite"));
            }
            limits.minimum = minimum;
            if minimum > maximum {
                limits.maximum = minimum;
                return Some(format!(
                    "the bounds {minimum}..={maximum} are reversed; the view uses {minimum}..={minimum}"
                ));
            }
            limits.maximum = maximum;
            None
        })
    }

    /// Sets the step. A step that is not positive and finite becomes 1,
    /// with a note.
    pub(super) fn set_step(self, step: f64) -> Self {
        self.change_limits(|limits| {
            let valid = step.is_finite() && step > 0.0;
            limits.step = if valid { step } else { 1.0 };
            (!valid).then(|| format!("the step {step} is not positive; the view uses 1"))
        })
    }

    /// Sets the page step. A page that is not positive and finite becomes
    /// 1, with a note.
    pub(super) fn set_page(self, page: f64) -> Self {
        self.change_limits(|limits| {
            let valid = page.is_finite() && page > 0.0;
            limits.page = if valid { page } else { 1.0 };
            (!valid).then(|| format!("the page {page} is not positive; the view uses 1"))
        })
    }

    /// Lets the value go past one end to the other end.
    pub(super) fn set_wrapping(self, wrapping: bool) -> Self {
        self.change_limits(|limits| {
            limits.wrapping = wrapping;
            None
        })
    }
}

/// The `range`, `step`, and `page` builders of an integer range view.
macro_rules! integer_limits {
    ($($control:ty),+) => {$(
        impl Configured<$control> {
            /// The bounds of the value. Reversed bounds become
            /// `start..=start` and report a
            /// [`ViewError::Clamped`](super::ViewError::Clamped) when the
            /// view mounts.
            #[must_use]
            pub fn range(self, bounds: std::ops::RangeInclusive<i32>) -> Self {
                self.set_bounds(f64::from(*bounds.start()), f64::from(*bounds.end()))
            }

            /// The change of one arrow key. A step of zero becomes 1 and
            /// reports a [`ViewError::Clamped`](super::ViewError::Clamped).
            #[must_use]
            pub fn step(self, step: u32) -> Self {
                self.set_step(f64::from(step))
            }

            /// The change of one page key. A page of zero becomes 1 and
            /// reports a [`ViewError::Clamped`](super::ViewError::Clamped).
            #[must_use]
            pub fn page(self, page: u32) -> Self {
                self.set_page(f64::from(page))
            }
        }
    )+};
}

integer_limits!(
    choscorui_controls::Slider,
    choscorui_controls::SpinBox,
    choscorui_controls::HostedDial
);
