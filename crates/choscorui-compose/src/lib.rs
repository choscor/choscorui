//! Platform-independent Rust composition and reactive property primitives.

mod reactive;
mod reconcile;
mod states;
pub mod view;

pub use reactive::{Property, PropertyBinding, ReactiveError, Runtime, Subscription};
pub use reconcile::{ComposeError, Element, Key, ReconcileReport, Reconciler};
pub use states::{
    DeclarativeState, StateAssignment, StateError, StateMachine, StateName, TransitionDescription,
};
