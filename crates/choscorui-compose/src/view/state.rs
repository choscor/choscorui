//! Copyable state handles, two-way bindings, and live values.
//!
//! A [`State`] is a small `Copy` handle to a [`Property`] that lives in a
//! per-thread slot table. The component that created it owns the slot,
//! and the slot is freed when the component's scope unmounts. A handle
//! that outlives its slot reports [`ReactiveError::Disposed`] and never
//! panics.

use crate::reactive::{Property, ReactiveError, Runtime};
use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

struct Entry {
    generation: u32,
    value: Option<Box<dyn Any>>,
}

#[derive(Default)]
struct Slots {
    entries: Vec<Entry>,
    free: Vec<u32>,
    /// Errors from handles used after their slot was freed.
    errors: Vec<ReactiveError>,
}

thread_local! {
    static SLOTS: RefCell<Slots> = RefCell::new(Slots::default());
}

/// A slot in the per-thread table, with the generation it was filled in.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SlotId {
    index: u32,
    generation: u32,
}

/// Stores `value` in a free slot.
pub(crate) fn allocate(value: Box<dyn Any>) -> SlotId {
    SLOTS.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(index) = slots.free.pop() {
            let entry = &mut slots.entries[index as usize];
            entry.value = Some(value);
            SlotId {
                index,
                generation: entry.generation,
            }
        } else {
            let index = u32::try_from(slots.entries.len()).unwrap_or(u32::MAX);
            slots.entries.push(Entry {
                generation: 0,
                value: Some(value),
            });
            SlotId {
                index,
                generation: 0,
            }
        }
    })
}

/// Empties a slot. Later reads through old handles report `Disposed`.
pub(crate) fn free(slot: SlotId) {
    SLOTS.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(entry) = slots.entries.get_mut(slot.index as usize)
            && entry.generation == slot.generation
            && entry.value.is_some()
        {
            entry.value = None;
            entry.generation = entry.generation.wrapping_add(1);
            slots.free.push(slot.index);
        }
    });
}

/// Clones the `T` stored in `slot`.
fn read<T: Clone + 'static>(slot: SlotId) -> Result<T, ReactiveError> {
    SLOTS.with(|slots| {
        let slots = slots.borrow();
        slots
            .entries
            .get(slot.index as usize)
            .filter(|entry| entry.generation == slot.generation)
            .and_then(|entry| entry.value.as_ref())
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
            .ok_or(ReactiveError::Disposed)
    })
}

/// Records an error from a handle that has no slot any more.
fn report(error: ReactiveError) {
    SLOTS.with(|slots| slots.borrow_mut().errors.push(error));
}

/// Takes the errors of handles used after their component unmounted.
pub fn take_handle_errors() -> Vec<ReactiveError> {
    SLOTS.with(|slots| std::mem::take(&mut slots.borrow_mut().errors))
}

/// A value that a component owns. Reading it inside a live property or a
/// memo records the read, and setting it updates only what read it.
///
/// `State` is `Copy`, so closures capture it without clones.
pub struct State<T> {
    slot: SlotId,
    _value: PhantomData<fn() -> T>,
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for State<T> {}

impl<T> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").field("slot", &self.slot).finish()
    }
}

impl<T> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot
    }
}

impl<T: Clone + PartialEq + 'static> State<T> {
    pub(crate) fn new(property: Property<T>) -> Self {
        Self {
            slot: allocate(Box::new(property)),
            _value: PhantomData,
        }
    }

    pub(crate) fn slot(&self) -> SlotId {
        self.slot
    }

    fn property(&self) -> Result<Property<T>, ReactiveError> {
        read::<Property<T>>(self.slot)
    }

    /// The value, or `Disposed` when the owning component has unmounted.
    pub fn try_get(&self) -> Result<T, ReactiveError> {
        self.property().map(|property| property.get())
    }

    /// The value. After the owning component unmounts, this reports
    /// `Disposed` (see [`take_handle_errors`]) and returns the default.
    pub fn get(&self) -> T
    where
        T: Default,
    {
        self.try_get().unwrap_or_else(|error| {
            report(error);
            T::default()
        })
    }

    /// Calls `read` with the value without cloning it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> Result<R, ReactiveError> {
        self.property().map(|property| property.with(read))
    }

    /// The value, without recording the read.
    pub fn get_untracked(&self) -> Result<T, ReactiveError> {
        self.property().map(|property| property.get_untracked())
    }

    /// Replaces the value. Dependents update when it differs.
    pub fn try_set(&self, value: T) -> Result<(), ReactiveError> {
        self.property().map(|property| property.set(value))
    }

    /// Replaces the value; after the owner unmounts, reports `Disposed`.
    pub fn set(&self, value: T) {
        if let Err(error) = self.try_set(value) {
            report(error);
        }
    }

    /// Changes the value in place; after the owner unmounts, reports
    /// `Disposed`.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        match self.property() {
            Ok(property) => property.update(change),
            Err(error) => report(error),
        }
    }

    /// A two-way binding to this state.
    pub fn binding(&self) -> Binding<T> {
        Binding {
            source: BindingSource::State(*self),
        }
    }

    /// The runtime that the state belongs to.
    pub fn runtime(&self) -> Result<Runtime, ReactiveError> {
        self.property().map(|property| property.runtime().clone())
    }
}

/// A read-only value computed from state. It computes again only when a
/// value that it read last time changes. `Memo` is `Copy`.
pub struct Memo<T>(State<T>);

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Memo<T> {}

impl<T> std::fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Memo").field(&self.0).finish()
    }
}

impl<T: Clone + PartialEq + 'static> Memo<T> {
    pub(crate) fn new(property: Property<T>) -> Self {
        Self(State::new(property))
    }

    pub(crate) fn slot(&self) -> SlotId {
        self.0.slot
    }

    /// The value, or `Disposed` when the owning component has unmounted.
    pub fn try_get(&self) -> Result<T, ReactiveError> {
        self.0.try_get()
    }

    /// The value; after the owner unmounts, reports `Disposed` and returns
    /// the default.
    pub fn get(&self) -> T
    where
        T: Default,
    {
        self.0.get()
    }

    /// Calls `read` with the value without cloning it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> Result<R, ReactiveError> {
        self.0.with(read)
    }
}

/// How a binding reads and writes its value.
trait Source<T> {
    fn get(&self) -> Result<T, ReactiveError>;
    fn set(&self, value: T) -> Result<(), ReactiveError>;
}

/// A binding through a lens into a larger value.
struct Projection<S, U> {
    parent: Binding<S>,
    lens: Rc<dyn Fn(&mut S) -> &mut U>,
}

impl<S, U> Source<U> for Projection<S, U>
where
    S: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
{
    fn get(&self) -> Result<U, ReactiveError> {
        let mut whole = self.parent.try_get()?;
        Ok((self.lens)(&mut whole).clone())
    }

    fn set(&self, value: U) -> Result<(), ReactiveError> {
        let mut whole = self.parent.try_get()?;
        *(self.lens)(&mut whole) = value;
        self.parent.try_set(whole)
    }
}

/// A binding made of two closures.
struct Custom<T> {
    get: Rc<dyn Fn() -> T>,
    set: Rc<dyn Fn(T)>,
}

impl<T> Source<T> for Custom<T> {
    fn get(&self) -> Result<T, ReactiveError> {
        Ok((self.get)())
    }

    fn set(&self, value: T) -> Result<(), ReactiveError> {
        (self.set)(value);
        Ok(())
    }
}

enum BindingSource<T> {
    State(State<T>),
    Slot(SlotId),
}

impl<T> Clone for BindingSource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for BindingSource<T> {}

/// A two-way handle to a value that something else owns: a [`State`], a
/// part of one, or a pair of closures. Controls that edit a value take a
/// binding. `Binding` is `Copy`.
pub struct Binding<T> {
    source: BindingSource<T>,
}

impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Binding<T> {}

impl<T> std::fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.source {
            BindingSource::State(state) => f.debug_tuple("Binding").field(&state).finish(),
            BindingSource::Slot(slot) => f.debug_tuple("Binding").field(&slot).finish(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Binding<T> {
    /// A binding that reads with `get` and writes with `set`. The
    /// component being mounted owns it.
    pub fn new(get: impl Fn() -> T + 'static, set: impl Fn(T) + 'static) -> Self {
        let source: Rc<dyn Source<T>> = Rc::new(Custom {
            get: Rc::new(get),
            set: Rc::new(set),
        });
        Self::owned(source)
    }

    fn owned(source: Rc<dyn Source<T>>) -> Self {
        let slot = allocate(Box::new(source));
        super::scope::own_slot(slot);
        Self {
            source: BindingSource::Slot(slot),
        }
    }

    /// The value, or `Disposed` when the owner has unmounted.
    pub fn try_get(&self) -> Result<T, ReactiveError> {
        match self.source {
            BindingSource::State(state) => state.try_get(),
            BindingSource::Slot(slot) => read::<Rc<dyn Source<T>>>(slot)?.get(),
        }
    }

    /// The value; after the owner unmounts, reports `Disposed` and returns
    /// the default.
    pub fn get(&self) -> T
    where
        T: Default,
    {
        self.try_get().unwrap_or_else(|error| {
            report(error);
            T::default()
        })
    }

    /// Writes the value, or returns `Disposed` when the owner has
    /// unmounted.
    pub fn try_set(&self, value: T) -> Result<(), ReactiveError> {
        match self.source {
            BindingSource::State(state) => state.try_set(value),
            BindingSource::Slot(slot) => read::<Rc<dyn Source<T>>>(slot)?.set(value),
        }
    }

    /// Writes the value; after the owner unmounts, reports `Disposed`.
    pub fn set(&self, value: T) {
        if let Err(error) = self.try_set(value) {
            report(error);
        }
    }

    /// Changes the value in place.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        match self.try_get() {
            Ok(mut value) => {
                change(&mut value);
                self.set(value);
            }
            Err(error) => report(error),
        }
    }

    /// A binding to the part of the value that `lens` selects, for
    /// example `person.map(|p| &mut p.name)`.
    pub fn map<U>(self, lens: impl Fn(&mut T) -> &mut U + 'static) -> Binding<U>
    where
        U: Clone + PartialEq + 'static,
    {
        Binding::owned(Rc::new(Projection {
            parent: self,
            lens: Rc::new(lens),
        }))
    }
}

/// A view property: a fixed value, or a closure that reads state and runs
/// again when that state changes.
pub enum Live<T> {
    Value(T),
    Computed(Rc<dyn Fn() -> T>),
}

impl<T: Clone> Clone for Live<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Value(value) => Self::Value(value.clone()),
            Self::Computed(compute) => Self::Computed(compute.clone()),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Live<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(value) => f.debug_tuple("Value").field(value).finish(),
            Self::Computed(_) => f.write_str("Computed"),
        }
    }
}

impl<T: Clone + 'static> Live<T> {
    /// The current value. Inside a live property, the reads are recorded.
    pub fn get(&self) -> T {
        match self {
            Self::Value(value) => value.clone(),
            Self::Computed(compute) => compute(),
        }
    }

    /// True when the value can change.
    pub fn is_computed(&self) -> bool {
        matches!(self, Self::Computed(_))
    }

    /// A live value that applies `map` to this one.
    pub fn map<U: 'static>(self, map: impl Fn(T) -> U + 'static) -> Live<U> {
        match self {
            Self::Value(value) => Live::Value(map(value)),
            Self::Computed(compute) => Live::Computed(Rc::new(move || map(compute()))),
        }
    }
}

/// Marks a fixed value for [`IntoLive`].
pub struct Fixed;

/// Marks a closure for [`IntoLive`].
pub struct Reading;

/// Marks a state handle for [`IntoLive`].
pub struct Handle;

/// Converts a value, a closure, or a state handle into a [`Live`]
/// property. `M` only tells the conversions apart.
pub trait IntoLive<T, M> {
    /// The value as a live property.
    fn into_live(self) -> Live<T>;
}

impl<T, V: Into<T>> IntoLive<T, Fixed> for V {
    fn into_live(self) -> Live<T> {
        Live::Value(self.into())
    }
}

impl<T, R, F> IntoLive<T, Reading> for F
where
    F: Fn() -> R + 'static,
    R: Into<T>,
{
    fn into_live(self) -> Live<T> {
        Live::Computed(Rc::new(move || self().into()))
    }
}

impl<T> IntoLive<T, Handle> for Live<T> {
    fn into_live(self) -> Live<T> {
        self
    }
}

impl<T> IntoLive<T, Handle> for State<T>
where
    T: Clone + PartialEq + Default + 'static,
{
    fn into_live(self) -> Live<T> {
        Live::Computed(Rc::new(move || self.get()))
    }
}

impl<T> IntoLive<T, Handle> for Binding<T>
where
    T: Clone + PartialEq + Default + 'static,
{
    fn into_live(self) -> Live<T> {
        Live::Computed(Rc::new(move || self.get()))
    }
}

impl<T> IntoLive<T, Handle> for Memo<T>
where
    T: Clone + PartialEq + Default + 'static,
{
    fn into_live(self) -> Live<T> {
        Live::Computed(Rc::new(move || self.get()))
    }
}
