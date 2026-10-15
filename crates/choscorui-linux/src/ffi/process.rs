//! The resource usage call of the C library, and the counting global
//! allocator, for benchmarks and resource checks.

use super::ctypes::{CInt, CLong};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

super::macros::ffi_struct! {
    /// `struct timeval`.
    pub(crate) struct Timeval = "struct timeval" {
        pub(crate) tv_sec: CLong,
        pub(crate) tv_usec: CLong,
    }

    /// `struct rusage`.
    pub(crate) struct Rusage = "struct rusage" {
        pub(crate) ru_utime: Timeval,
        pub(crate) ru_stime: Timeval,
        pub(crate) ru_maxrss: CLong,
        pub(crate) ru_ixrss: CLong,
        pub(crate) ru_idrss: CLong,
        pub(crate) ru_isrss: CLong,
        pub(crate) ru_minflt: CLong,
        pub(crate) ru_majflt: CLong,
        pub(crate) ru_nswap: CLong,
        pub(crate) ru_inblock: CLong,
        pub(crate) ru_oublock: CLong,
        pub(crate) ru_msgsnd: CLong,
        pub(crate) ru_msgrcv: CLong,
        pub(crate) ru_nsignals: CLong,
        pub(crate) ru_nvcsw: CLong,
        pub(crate) ru_nivcsw: CLong,
    }
}

super::macros::c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    const RUSAGE_SELF: CInt = 0;
}

super::macros::libc_functions! {
    headers: ["sys/time.h", "sys/resource.h"];
    fn getrusage(who: CInt, usage: *mut Rusage) -> CInt;
}

/// The resource usage of all threads of this process, or `None` when the
/// call fails.
pub(crate) fn self_usage() -> Option<Rusage> {
    let zero = Timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut usage = Rusage {
        ru_utime: zero,
        ru_stime: zero,
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    };
    // SAFETY: `usage` is a valid, writable `struct rusage` for the call.
    let result = unsafe { getrusage(RUSAGE_SELF, &mut usage) };
    (result == 0).then_some(usage)
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// The system allocator with a count of each allocation and each
/// reallocation. A binary makes it its global allocator with
/// `#[global_allocator]`, and [`allocation_count`] reads the count.
///
/// # Examples
///
/// ```
/// use choscorui_linux::{CountingAllocator, allocation_count};
///
/// #[global_allocator]
/// static ALLOCATOR: CountingAllocator = CountingAllocator;
///
/// let before = allocation_count();
/// let text = String::from("allocated");
/// assert!(allocation_count() > before);
/// drop(text);
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct CountingAllocator;

// SAFETY: Each method forwards to `System` with the arguments of the
// caller, so the allocator keeps the contract of `System`.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The caller keeps the contract of `GlobalAlloc::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller keeps the contract of `GlobalAlloc::dealloc`,
        // and `System` made `ptr`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The caller keeps the contract of
        // `GlobalAlloc::alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The caller keeps the contract of `GlobalAlloc::realloc`,
        // and `System` made `ptr`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// The number of allocations and reallocations that
/// [`CountingAllocator`] has made since the process started. It is 0
/// when the process uses another global allocator.
pub fn allocation_count() -> u64 {
    ALLOCATIONS.load(Ordering::Relaxed)
}
