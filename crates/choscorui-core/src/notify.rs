//! State-change notifications and UI-thread work scheduling.
//!
//! Controls never call application code while the tree is borrowed. They
//! queue a [`Notification`] that names their retained node; the application
//! drains the queue after dispatch returns. Workers and timers likewise queue
//! closures that run later on the UI thread with exclusive access to state.

use crate::{NodeId, Tree};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A value change or command request reported by a control.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification<K> {
    pub source: NodeId,
    pub kind: K,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotifyError {
    /// Delivery produced more cascaded notifications than the configured
    /// limit, which indicates a feedback loop between handlers.
    CascadeLimit,
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CascadeLimit => {
                f.write_str("notifications kept triggering each other past the cascade limit")
            }
        }
    }
}

impl std::error::Error for NotifyError {}

/// FIFO notification queue with removal-safe, reentrant delivery.
///
/// Handlers may push more notifications; they are delivered in the same
/// drain after earlier ones. A notification whose source was detached before
/// delivery is dropped, so no handler receives a stale node reference.
pub struct NotificationQueue<K> {
    pending: VecDeque<Notification<K>>,
    cascade_limit: usize,
}

impl<K> Default for NotificationQueue<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> NotificationQueue<K> {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            cascade_limit: 10_000,
        }
    }

    pub fn with_cascade_limit(cascade_limit: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            cascade_limit,
        }
    }

    pub fn push(&mut self, source: NodeId, kind: K) {
        self.pending.push_back(Notification { source, kind });
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Delivers queued notifications in order. The handler receives the
    /// queue so it can report follow-up changes. Returns the number
    /// delivered; stale sources are skipped and not counted.
    pub fn drain<S>(
        &mut self,
        state: &mut S,
        tree: impl Fn(&S) -> &Tree,
        mut handler: impl FnMut(&mut S, &mut Self, Notification<K>),
    ) -> Result<usize, NotifyError> {
        let mut delivered = 0;
        while let Some(notification) = self.pending.pop_front() {
            if !tree(state).contains(notification.source) {
                continue;
            }
            delivered += 1;
            if delivered > self.cascade_limit {
                self.pending.clear();
                return Err(NotifyError::CascadeLimit);
            }
            handler(state, self, notification);
        }
        Ok(delivered)
    }
}

/// Identity of a scheduled timer. Identities are never reused by a queue.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimerId(u64);

type UiTask<S> = Box<dyn FnOnce(&mut S) + Send>;
type TimerTask<S> = Box<dyn FnMut(&mut S) + Send>;

struct ScheduledTimer<S> {
    id: TimerId,
    interval: Option<Duration>,
    task: TimerTask<S>,
}

struct Shared<S> {
    posted: VecDeque<UiTask<S>>,
    closed: bool,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkError {
    /// The UI queue was shut down; the task was not queued.
    Closed,
    InvalidInterval,
}

impl std::fmt::Display for WorkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => f.write_str("the work queue is closed"),
            Self::InvalidInterval => f.write_str("the interval must be positive and finite"),
        }
    }
}

impl std::error::Error for WorkError {}

/// Worker-side handle. It is `Send + Clone` and can only enqueue work; the
/// work runs later on the UI thread inside [`WorkQueue::run_ready`].
pub struct WorkSender<S> {
    shared: Arc<Mutex<Shared<S>>>,
}

impl<S> Clone for WorkSender<S> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<S> WorkSender<S> {
    pub fn post(&self, task: impl FnOnce(&mut S) + Send + 'static) -> Result<(), WorkError> {
        let wake = {
            let mut shared = self.shared.lock().map_err(|_| WorkError::Closed)?;
            if shared.closed {
                return Err(WorkError::Closed);
            }
            shared.posted.push_back(Box::new(task));
            shared.wake.clone()
        };
        if let Some(wake) = wake {
            wake();
        }
        Ok(())
    }
}

/// Deterministic UI-thread scheduler for posted work and timers.
///
/// Ordering per [`run_ready`](Self::run_ready): all work posted before the
/// call runs first, FIFO; then due timers in (deadline, creation) order.
/// Work posted while running is deferred to the next call, so one call is
/// bounded. Time is an injected monotonic [`Duration`] so tests and the
/// platform loop share the same rules.
pub struct WorkQueue<S> {
    shared: Arc<Mutex<Shared<S>>>,
    timers: BTreeMap<(Duration, u64), ScheduledTimer<S>>,
    deadlines: BTreeMap<TimerId, (Duration, u64)>,
    next_timer: u64,
    sequence: u64,
}

impl<S> Default for WorkQueue<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> WorkQueue<S> {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                posted: VecDeque::new(),
                closed: false,
                wake: None,
            })),
            timers: BTreeMap::new(),
            deadlines: BTreeMap::new(),
            next_timer: 1,
            sequence: 0,
        }
    }

    /// Installs the platform wake-up called after each worker post.
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.wake = Some(Arc::new(wake));
        }
    }

    pub fn sender(&self) -> WorkSender<S> {
        WorkSender {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn schedule_once(
        &mut self,
        now: Duration,
        delay: Duration,
        task: impl FnOnce(&mut S) + Send + 'static,
    ) -> TimerId {
        let mut task = Some(task);
        self.insert_timer(now + delay, None, move |state| {
            if let Some(task) = task.take() {
                task(state);
            }
        })
    }

    /// Repeats every `interval` after `now + interval`. A late run schedules
    /// the next deadline from the previous deadline, skipping missed ticks
    /// rather than bursting to catch up.
    pub fn schedule_repeating(
        &mut self,
        now: Duration,
        interval: Duration,
        task: impl FnMut(&mut S) + Send + 'static,
    ) -> Result<TimerId, WorkError> {
        if interval.is_zero() {
            return Err(WorkError::InvalidInterval);
        }
        Ok(self.insert_timer(now + interval, Some(interval), task))
    }

    fn insert_timer(
        &mut self,
        deadline: Duration,
        interval: Option<Duration>,
        task: impl FnMut(&mut S) + Send + 'static,
    ) -> TimerId {
        let id = TimerId(self.next_timer);
        self.next_timer += 1;
        let key = (deadline, self.sequence);
        self.sequence += 1;
        self.timers.insert(
            key,
            ScheduledTimer {
                id,
                interval,
                task: Box::new(task),
            },
        );
        self.deadlines.insert(id, key);
        id
    }

    /// Cancels future delivery. Returns false if the timer already finished.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        match self.deadlines.remove(&id) {
            Some(key) => self.timers.remove(&key).is_some(),
            None => false,
        }
    }

    pub fn is_scheduled(&self, id: TimerId) -> bool {
        self.deadlines.contains_key(&id)
    }

    /// The earliest timer deadline, for the platform's sleep timeout.
    pub fn next_deadline(&self) -> Option<Duration> {
        self.timers.keys().next().map(|(deadline, _)| *deadline)
    }

    pub fn has_posted_work(&self) -> bool {
        self.shared
            .lock()
            .map(|shared| !shared.posted.is_empty())
            .unwrap_or(false)
    }

    /// Runs ready work and returns how many tasks ran.
    pub fn run_ready(&mut self, now: Duration, state: &mut S) -> usize {
        let posted: Vec<UiTask<S>> = match self.shared.lock() {
            Ok(mut shared) => shared.posted.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        let mut ran = posted.len();
        for task in posted {
            task(state);
        }
        let due: Vec<(Duration, u64)> = self
            .timers
            .range(..=(now, u64::MAX))
            .map(|(key, _)| *key)
            .collect();
        for key in due {
            // A previous timer task cannot cancel this one through `self`,
            // because tasks only receive application state.
            let Some(mut timer) = self.timers.remove(&key) else {
                continue;
            };
            self.deadlines.remove(&timer.id);
            (timer.task)(state);
            ran += 1;
            if let Some(interval) = timer.interval {
                let mut next = key.0 + interval;
                while next <= now {
                    next += interval;
                }
                let next_key = (next, self.sequence);
                self.sequence += 1;
                self.deadlines.insert(timer.id, next_key);
                self.timers.insert(next_key, timer);
            }
        }
        ran
    }

    /// Rejects further posts and drops queued work and timers.
    pub fn shutdown(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.closed = true;
            shared.posted.clear();
            shared.wake = None;
        }
        self.timers.clear();
        self.deadlines.clear();
    }
}

impl<S> Drop for WorkQueue<S> {
    fn drop(&mut self) {
        self.shutdown();
    }
}
