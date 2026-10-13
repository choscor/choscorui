use super::*;

pub(crate) type PostedTask = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct PostState {
    pub(crate) source: usize,
    pub(crate) tasks: VecDeque<PostedTask>,
    pub(crate) panics: usize,
}

pub(crate) struct PostInner {
    pub(crate) application: usize,
    pub(crate) runloop: usize,
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
        if state.source == 0 {
            return Err(PostError::Stopped);
        }
        state.tasks.push_back(Box::new(task));
        // SAFETY: The mutex excludes Application::drop, which removes and
        // releases the source. The run loop is process-owned and still live.
        unsafe {
            CFRunLoopSourceSignal(state.source as *mut c_void);
            CFRunLoopWakeUp(self.inner.runloop as *mut c_void);
        }
        Ok(())
    }

    /// Requests exit from `Application::run` through the main run loop.
    pub fn request_stop(&self) -> Result<(), PostError> {
        let application = self.inner.application;
        self.post(move || {
            // SAFETY: post executes only while Application is alive, on its
            // main run loop.
            unsafe { stop_application(application as Id) }
        })
    }
}

pub(crate) unsafe extern "C" fn perform_posted(info: *mut c_void) {
    // SAFETY: info owns an Arc strong count until the source is removed and
    // released on the main thread. This callback runs only on that thread.
    let inner = unsafe { &*(info as *const PostInner) };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        loop {
            let task = match inner.state.lock() {
                Ok(mut state) => state.tasks.pop_front(),
                Err(_) => return,
            };
            let Some(task) = task else {
                return;
            };
            if catch_unwind(AssertUnwindSafe(task)).is_err()
                && let Ok(mut state) = inner.state.lock()
            {
                state.panics += 1;
            }
        }
    }));
}

pub(crate) enum TimerAction {
    Once(Box<dyn FnOnce()>),
    Repeat(Box<dyn FnMut()>),
}

pub(crate) struct TimerContext {
    pub(crate) owner: Weak<TimerInner>,
    pub(crate) action: RefCell<Option<TimerAction>>,
    pub(crate) panics: Rc<Cell<usize>>,
}

pub(crate) struct TimerInner {
    pub(crate) raw: Cell<usize>,
    pub(crate) info: Cell<usize>,
    pub(crate) runloop: usize,
    pub(crate) active: Cell<bool>,
}

impl TimerInner {
    pub(crate) fn cancel(&self) {
        if self.active.replace(false) {
            // SAFETY: TimerInner owns a live +1 CF timer on the main thread.
            unsafe { CFRunLoopTimerInvalidate(self.raw.get() as *mut c_void) }
        }
        if self.info.get() != 0 {
            // SAFETY: TimerInner owns this context until Drop; callback cannot
            // overlap cancellation because both run on the main thread.
            unsafe {
                (&*(self.info.get() as *const TimerContext))
                    .action
                    .borrow_mut()
                    .take();
            }
        }
    }
}

impl Drop for TimerInner {
    fn drop(&mut self) {
        if self.raw.get() == 0 {
            return;
        }
        self.cancel();
        // SAFETY: CF timer was created +1 and added to this run loop. Removal
        // and release happen after the last Rust handle and callback finish.
        unsafe {
            let raw = self.raw.get() as *mut c_void;
            CFRunLoopRemoveTimer(self.runloop as *mut c_void, raw, kCFRunLoopCommonModes);
            ffi::legacy::cf_release(raw);
            drop(Box::from_raw(self.info.get() as *mut TimerContext));
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

pub(crate) unsafe extern "C" fn fire_timer(_: *mut c_void, info: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: TimerInner keeps this Box alive through the callback. The
        // upgraded Rc prevents a user action from freeing it during delivery.
        let context = unsafe { &*(info as *const TimerContext) };
        let Some(owner) = context.owner.upgrade() else {
            return;
        };
        let action = context.action.borrow_mut().take();
        match action {
            Some(TimerAction::Once(action)) => {
                owner.active.set(false);
                // SAFETY: One-shot timer is no longer needed after firing.
                unsafe { CFRunLoopTimerInvalidate(owner.raw.get() as *mut c_void) }
                if catch_unwind(AssertUnwindSafe(action)).is_err() {
                    context.panics.set(context.panics.get() + 1);
                }
            }
            Some(TimerAction::Repeat(mut action)) => {
                if catch_unwind(AssertUnwindSafe(&mut action)).is_err() {
                    context.panics.set(context.panics.get() + 1);
                }
                // Restore unless the callback canceled its own timer.
                if owner.active.get() {
                    *context.action.borrow_mut() = Some(TimerAction::Repeat(action));
                }
            }
            None => {}
        }
    }));
}
