//! The Core Foundation run loop: the main run loop, a source that other
//! threads signal, and timers.
//!
//! The callbacks are generic trampolines over [`Perform`] and [`Fire`].
//! Each one catches a panic, so no panic crosses into Core Foundation.

use super::cf::{CFString, CfRetained, cf_type};
use super::sdk::{c_functions, ffi_struct};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Weak;
use std::sync::Arc;

cf_type!(CFRunLoop, CFRunLoopSource, CFRunLoopTimer);

ffi_struct! {
    /// `CFRunLoopSourceContext` (version 0).
    struct CFRunLoopSourceContext = CFRunLoopSourceContext {
        version: isize,
        info: *const c_void,
        retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
        release: Option<unsafe extern "C" fn(*const c_void)>,
        copy_description as copyDescription:
            Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
        equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> u8>,
        hash: Option<unsafe extern "C" fn(*const c_void) -> usize>,
        schedule: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_void)>,
        cancel: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_void)>,
        perform: Option<unsafe extern "C" fn(*const c_void)>,
    }

    /// `CFRunLoopTimerContext`.
    struct CFRunLoopTimerContext = CFRunLoopTimerContext {
        version: isize,
        info: *const c_void,
        retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
        release: Option<unsafe extern "C" fn(*const c_void)>,
        copy_description as copyDescription:
            Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
    }
}

c_functions! {
    #[link(name = "CoreFoundation", kind = "framework")]
    statics {
        safe static kCFRunLoopDefaultMode: &'static CFString;
        safe static kCFRunLoopCommonModes: &'static CFString;
    }
    functions {
        safe fn CFRunLoopGetMain() -> Option<&'static CFRunLoop>;
        safe fn CFRunLoopWakeUp(runloop: &CFRunLoop);
        safe fn CFRunLoopRunInMode(mode: &CFString, seconds: f64, return_after_source: u8) -> i32;
        unsafe fn CFRunLoopSourceCreate(
            allocator: *const c_void,
            order: isize,
            context: *mut CFRunLoopSourceContext,
        ) -> Option<CfRetained<CFRunLoopSource>>;
        safe fn CFRunLoopAddSource(runloop: &CFRunLoop, source: &CFRunLoopSource, mode: &CFString);
        safe fn CFRunLoopRemoveSource(runloop: &CFRunLoop, source: &CFRunLoopSource, mode: &CFString);
        safe fn CFRunLoopSourceSignal(source: &CFRunLoopSource);
        safe fn CFRunLoopSourceInvalidate(source: &CFRunLoopSource);
        unsafe fn CFRunLoopTimerCreate(
            allocator: *const c_void,
            fire_date: f64,
            interval: f64,
            flags: usize,
            order: isize,
            callback: unsafe extern "C" fn(*mut c_void, *const c_void),
            context: *mut CFRunLoopTimerContext,
        ) -> Option<CfRetained<CFRunLoopTimer>>;
        safe fn CFRunLoopAddTimer(runloop: &CFRunLoop, timer: &CFRunLoopTimer, mode: &CFString);
        safe fn CFRunLoopRemoveTimer(runloop: &CFRunLoop, timer: &CFRunLoopTimer, mode: &CFString);
        safe fn CFRunLoopTimerInvalidate(timer: &CFRunLoopTimer);
        safe fn CFAbsoluteTimeGetCurrent() -> f64;
    }
}

/// The main run loop, which any thread may wake.
#[derive(Clone, Copy)]
pub(crate) struct MainRunLoop(&'static CFRunLoop);

// SAFETY: the main run loop lives for the whole process. Other threads only
// wake it, and `CFRunLoopWakeUp` is thread-safe.
unsafe impl Send for MainRunLoop {}
// SAFETY: as above.
unsafe impl Sync for MainRunLoop {}

impl MainRunLoop {
    /// The main run loop, or `None` when Core Foundation has none.
    pub(crate) fn get() -> Option<Self> {
        CFRunLoopGetMain().map(Self)
    }

    /// Wakes the run loop so that it handles signaled sources.
    pub(crate) fn wake_up(&self) {
        CFRunLoopWakeUp(self.0);
    }

    /// Handles at most one source or timer in the default mode of the
    /// current thread's run loop, waiting up to `seconds`.
    pub(crate) fn run_once(seconds: f64) {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 1);
    }
}

/// The work of a run loop source, done on the run loop's thread.
pub(crate) trait Perform: Send + Sync + 'static {
    fn perform(&self);
}

/// The `perform` callback of a [`RunLoopSource<T>`]. It holds its own
/// strong count of `T` for the call.
///
/// # Safety
///
/// `info` is the `Arc<T>` count that the source holds until it is
/// invalidated.
unsafe extern "C" fn perform<T: Perform>(info: *const c_void) {
    let info = info.cast::<T>();
    // SAFETY: the source holds a strong count of this `Arc`.
    unsafe { Arc::increment_strong_count(info) };
    // SAFETY: the count taken above, released when `info` drops.
    let info = unsafe { Arc::from_raw(info) };
    let _ = catch_unwind(AssertUnwindSafe(|| info.perform()));
}

/// A run loop source in the common modes of the main run loop. It owns
/// one strong count of its `T`, and it invalidates the source in `Drop`
/// before it gives up that count.
pub(crate) struct RunLoopSource<T: Perform> {
    source: CfRetained<CFRunLoopSource>,
    runloop: MainRunLoop,
    info: NonNull<T>,
}

impl<T: Perform> RunLoopSource<T> {
    /// A new source that calls `info.perform()` when signaled, added to the
    /// common modes of `runloop`.
    pub(crate) fn new(runloop: MainRunLoop, info: Arc<T>) -> Option<Self> {
        let info = NonNull::new(Arc::into_raw(info).cast_mut())?;
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: info.as_ptr().cast_const().cast(),
            retain: None,
            release: None,
            copy_description: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(perform::<T>),
        };
        // SAFETY: Core Foundation copies the context. The source keeps the
        // `T` alive through `info` until `Drop` invalidates the source.
        let Some(source) = (unsafe { CFRunLoopSourceCreate(std::ptr::null(), 0, &mut context) })
        else {
            // SAFETY: `info` came from `Arc::into_raw` above and no source
            // uses it.
            drop(unsafe { Arc::from_raw(info.as_ptr()) });
            return None;
        };
        CFRunLoopAddSource(runloop.0, &source, kCFRunLoopCommonModes);
        Some(Self {
            source,
            runloop,
            info,
        })
    }

    /// A handle that other threads use to signal the source.
    pub(crate) fn signal(&self) -> SourceSignal {
        SourceSignal(CfRetained::retain(&self.source))
    }
}

impl<T: Perform> Drop for RunLoopSource<T> {
    fn drop(&mut self) {
        CFRunLoopRemoveSource(self.runloop.0, &self.source, kCFRunLoopCommonModes);
        // After invalidation the source never calls `perform` again.
        CFRunLoopSourceInvalidate(&self.source);
        // SAFETY: `info` came from `Arc::into_raw` in `new`, and the
        // invalidated source no longer uses it.
        drop(unsafe { Arc::from_raw(self.info.as_ptr()) });
    }
}

/// A reference to a run loop source that any thread may signal.
pub(crate) struct SourceSignal(CfRetained<CFRunLoopSource>);

// SAFETY: `CFRunLoopSourceSignal`, `CFRetain`, and `CFRelease` are
// thread-safe, and signaling an invalidated source does nothing.
unsafe impl Send for SourceSignal {}
// SAFETY: as above.
unsafe impl Sync for SourceSignal {}

impl SourceSignal {
    /// Marks the source ready; the run loop handles it after a wake-up.
    pub(crate) fn signal(&self) {
        CFRunLoopSourceSignal(&self.0);
    }
}

/// The work of a timer, done on the main thread.
pub(crate) trait Fire: 'static {
    fn fire(&self);
}

/// The callback of a [`RunLoopTimer<T>`]. It upgrades the owner for the
/// call, so the owner may drop the timer from inside its own callback.
///
/// # Safety
///
/// `info` is the `Weak<T>` that the timer keeps alive until it is
/// released.
unsafe extern "C" fn fire<T: Fire>(_: *mut c_void, info: *const c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the caller guarantees that `info` is a live `Weak<T>`;
        // the borrow ends with this statement.
        let owner = unsafe { &*info.cast::<Weak<T>>() }.upgrade();
        if let Some(owner) = owner {
            owner.fire();
        }
    }));
}

/// A timer in the common modes of the main run loop that calls its
/// owner. The owner usually holds the timer.
pub(crate) struct RunLoopTimer<T: Fire> {
    timer: CfRetained<CFRunLoopTimer>,
    runloop: MainRunLoop,
    owner: NonNull<Weak<T>>,
}

impl<T: Fire> RunLoopTimer<T> {
    /// A new timer that first fires after `delay` seconds and then every
    /// `interval` seconds, or once when `interval` is zero.
    pub(crate) fn new(
        runloop: MainRunLoop,
        delay: f64,
        interval: f64,
        owner: Weak<T>,
    ) -> Option<Self> {
        let owner = NonNull::from(Box::leak(Box::new(owner)));
        let mut context = CFRunLoopTimerContext {
            version: 0,
            info: owner.as_ptr().cast_const().cast(),
            retain: None,
            release: None,
            copy_description: None,
        };
        // SAFETY: Core Foundation copies the context. The timer keeps the
        // owner alive through `info` until `Drop` releases the timer.
        let created = unsafe {
            CFRunLoopTimerCreate(
                std::ptr::null(),
                CFAbsoluteTimeGetCurrent() + delay,
                interval,
                0,
                0,
                fire::<T>,
                &mut context,
            )
        };
        let Some(timer) = created else {
            // SAFETY: `owner` came from `Box::leak` above and no timer uses
            // it.
            drop(unsafe { Box::from_raw(owner.as_ptr()) });
            return None;
        };
        CFRunLoopAddTimer(runloop.0, &timer, kCFRunLoopCommonModes);
        Some(Self {
            timer,
            runloop,
            owner,
        })
    }

    /// Stops the timer from firing again.
    pub(crate) fn invalidate(&self) {
        CFRunLoopTimerInvalidate(&self.timer);
    }
}

impl<T: Fire> Drop for RunLoopTimer<T> {
    fn drop(&mut self) {
        CFRunLoopTimerInvalidate(&self.timer);
        CFRunLoopRemoveTimer(self.runloop.0, &self.timer, kCFRunLoopCommonModes);
        // SAFETY: `owner` came from `Box::leak` in `new`. The invalidated
        // timer never calls back again, and a running callback no longer
        // borrows the box.
        drop(unsafe { Box::from_raw(self.owner.as_ptr()) });
    }
}
