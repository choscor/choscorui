#![deny(unsafe_code)]

use super::*;
use crate::ffi::appkit::{OTHER_EVENT, POST_EVENT_AT_START, SHARED_APPLICATION, STOP};
use crate::ffi::runloop::{Fire, MainRunLoop, Perform, RunLoopTimer, SourceSignal};

pub(crate) type PostedTask = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct PostState {
    /// The signal of the posting source, or `None` once the application
    /// stopped.
    pub(crate) source: Option<SourceSignal>,
    pub(crate) tasks: VecDeque<PostedTask>,
    pub(crate) panics: usize,
}

pub(crate) struct PostInner {
    pub(crate) runloop: MainRunLoop,
    pub(crate) state: Mutex<PostState>,
}

/// Worker-safe path to queue work on the AppKit main run loop.
#[derive(Clone)]
pub struct Poster {
    pub(crate) inner: Arc<PostInner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostError {
    Stopped,
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => f.write_str("the application run loop has stopped"),
        }
    }
}

impl std::error::Error for PostError {}

impl Poster {
    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> Result<(), PostError> {
        let mut state = self.inner.state.lock().map_err(|_| PostError::Stopped)?;
        if state.source.is_none() {
            return Err(PostError::Stopped);
        }
        state.tasks.push_back(Box::new(task));
        // The mutex excludes Application::drop, which clears the signal.
        if let Some(source) = &state.source {
            source.signal();
        }
        self.inner.runloop.wake_up();
        Ok(())
    }

    /// Requests exit from `Application::run` through the main run loop.
    pub fn request_stop(&self) -> Result<(), PostError> {
        // Posted work runs on the main run loop of the application.
        self.post(stop_application)
    }
}

impl Perform for PostInner {
    fn perform(&self) {
        loop {
            let task = match self.state.lock() {
                Ok(mut state) => state.tasks.pop_front(),
                Err(_) => return,
            };
            let Some(task) = task else {
                return;
            };
            if catch_unwind(AssertUnwindSafe(task)).is_err()
                && let Ok(mut state) = self.state.lock()
            {
                state.panics += 1;
            }
        }
    }
}

/// `NSEventTypeApplicationDefined`.
const APPLICATION_DEFINED_EVENT: usize = 15;

/// Stops `[NSApp run]` on the main thread. AppKit checks the stop flag only
/// after `run` handles an event, so an application-defined event follows
/// `stop:`. Without it, a stop from a run loop source waits for the next
/// input.
pub(crate) fn stop_application() {
    let _pool = Pool::new();
    let Some(application) = SHARED_APPLICATION.send_class(()).ok().flatten() else {
        return;
    };
    STOP.send(application, (None,));
    let origin = NSPoint { x: 0.0, y: 0.0 };
    let event = OTHER_EVENT
        .send_class((APPLICATION_DEFINED_EVENT, origin, 0, 0.0, 0, None, 0, 0, 0))
        .ok()
        .flatten();
    if let Some(event) = event {
        POST_EVENT_AT_START.send(application, (&event, true));
    }
}

pub(crate) enum TimerAction {
    Once(Box<dyn FnOnce()>),
    Repeat(Box<dyn FnMut()>),
}

pub(crate) struct TimerInner {
    /// The native timer, or `None` when Core Foundation could not create
    /// it.
    pub(crate) timer: Option<RunLoopTimer<TimerInner>>,
    pub(crate) action: RefCell<Option<TimerAction>>,
    pub(crate) panics: Rc<Cell<usize>>,
    pub(crate) active: Cell<bool>,
}

impl TimerInner {
    pub(crate) fn cancel(&self) {
        if self.active.replace(false)
            && let Some(timer) = &self.timer
        {
            timer.invalidate();
        }
        // A callback in progress has taken its action out first.
        self.action.borrow_mut().take();
    }
}

impl Fire for TimerInner {
    fn fire(&self) {
        let action = self.action.borrow_mut().take();
        match action {
            Some(TimerAction::Once(action)) => {
                self.active.set(false);
                // A one-shot timer is no longer needed after firing.
                if let Some(timer) = &self.timer {
                    timer.invalidate();
                }
                if catch_unwind(AssertUnwindSafe(action)).is_err() {
                    self.panics.set(self.panics.get() + 1);
                }
            }
            Some(TimerAction::Repeat(mut action)) => {
                if catch_unwind(AssertUnwindSafe(&mut action)).is_err() {
                    self.panics.set(self.panics.get() + 1);
                }
                // Restore unless the callback canceled its own timer.
                if self.active.get() {
                    *self.action.borrow_mut() = Some(TimerAction::Repeat(action));
                }
            }
            None => {}
        }
    }
}

/// Main-thread one-shot or repeating timer. Dropping or canceling it
/// prevents further delivery, including from inside its own callback.
pub struct Timer {
    pub(crate) inner: Rc<TimerInner>,
}

impl Timer {
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn is_active(&self) -> bool {
        self.inner.active.get()
    }
}
