//! Form controls that mount through the control host: switches, toggles,
//! toggle groups, and key caps.

mod kbd;
mod switch;
mod toggle;

pub use kbd::Kbd;
pub use switch::Switch;
pub use toggle::{Toggle, ToggleGroup, ToggleItem};
