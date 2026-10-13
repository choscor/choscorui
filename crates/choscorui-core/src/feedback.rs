//! [`Feedback`]: the signals that controls send to the person through the
//! platform while they handle input.

/// The platform signals that a control sends while it handles input. A
/// backend implements it. Each method does nothing by default, for a
/// backend that does not support the signal.
pub trait Feedback {
    /// Plays the system alert sound, for example when a text field
    /// refuses a key or a paste.
    fn beep(&self) {}
}

/// Feedback that sends no signal: the default of a host without a
/// backend.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoFeedback;

impl Feedback for NoFeedback {}
