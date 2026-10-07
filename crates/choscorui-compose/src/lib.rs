//! Platform-independent Rust composition and reactive property primitives.

mod reactive;
mod reconcile;
mod states;

pub use reactive::{Binding, Property, ReactiveError, Runtime};
pub use reconcile::{ComposeError, Element, Key, ReconcileReport, Reconciler};
pub use states::{
    DeclarativeState, StateAssignment, StateError, StateMachine, StateName, TransitionDescription,
};
