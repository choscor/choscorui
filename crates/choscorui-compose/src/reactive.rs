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
}

impl std::fmt::Display for ReactiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle => f.write_str("a computed value depends on itself"),
            Self::DifferentRuntime => f.write_str("the binding belongs to a different runtime"),
            Self::Compute(message) => write!(f, "computation failed: {message}"),
        }
    }
}

impl std::error::Error for ReactiveError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Binding {
    source: NodeId,
    target: NodeId,
}

struct NodeCallbacks {
    internal: Vec<Callback>,
    observers: Vec<Callback>,
}

#[derive(Default)]
struct State {
    next_id: NodeId,
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

#[derive(Clone, Default)]
pub struct Runtime {
    state: Rc<RefCell<State>>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn batch<R>(&self, operation: impl FnOnce() -> R) -> R {
        self.state.borrow_mut().depth += 1;
        let guard = BatchDepth(self);
        let result = operation();
        drop(guard);
        self.flush_if_ready();
        result
    }

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

    pub fn bind<A, T>(
        &self,
        source: &Property<A>,
        target: &Property<T>,
        map: impl Fn(&A) -> Result<T, ReactiveError> + 'static,
    ) -> Result<Binding, ReactiveError>
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
        Ok(Binding {
            source: source.id,
            target: target.id,
        })
    }

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
        state.callbacks.insert(
            id,
            NodeCallbacks {
                internal: Vec::new(),
                observers: Vec::new(),
            },
        );
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
        self.state
            .borrow_mut()
            .callbacks
            .get_mut(&source)
            .expect("source property exists")
            .internal
            .push(callback);
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
        loop {
            loop {
                let callbacks = {
                    let mut state = self.state.borrow_mut();
                    let pending = std::mem::take(&mut state.pending);
                    pending
                        .into_iter()
                        .flat_map(|id| state.callbacks[&id].internal.clone())
                        .collect::<Vec<_>>()
                };
                if callbacks.is_empty() {
                    break;
                }
                for callback in callbacks {
                    callback();
                }
            }
            let observers = {
                let mut state = self.state.borrow_mut();
                let notify = std::mem::take(&mut state.notify);
                notify
                    .into_iter()
                    .flat_map(|id| state.callbacks[&id].observers.clone())
                    .collect::<Vec<_>>()
            };
            for observer in observers {
                observer();
            }
            if self.state.borrow().pending.is_empty() {
                break;
            }
        }
        self.state.borrow_mut().flushing = false;
    }
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
    pub(crate) fn identity(&self) -> u64 {
        self.id
    }

    pub fn get(&self) -> T {
        self.value.borrow().clone()
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

    pub fn subscribe(&self, observer: impl Fn(&T) + 'static) {
        let value = self.value.clone();
        self.runtime
            .state
            .borrow_mut()
            .callbacks
            .get_mut(&self.id)
            .expect("property exists")
            .observers
            .push(Rc::new(move || {
                let current = value.borrow().clone();
                observer(&current);
            }));
    }
}
