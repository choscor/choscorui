//! Ownership of state slots, effects, and computed properties. Each
//! structural scope (the root, each `Dynamic` branch, each `ForEach`
//! item) owns what its components created, and frees it on unmount.

use super::state::{self, SlotId};
use crate::reactive::{Runtime, Subscription};
use std::cell::RefCell;
use std::rc::Rc;

/// What one scope owns.
#[derive(Default)]
pub(crate) struct Owned {
    slots: Vec<SlotId>,
    effects: Vec<(Runtime, Subscription)>,
    properties: Vec<(Runtime, u64)>,
}

impl Owned {
    pub(crate) fn add_property(&mut self, runtime: &Runtime, property: u64, slot: SlotId) {
        self.properties.push((runtime.clone(), property));
        self.slots.push(slot);
    }

    pub(crate) fn add_effect(&mut self, runtime: &Runtime, effect: Subscription) {
        self.effects.push((runtime.clone(), effect));
    }

    /// Frees every slot and stops every effect and computation.
    pub(crate) fn dispose(&mut self) {
        for (runtime, effect) in self.effects.drain(..) {
            runtime.unsubscribe(effect);
        }
        for (runtime, property) in self.properties.drain(..) {
            runtime.dispose(property);
        }
        for slot in self.slots.drain(..) {
            state::free(slot);
        }
    }
}

thread_local! {
    /// The scopes of the components being mounted, innermost last.
    static OWNERS: RefCell<Vec<Rc<RefCell<Owned>>>> = const { RefCell::new(Vec::new()) };
    /// Slots made outside any mount, for example by `Binding::map` in an
    /// action. They live as long as the thread.
    static DETACHED: RefCell<Owned> = RefCell::new(Owned::default());
}

/// Makes `owned` the scope of what is created until the guard drops.
pub(crate) fn enter(owned: Rc<RefCell<Owned>>) -> OwnerGuard {
    OWNERS.with(|owners| owners.borrow_mut().push(owned));
    OwnerGuard
}

pub(crate) struct OwnerGuard;

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        OWNERS.with(|owners| owners.borrow_mut().pop());
    }
}

/// Gives `slot` to the innermost scope being mounted.
pub(crate) fn own_slot(slot: SlotId) {
    let owner = OWNERS.with(|owners| owners.borrow().last().cloned());
    match owner {
        Some(owner) => owner.borrow_mut().slots.push(slot),
        None => DETACHED.with(|detached| detached.borrow_mut().slots.push(slot)),
    }
}
