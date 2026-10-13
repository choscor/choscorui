use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
    fmt,
    rc::Rc,
};

type NodeId = u64;
type Callback = Rc<dyn Fn()>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReactiveError {
    Cycle,
    DifferentRuntime,
    Compute(String),
    /// A state handle outlived the component that created it.
    Disposed,
}

impl std::fmt::Display for ReactiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle => f.write_str("a computed value depends on itself"),
            Self::DifferentRuntime => f.write_str("the binding belongs to a different runtime"),
            Self::Compute(message) => write!(f, "computation failed: {message}"),
            Self::Disposed => f.write_str("the state belongs to a component that has unmounted"),
        }
    }
}

impl std::error::Error for ReactiveError {}

/// A one-way connection that [`Runtime::bind`] made between two
/// properties.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyBinding {
    source: NodeId,
    target: NodeId,
}

/// A registered callback, removable through its [`Subscription`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Subscription(u64);

#[derive(Default)]
struct NodeCallbacks {
    /// Run while changes propagate, before any observer: derived values.
    internal: Vec<(Subscription, Callback)>,
    /// Run once per flush after propagation settles.
    observers: Vec<(Subscription, Callback)>,
}

/// The most rounds of propagation in one flush. More means that values
/// keep changing each other, which is a cycle.
const MAX_ROUNDS: usize = 10_000;

#[derive(Default)]
struct State {
    next_id: NodeId,
    next_subscription: u64,
    /// The sources of each subscription, to remove it.
    subscriptions: HashMap<Subscription, Vec<NodeId>>,
    /// Each subscription's callback, with whether it runs during
    /// propagation.
    retained: HashMap<Subscription, (Callback, bool)>,
    /// The subscription that fills each computed property.
    owned: HashMap<NodeId, Subscription>,
    depth: usize,
    flushing: bool,
    pending: BTreeSet<NodeId>,
    notify: BTreeSet<NodeId>,
    edges: HashMap<NodeId, HashSet<NodeId>>,
    callbacks: HashMap<NodeId, NodeCallbacks>,
    errors: Vec<ReactiveError>,
}

struct BatchDepth<'a>(&'a Runtime);

impl Drop for BatchDepth<'_> {
    fn drop(&mut self) {
        self.0.state.borrow_mut().depth -= 1;
    }
}

thread_local! {
    /// The properties read inside each running [`Runtime::track`], the
    /// innermost last.
    static TRACKING: RefCell<Vec<(usize, Vec<NodeId>)>> = const { RefCell::new(Vec::new()) };
}

/// Ends a tracking frame even when the tracked code panics.
struct TrackFrame;

impl TrackFrame {
    /// Starts recording reads for runtime `key` until the frame drops.
    /// Kept out of line so that each generic caller only calls it.
    #[inline(never)]
    fn push(key: usize) -> Self {
        TRACKING.with(|frames| frames.borrow_mut().push((key, Vec::new())));
        Self
    }
}

impl Drop for TrackFrame {
    #[inline(never)]
    fn drop(&mut self) {
        TRACKING.with(|frames| frames.borrow_mut().pop());
    }
}

/// The reactive graph: typed properties and the connections between
/// them. Clones share one graph. The views of all windows of an app use
/// one runtime.
#[derive(Clone, Default)]
pub struct Runtime {
    state: Rc<RefCell<State>>,
}

impl Runtime {
    /// An empty runtime.
    pub fn new() -> Self {
        Self::default()
    }

    /// A new property with the start value `value`.
    pub fn property<T>(&self, value: T) -> Property<T>
    where
        T: Clone + PartialEq + 'static,
    {
        let id = self.allocate_node();
        Property {
            id,
            value: Rc::new(RefCell::new(value)),
            runtime: self.clone(),
        }
    }

    /// Runs `operation` and sends the change notifications once, after
    /// it returns.
    pub fn batch<R>(&self, operation: impl FnOnce() -> R) -> R {
        self.state.borrow_mut().depth += 1;
        let guard = BatchDepth(self);
        let result = operation();
        drop(guard);
        self.flush_if_ready();
        result
    }

    /// A property that `compute` calculates from `source`, again after
    /// each change of `source`.
    pub fn derive1<A, T>(
        &self,
        source: &Property<A>,
        compute: impl Fn(&A) -> Result<T, ReactiveError> + 'static,
    ) -> Result<Property<T>, ReactiveError>
    where
        A: Clone + PartialEq + 'static,
        T: Clone + PartialEq + 'static,
    {
        self.require_runtime(source)?;
        let compute = Rc::new(compute);
        let initial = compute(&source.get())?;
        let output = self.property(initial);
        self.connect(&[source.id], output.id)?;
        let input = source.clone();
        let target = output.clone();
        let runtime = self.clone();
        self.add_internal(
            source.id,
            Rc::new(move || match compute(&input.get()) {
                Ok(value) => target.set(value),
                Err(error) => runtime.state.borrow_mut().errors.push(error),
            }),
        );
        Ok(output)
    }

    /// A property that `compute` calculates from two sources, again
    /// after each change of one of them.
    pub fn derive2<A, B, T>(
        &self,
        first: &Property<A>,
        second: &Property<B>,
        compute: impl Fn(&A, &B) -> Result<T, ReactiveError> + 'static,
    ) -> Result<Property<T>, ReactiveError>
    where
        A: Clone + PartialEq + 'static,
        B: Clone + PartialEq + 'static,
        T: Clone + PartialEq + 'static,
    {
        self.require_runtime(first)?;
        self.require_runtime(second)?;
        let compute = Rc::new(compute);
        let initial = compute(&first.get(), &second.get())?;
        let output = self.property(initial);
        self.connect(&[first.id, second.id], output.id)?;
        for source_id in [first.id, second.id] {
            let first = first.clone();
            let second = second.clone();
            let target = output.clone();
            let compute = compute.clone();
            let runtime = self.clone();
            self.add_internal(
                source_id,
                Rc::new(move || match compute(&first.get(), &second.get()) {
                    Ok(value) => target.set(value),
                    Err(error) => runtime.state.borrow_mut().errors.push(error),
                }),
            );
        }
        Ok(output)
    }

    /// Writes `map` of `source` into `target` now and after each change
    /// of `source`.
    pub fn bind<A, T>(
        &self,
        source: &Property<A>,
        target: &Property<T>,
        map: impl Fn(&A) -> Result<T, ReactiveError> + 'static,
    ) -> Result<PropertyBinding, ReactiveError>
    where
        A: Clone + PartialEq + 'static,
        T: Clone + PartialEq + 'static,
    {
        self.require_runtime(source)?;
        self.require_runtime(target)?;
        let initial = map(&source.get())?;
        self.connect(&[source.id], target.id)?;
        target.set(initial);
        let input = source.clone();
        let output = target.clone();
        let runtime = self.clone();
        self.add_internal(
            source.id,
            Rc::new(move || match map(&input.get()) {
                Ok(value) => output.set(value),
                Err(error) => runtime.state.borrow_mut().errors.push(error),
            }),
        );
        Ok(PropertyBinding {
            source: source.id,
            target: target.id,
        })
    }

    /// Runs `operation` and returns the properties of this runtime that
    /// it read, in first-read order. Reads in a nested `track` belong to
    /// the nested call only.
    pub fn track<R>(&self, operation: impl FnOnce() -> R) -> (R, Vec<u64>) {
        let frame = TrackFrame::push(self.key());
        let result = operation();
        let reads = TRACKING.with(|frames| {
            frames
                .borrow_mut()
                .last_mut()
                .map(|(_, reads)| std::mem::take(reads))
                .unwrap_or_default()
        });
        drop(frame);
        (result, reads)
    }

    /// Runs `operation` without recording its reads in any enclosing
    /// [`Runtime::track`].
    pub fn untracked<R>(operation: impl FnOnce() -> R) -> R {
        // No runtime has key 0, so the reads go nowhere.
        let frame = TrackFrame::push(0);
        let result = operation();
        drop(frame);
        result
    }

    /// Calls `callback` once after each flush in which one of `sources`
    /// changed. Unknown sources are ignored.
    pub fn observe(&self, sources: &[u64], callback: impl Fn() + 'static) -> Subscription {
        self.subscribe_to(sources, Rc::new(callback), false)
    }

    /// Removes a callback. Removing it twice does nothing.
    pub fn unsubscribe(&self, subscription: Subscription) {
        let mut state = self.state.borrow_mut();
        state.retained.remove(&subscription);
        let Some(sources) = state.subscriptions.remove(&subscription) else {
            return;
        };
        for source in sources {
            if let Some(callbacks) = state.callbacks.get_mut(&source) {
                callbacks.internal.retain(|(id, _)| *id != subscription);
                callbacks.observers.retain(|(id, _)| *id != subscription);
            }
        }
    }

    /// A property that holds `compute`'s result. It records the
    /// properties that `compute` reads and computes again, before any
    /// observer runs, whenever one of them changes.
    pub fn computed<T>(&self, compute: impl Fn() -> T + 'static) -> Property<T>
    where
        T: Clone + PartialEq + 'static,
    {
        let (initial, reads) = self.track(&compute);
        let output = self.property(initial);
        let slot: Rc<RefCell<Option<Subscription>>> = Rc::default();
        let recompute: Callback = {
            let runtime = self.clone();
            let output = output.clone();
            let slot = slot.clone();
            Rc::new(move || {
                let (value, reads) = runtime.track(&compute);
                output.set(value);
                if let Some(subscription) = *slot.borrow() {
                    runtime.resubscribe(subscription, &reads);
                }
            })
        };
        let subscription = self.subscribe_to(&reads, recompute, true);
        self.state
            .borrow_mut()
            .owned
            .insert(output.id, subscription);
        *slot.borrow_mut() = Some(subscription);
        output
    }

    /// Runs `effect` now and again after each flush in which a property
    /// that it read last time changed. Unsubscribe to stop it.
    pub fn effect(&self, effect: impl Fn() + 'static) -> Subscription {
        let (_, reads) = self.track(&effect);
        let slot: Rc<RefCell<Option<Subscription>>> = Rc::default();
        let rerun: Callback = {
            let runtime = self.clone();
            let slot = slot.clone();
            Rc::new(move || {
                let (_, reads) = runtime.track(&effect);
                if let Some(subscription) = *slot.borrow() {
                    runtime.resubscribe(subscription, &reads);
                }
            })
        };
        let subscription = self.subscribe_to(&reads, rerun, false);
        *slot.borrow_mut() = Some(subscription);
        subscription
    }

    /// Forgets a property: its callbacks, its connections, and the
    /// computation that fills it. Handles to it keep their last value but
    /// no longer notify anyone.
    pub fn dispose(&self, property: u64) {
        let owned = {
            let mut state = self.state.borrow_mut();
            state.edges.remove(&property);
            for targets in state.edges.values_mut() {
                targets.remove(&property);
            }
            state.callbacks.remove(&property);
            state.owned.remove(&property)
        };
        if let Some(subscription) = owned {
            self.unsubscribe(subscription);
        }
    }

    fn key(&self) -> usize {
        Rc::as_ptr(&self.state) as usize
    }

    fn subscribe_to(&self, sources: &[u64], callback: Callback, internal: bool) -> Subscription {
        let subscription = {
            let mut state = self.state.borrow_mut();
            let subscription = Subscription(state.next_subscription);
            state.next_subscription += 1;
            state.retained.insert(subscription, (callback, internal));
            subscription
        };
        self.attach(subscription, sources);
        subscription
    }

    /// Adds a subscription's callback to each known source.
    fn attach(&self, subscription: Subscription, sources: &[u64]) {
        let mut state = self.state.borrow_mut();
        let Some((callback, internal)) = state.retained.get(&subscription).cloned() else {
            return;
        };
        let mut known = Vec::new();
        for source in sources {
            if let Some(callbacks) = state.callbacks.get_mut(source) {
                let list = if internal {
                    &mut callbacks.internal
                } else {
                    &mut callbacks.observers
                };
                list.push((subscription, callback.clone()));
                known.push(*source);
            }
        }
        state.subscriptions.insert(subscription, known);
    }

    /// Moves a tracked callback to the sources it read this time, keeping
    /// its subscription.
    fn resubscribe(&self, subscription: Subscription, reads: &[u64]) {
        {
            let mut state = self.state.borrow_mut();
            let Some(sources) = state.subscriptions.get(&subscription).cloned() else {
                return;
            };
            if sources == reads {
                return;
            }
            for source in sources {
                if let Some(callbacks) = state.callbacks.get_mut(&source) {
                    callbacks.internal.retain(|(id, _)| *id != subscription);
                    callbacks.observers.retain(|(id, _)| *id != subscription);
                }
            }
        }
        self.attach(subscription, reads);
    }

    /// Takes the errors that derived properties and bindings reported.
    pub fn take_errors(&self) -> Vec<ReactiveError> {
        std::mem::take(&mut self.state.borrow_mut().errors)
    }

    fn allocate_node(&self) -> NodeId {
        let mut state = self.state.borrow_mut();
        let id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .expect("property identity exhausted");
        state.callbacks.insert(id, NodeCallbacks::default());
        id
    }

    fn require_runtime<T>(&self, property: &Property<T>) -> Result<(), ReactiveError>
    where
        T: Clone + PartialEq + 'static,
    {
        if Rc::ptr_eq(&self.state, &property.runtime.state) {
            Ok(())
        } else {
            Err(ReactiveError::DifferentRuntime)
        }
    }

    fn connect(&self, sources: &[NodeId], target: NodeId) -> Result<(), ReactiveError> {
        let mut state = self.state.borrow_mut();
        if sources
            .iter()
            .any(|source| *source == target || reachable(&state.edges, target, *source))
        {
            return Err(ReactiveError::Cycle);
        }
        for source in sources {
            state.edges.entry(*source).or_default().insert(target);
        }
        Ok(())
    }

    fn add_internal(&self, source: NodeId, callback: Callback) {
        self.subscribe_to(&[source], callback, true);
    }

    fn changed(&self, id: NodeId) {
        {
            let mut state = self.state.borrow_mut();
            state.pending.insert(id);
            state.notify.insert(id);
        }
        self.flush_if_ready();
    }

    fn flush_if_ready(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.depth > 0 || state.flushing {
                return;
            }
            state.flushing = true;
        }
        let mut rounds = 0;
        loop {
            loop {
                let callbacks = {
                    let mut state = self.state.borrow_mut();
                    let pending = std::mem::take(&mut state.pending);
                    collect(&state.callbacks, pending, |callbacks| &callbacks.internal)
                };
                if callbacks.is_empty() {
                    break;
                }
                for callback in callbacks {
                    callback();
                }
                rounds += 1;
                if rounds > MAX_ROUNDS {
                    self.abandon();
                    return;
                }
            }
            let observers = {
                let mut state = self.state.borrow_mut();
                let notify = std::mem::take(&mut state.notify);
                collect(&state.callbacks, notify, |callbacks| &callbacks.observers)
            };
            for observer in observers {
                observer();
            }
            if self.state.borrow().pending.is_empty() {
                break;
            }
            rounds += 1;
            if rounds > MAX_ROUNDS {
                self.abandon();
                return;
            }
        }
        self.state.borrow_mut().flushing = false;
    }

    /// Stops a flush that does not settle and reports the cycle.
    fn abandon(&self) {
        let mut state = self.state.borrow_mut();
        state.pending.clear();
        state.notify.clear();
        state.errors.push(ReactiveError::Cycle);
        state.flushing = false;
    }
}

/// The callbacks of `ids`, each subscription once, in subscription order.
fn collect(
    callbacks: &HashMap<NodeId, NodeCallbacks>,
    ids: BTreeSet<NodeId>,
    list: impl Fn(&NodeCallbacks) -> &Vec<(Subscription, Callback)>,
) -> Vec<Callback> {
    let mut unique = std::collections::BTreeMap::new();
    for id in ids {
        if let Some(entry) = callbacks.get(&id) {
            for (subscription, callback) in list(entry) {
                unique
                    .entry(*subscription)
                    .or_insert_with(|| callback.clone());
            }
        }
    }
    unique.into_values().collect()
}

fn reachable(edges: &HashMap<NodeId, HashSet<NodeId>>, start: NodeId, goal: NodeId) -> bool {
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    while let Some(node) = pending.pop() {
        if node == goal {
            return true;
        }
        if visited.insert(node) {
            pending.extend(edges.get(&node).into_iter().flatten().copied());
        }
    }
    false
}

pub struct Property<T> {
    id: NodeId,
    value: Rc<RefCell<T>>,
    runtime: Runtime,
}

impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

impl<T> fmt::Debug for Property<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Property")
            .field("id", &self.id)
            .field("value", &self.value.borrow())
            .finish()
    }
}

impl<T> Property<T>
where
    T: Clone + PartialEq + 'static,
{
    /// The property's identity in its runtime, as [`Runtime::track`]
    /// reports it.
    pub fn identity(&self) -> u64 {
        self.id
    }

    /// The value. Inside [`Runtime::track`], the read is recorded.
    pub fn get(&self) -> T {
        self.record();
        self.value.borrow().clone()
    }

    /// Calls `read` with the value, without cloning it. Inside
    /// [`Runtime::track`], the read is recorded.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.record();
        read(&self.value.borrow())
    }

    /// The value, without recording the read.
    pub fn get_untracked(&self) -> T {
        self.value.borrow().clone()
    }

    /// Changes the value in place and notifies when it differs.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        let mut value = self.get_untracked();
        change(&mut value);
        self.set(value);
    }

    fn record(&self) {
        let key = self.runtime.key();
        TRACKING.with(|frames| {
            if let Some((runtime, reads)) = frames.borrow_mut().last_mut()
                && *runtime == key
                && !reads.contains(&self.id)
            {
                reads.push(self.id);
            }
        });
    }

    pub fn set(&self, value: T) {
        let changed = {
            let mut current = self.value.borrow_mut();
            if *current == value {
                false
            } else {
                *current = value;
                true
            }
        };
        if changed {
            self.runtime.changed(self.id);
        }
    }

    /// Calls `observer` with the value after each flush in which it
    /// changed.
    pub fn subscribe(&self, observer: impl Fn(&T) + 'static) -> Subscription {
        let value = self.value.clone();
        self.runtime.observe(&[self.id], move || {
            let current = value.borrow().clone();
            observer(&current);
        })
    }

    /// The runtime that the property belongs to.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}
