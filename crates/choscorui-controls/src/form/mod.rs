//! Form controls that mount through the control host: switches, toggles,
//! toggle groups, key caps, and selects.

mod kbd;
mod select;
mod switch;
mod toggle;

pub use kbd::Kbd;
pub use select::Select;
pub use switch::Switch;
pub use toggle::{Toggle, ToggleGroup, ToggleItem};
