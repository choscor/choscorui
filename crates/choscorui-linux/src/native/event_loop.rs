//! Work from other threads and timers on the UI thread.
//!
//! `Poster::post` queues a closure and writes the eventfd that the event
//! loop watches. The loop runs the closures in FIFO order. Timers keep a
//! deadline; the loop arms its timerfd for the earliest one and runs each
//! due timer. A panic in a closure or a timer stops at this boundary and
//! is counted.

use crate::ffi::libc::{EventFd, TimerFd};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type PostedTask = Box<dyn FnOnce() + Send + 'static>;

struct PostState {
    tasks: VecDeque<PostedTask>,
    /// False after the application drops: `post` then fails.
    open: bool,
    /// True after `request_stop`, until `run` returns.
    stop: bool,
    panics: usize,
}

pub(crate) struct PostInner {
    state: Mutex<PostState>,
    wake: EventFd,
}

/// Sends work to the UI thread from any thread.
#[derive(Clone)]
pub struct Poster {
    inner: Arc<PostInner>,
}

/// Why `Poster::post` failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostError {
    /// The application has stopped.
    Stopped,
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => f.write_str("the application event loop has stopped"),
        }
    }
}

impl std::error::Error for PostError {}

impl Poster {
    /// Queues `task` to run on the UI thread. After the application
    /// drops, it returns `PostError::Stopped` and drops `task`.
    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> Result<(), PostError> {
        let mut state = self.inner.state.lock().map_err(|_| PostError::Stopped)?;
        if !state.open {
            return Err(PostError::Stopped);
        }
        state.tasks.push_back(Box::new(task));
        drop(state);
        let _ = self.inner.wake.signal();
        Ok(())
    }

    /// Asks `Application::run` to return.
    pub fn request_stop(&self) -> Result<(), PostError> {
        let mut state = self.inner.state.lock().map_err(|_| PostError::Stopped)?;
        if !state.open {
            return Err(PostError::Stopped);
        }
        state.stop = true;
        drop(state);
        let _ = self.inner.wake.signal();
        Ok(())
    }
}

/// The posted work of an application.
pub(crate) struct Posted {
    inner: Arc<PostInner>,
}

impl Posted {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            inner: Arc::new(PostInner {
                state: Mutex::new(PostState {
                    tasks: VecDeque::new(),
                    open: true,
                    stop: false,
                    panics: 0,
                }),
                wake: EventFd::new()?,
            }),
        })
    }

    pub(crate) fn poster(&self) -> Poster {
        Poster {
            inner: Arc::clone(&self.inner),
        }
    }

    /// The descriptor that becomes readable when work is posted.
    pub(crate) fn wake(&self) -> &EventFd {
        &self.inner.wake
    }

    /// Runs the queued tasks in order, including tasks that they post.
    /// Returns true when a task ran.
    pub(crate) fn run(&self) -> bool {
        self.inner.wake.clear();
        let mut ran = false;
        loop {
            let task = match self.inner.state.lock() {
                Ok(mut state) => state.tasks.pop_front(),
                Err(_) => return ran,
            };
            let Some(task) = task else {
                return ran;
            };
            ran = true;
            if catch_unwind(AssertUnwindSafe(task)).is_err()
                && let Ok(mut state) = self.inner.state.lock()
            {
                state.panics += 1;
            }
        }
    }

    /// Takes a stop request of `Poster::request_stop`.
    pub(crate) fn take_stop(&self) -> bool {
        self.inner
            .state
            .lock()
            .is_ok_and(|mut state| std::mem::replace(&mut state.stop, false))
    }

    /// Asks the loop to stop, from the UI thread.
    pub(crate) fn stop(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.stop = true;
        }
        let _ = self.inner.wake.signal();
    }

    pub(crate) fn panics(&self) -> usize {
        self.inner.state.lock().map_or(0, |state| state.panics)
    }
}

impl Drop for Posted {
    fn drop(&mut self) {
        // Later posts fail, and the waiting tasks drop without running.
        if let Ok(mut state) = self.inner.state.lock() {
            state.open = false;
            state.tasks.clear();
        }
    }
}

enum TimerAction {
    Once(Box<dyn FnOnce()>),
    Repeat(Box<dyn FnMut()>),
}

pub(crate) struct TimerInner {
    deadline: Cell<Instant>,
    interval: Option<Duration>,
    action: RefCell<Option<TimerAction>>,
    active: Cell<bool>,
}

impl TimerInner {
    fn cancel(&self) {
        self.active.set(false);
        self.action.borrow_mut().take();
    }
}

/// A scheduled callback. Dropping or canceling it stops further calls,
/// also from inside the callback.
pub struct Timer {
    inner: Rc<TimerInner>,
}

impl Timer {
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    pub fn is_active(&self) -> bool {
        self.inner.active.get()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.inner.cancel();
    }
}

/// The timers of an application and the timerfd of the earliest one.
pub(crate) struct Timers {
    timers: RefCell<Vec<Weak<TimerInner>>>,
    fd: TimerFd,
    panics: Cell<usize>,
}

impl Timers {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            timers: RefCell::new(Vec::new()),
            fd: TimerFd::new()?,
            panics: Cell::new(0),
        })
    }

    pub(crate) fn fd(&self) -> &TimerFd {
        &self.fd
    }

    pub(crate) fn once(&self, delay: Duration, action: impl FnOnce() + 'static) -> Timer {
        self.add(delay, None, TimerAction::Once(Box::new(action)))
    }

    pub(crate) fn repeating(&self, interval: Duration, action: impl FnMut() + 'static) -> Timer {
        self.add(
            interval,
            Some(interval),
            TimerAction::Repeat(Box::new(action)),
        )
    }

    fn add(&self, delay: Duration, interval: Option<Duration>, action: TimerAction) -> Timer {
        let inner = Rc::new(TimerInner {
            deadline: Cell::new(Instant::now() + delay),
            interval,
            action: RefCell::new(Some(action)),
            active: Cell::new(true),
        });
        let mut timers = self.timers.borrow_mut();
        timers.retain(|timer| timer.upgrade().is_some_and(|timer| timer.active.get()));
        timers.push(Rc::downgrade(&inner));
        drop(timers);
        self.arm();
        Timer { inner }
    }

    /// The deadline of the earliest active timer.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.timers
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|timer| timer.active.get())
            .map(|timer| timer.deadline.get())
            .min()
    }

    /// Arms the timerfd for the earliest deadline.
    pub(crate) fn arm(&self) {
        let delay = self
            .next_deadline()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        let _ = self.fd.arm(delay);
    }

    /// Runs each due timer. A repeating timer skips missed ticks instead
    /// of bursting. Returns true when a timer ran.
    pub(crate) fn run_due(&self) -> bool {
        self.fd.clear();
        let now = Instant::now();
        let due: Vec<Rc<TimerInner>> = self
            .timers
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|timer| timer.active.get() && timer.deadline.get() <= now)
            .collect();
        for timer in &due {
            self.fire(timer, now);
        }
        self.arm();
        !due.is_empty()
    }

    fn fire(&self, timer: &Rc<TimerInner>, now: Instant) {
        let action = timer.action.borrow_mut().take();
        let Some(action) = action else {
            return;
        };
        match action {
            TimerAction::Once(action) => {
                timer.active.set(false);
                if catch_unwind(AssertUnwindSafe(action)).is_err() {
                    self.panics.set(self.panics.get() + 1);
                }
            }
            TimerAction::Repeat(mut action) => {
                if let Some(interval) = timer.interval {
                    let mut next = timer.deadline.get() + interval;
                    while next <= now {
                        next += interval;
                    }
                    timer.deadline.set(next);
                }
                if catch_unwind(AssertUnwindSafe(&mut action)).is_err() {
                    self.panics.set(self.panics.get() + 1);
                }
                // A cancel from inside the callback cleared `active`.
                if timer.active.get() {
                    *timer.action.borrow_mut() = Some(TimerAction::Repeat(action));
                }
            }
        }
    }

    pub(crate) fn panics(&self) -> usize {
        self.panics.get()
    }

    /// Cancels every timer, when the application drops.
    pub(crate) fn cancel_all(&self) {
        for timer in self.timers.borrow_mut().drain(..) {
            if let Some(timer) = timer.upgrade() {
                timer.cancel();
            }
        }
    }
}
