//! A global allocator that counts the Rust heap allocations of the
//! process and forwards each call to the system allocator unchanged.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// The system allocator with a count of each allocation and each
/// reallocation. A binary makes it its global allocator with
/// `#[global_allocator]`, and [`allocation_count`] reads the count.
///
/// # Examples
///
/// ```
/// use choscorui_macos::{CountingAllocator, allocation_count};
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
