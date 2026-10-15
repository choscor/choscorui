//! The C library calls of the event loop and the shared memory buffers,
//! with safe wrappers: `poll`, `eventfd`, `timerfd`, and `memfd` mappings.

use super::ctypes::{CChar, CInt, CLong, CUint, c_void};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::time::Duration;

super::macros::ffi_struct! {
    /// `struct pollfd`.
    pub(crate) struct PollFd = "struct pollfd" {
        pub(crate) fd: CInt,
        pub(crate) events: i16,
        pub(crate) revents: i16,
    }

    /// `struct timespec`.
    pub(crate) struct Timespec = "struct timespec" {
        pub(crate) tv_sec: CLong,
        pub(crate) tv_nsec: CLong,
    }

    /// `struct itimerspec`.
    pub(crate) struct Itimerspec = "struct itimerspec" {
        pub(crate) it_interval: Timespec,
        pub(crate) it_value: Timespec,
    }
}

super::macros::c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    pub(crate) const POLLIN: i16 = 0x001;
    pub(crate) const POLLOUT: i16 = 0x004;
    pub(crate) const POLLERR: i16 = 0x008;
    pub(crate) const POLLHUP: i16 = 0x010;
    pub(crate) const POLLNVAL: i16 = 0x020;
    const EFD_CLOEXEC: CInt = 0o2000000;
    const EFD_NONBLOCK: CInt = 0o4000;
    const TFD_CLOEXEC: CInt = 0o2000000;
    const TFD_NONBLOCK: CInt = 0o4000;
    const CLOCK_MONOTONIC: CInt = 1;
    const MFD_CLOEXEC: CUint = 1;
    const MFD_ALLOW_SEALING: CUint = 2;
    const PROT_READ: CInt = 1;
    const PROT_WRITE: CInt = 2;
    const MAP_SHARED: CInt = 1;
    const IPC_PRIVATE: CInt = 0;
    const IPC_CREAT: CInt = 0o1000;
    const IPC_RMID: CInt = 0;
    const EINTR: CInt = 4;
    const EAGAIN: CInt = 11;
}

super::macros::libc_functions! {
    headers: ["stdlib.h", "poll.h", "unistd.h", "sys/eventfd.h", "sys/timerfd.h", "sys/mman.h", "sys/shm.h", "errno.h", "time.h"];
    fn free(pointer: *mut c_void);
    fn poll(fds: *mut PollFd, count: u64, timeout: CInt) -> CInt;
    fn read(fd: CInt, buffer: *mut c_void, count: usize) -> isize;
    fn write(fd: CInt, buffer: *const c_void, count: usize) -> isize;
    fn eventfd(initial: CUint, flags: CInt) -> CInt;
    fn timerfd_create(clock: CInt, flags: CInt) -> CInt;
    fn timerfd_settime(fd: CInt, flags: CInt, new: *const Itimerspec, old: *mut Itimerspec) -> CInt;
    fn memfd_create(name: *const CChar, flags: CUint) -> CInt;
    fn ftruncate(fd: CInt, length: i64) -> CInt;
    fn mmap(address: *mut c_void, length: usize, protection: CInt, flags: CInt, fd: CInt, offset: i64) -> *mut c_void;
    fn munmap(address: *mut c_void, length: usize) -> CInt;
    fn shmget(key: CInt, size: usize, flags: CInt) -> CInt;
    fn shmat(id: CInt, address: *const c_void, flags: CInt) -> *mut c_void;
    fn shmdt(address: *const c_void) -> CInt;
    fn shmctl(id: CInt, command: CInt, buffer: *mut ShmidDs) -> CInt;
}

super::macros::opaque_types! {
    /// `struct shmid_ds`, which the crate never reads.
    pub(crate) ShmidDs = "struct shmid_ds";
}

/// Frees memory that a C library allocated with `malloc`, such as an XCB
/// reply or event.
///
/// # Safety
///
/// `pointer` is null or came from `malloc`, and nothing uses it after the
/// call.
pub(crate) unsafe fn free_malloc(pointer: *mut c_void) {
    // SAFETY: the caller guarantees a `malloc` pointer or null.
    unsafe { free(pointer) }
}

/// The result of a C call that returns -1 and sets `errno` on failure.
fn check(result: CInt) -> io::Result<CInt> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

/// An owned descriptor from a C call that returns one.
fn owned(result: CInt) -> io::Result<OwnedFd> {
    let fd = check(result)?;
    // SAFETY: the call returned a new descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// One descriptor that `wait` watches, and what happened to it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Watch<'a> {
    pub(crate) fd: BorrowedFd<'a>,
    pub(crate) read: bool,
    pub(crate) write: bool,
}

/// What `wait` saw on a descriptor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Ready {
    pub(crate) read: bool,
    pub(crate) write: bool,
    /// The peer hung up, or the descriptor has an error.
    pub(crate) hangup: bool,
}

/// Waits up to `timeout` (forever for `None`) until one of `watches` is
/// ready. A signal that interrupts the wait counts as a timeout.
pub(crate) fn wait(watches: &[Watch<'_>], timeout: Option<Duration>) -> io::Result<Vec<Ready>> {
    let mut fds: Vec<PollFd> = watches
        .iter()
        .map(|watch| PollFd {
            fd: watch.fd.as_raw_fd(),
            events: (if watch.read { POLLIN } else { 0 }) | (if watch.write { POLLOUT } else { 0 }),
            revents: 0,
        })
        .collect();
    let timeout = timeout.map_or(-1, |timeout| {
        // Round up, so a short wait does not become a busy loop.
        let millis = timeout.as_micros().div_ceil(1000);
        CInt::try_from(millis).unwrap_or(CInt::MAX)
    });
    // SAFETY: `fds` holds `fds.len()` initialized entries for the call.
    let result = unsafe { poll(fds.as_mut_ptr(), fds.len() as u64, timeout) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(EINTR) {
            return Ok(vec![Ready::default(); watches.len()]);
        }
        return Err(error);
    }
    Ok(fds
        .iter()
        .map(|fd| Ready {
            read: fd.revents & POLLIN != 0,
            write: fd.revents & POLLOUT != 0,
            hangup: fd.revents & (POLLHUP | POLLERR | POLLNVAL) != 0,
        })
        .collect())
}

/// Reads up to `buffer.len()` bytes. `Ok(0)` is the end of the stream;
/// `WouldBlock` means no data now.
pub(crate) fn read_fd(fd: BorrowedFd<'_>, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        // SAFETY: `buffer` is valid for writes of its length.
        let result = unsafe { read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if result >= 0 {
            return Ok(result.unsigned_abs());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(EINTR) {
            return Err(error);
        }
    }
}

/// Writes up to `data.len()` bytes and returns how many it wrote.
pub(crate) fn write_fd(fd: BorrowedFd<'_>, data: &[u8]) -> io::Result<usize> {
    loop {
        // SAFETY: `data` is valid for reads of its length.
        let result = unsafe { write(fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
        if result >= 0 {
            return Ok(result.unsigned_abs());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(EINTR) {
            return Err(error);
        }
    }
}

/// True when `error` means that a non-blocking descriptor has no data.
pub(crate) fn would_block(error: &io::Error) -> bool {
    error.raw_os_error() == Some(EAGAIN)
}

/// A non-blocking `eventfd` that wakes the event loop from other threads.
#[derive(Debug)]
pub(crate) struct EventFd {
    fd: OwnedFd,
}

impl EventFd {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: plain call with flag arguments.
        let fd = owned(unsafe { eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK) })?;
        Ok(Self { fd })
    }

    /// Makes the descriptor readable. Signals add up until `clear`.
    pub(crate) fn signal(&self) -> io::Result<()> {
        write_fd(self.fd.as_fd(), &1u64.to_ne_bytes()).map(drop)
    }

    /// Clears the pending signals.
    pub(crate) fn clear(&self) {
        let mut value = [0u8; 8];
        let _ = read_fd(self.fd.as_fd(), &mut value);
    }
}

impl AsFd for EventFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// A non-blocking monotonic `timerfd`.
#[derive(Debug)]
pub(crate) struct TimerFd {
    fd: OwnedFd,
}

fn timespec(duration: Duration) -> Timespec {
    Timespec {
        tv_sec: CLong::try_from(duration.as_secs()).unwrap_or(CLong::MAX),
        tv_nsec: CLong::from(duration.subsec_nanos()),
    }
}

impl TimerFd {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: plain call with flag arguments.
        let fd = owned(unsafe { timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK) })?;
        Ok(Self { fd })
    }

    /// Makes the descriptor readable after `delay`, or disarms it for
    /// `None`. A zero delay fires at once.
    pub(crate) fn arm(&self, delay: Option<Duration>) -> io::Result<()> {
        let value = match delay {
            // A zero `it_value` disarms the timer, so the shortest delay
            // is one nanosecond.
            Some(delay) => timespec(delay.max(Duration::from_nanos(1))),
            None => timespec(Duration::ZERO),
        };
        let spec = Itimerspec {
            it_interval: timespec(Duration::ZERO),
            it_value: value,
        };
        // SAFETY: `spec` is valid for the call; the old value is not read.
        check(unsafe {
            timerfd_settime(
                self.fd.as_raw_fd(),
                0,
                &raw const spec,
                std::ptr::null_mut(),
            )
        })
        .map(drop)
    }

    /// Clears the expirations.
    pub(crate) fn clear(&self) {
        let mut value = [0u8; 8];
        let _ = read_fd(self.fd.as_fd(), &mut value);
    }
}

impl AsFd for TimerFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// A shared memory file mapped into the process: the pixels of a
/// `wl_shm` buffer, or a keymap from the compositor.
pub(crate) struct Mapping {
    address: *mut c_void,
    length: usize,
    writable: bool,
}

impl Mapping {
    /// Maps `length` bytes of `fd`.
    fn map(fd: BorrowedFd<'_>, length: usize, writable: bool) -> io::Result<Self> {
        if length == 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let protection = if writable {
            PROT_READ | PROT_WRITE
        } else {
            PROT_READ
        };
        // SAFETY: a new shared mapping of a descriptor that stays open
        // for the call; the kernel picks the address.
        let address = unsafe {
            mmap(
                std::ptr::null_mut(),
                length,
                protection,
                MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if address as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            address,
            length,
            writable,
        })
    }

    /// Maps `length` bytes of `fd` for reading, such as a keymap.
    pub(crate) fn read_only(fd: BorrowedFd<'_>, length: usize) -> io::Result<Self> {
        Self::map(fd, length, false)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        // SAFETY: the mapping is valid for `length` bytes until `drop`.
        // Another process may write to the memory; the bytes are plain
        // data, so a concurrent write only changes the values read.
        unsafe { std::slice::from_raw_parts(self.address.cast::<u8>(), self.length) }
    }

    /// The bytes for writing. `None` for a read-only mapping.
    pub(crate) fn bytes_mut(&mut self) -> Option<&mut [u8]> {
        if !self.writable {
            return None;
        }
        // SAFETY: the mapping is valid and writable for `length` bytes
        // until `drop`, and `&mut self` excludes other borrows.
        Some(unsafe { std::slice::from_raw_parts_mut(self.address.cast::<u8>(), self.length) })
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the address and length came from `mmap`, and no borrow
        // of the bytes outlives `self`.
        unsafe {
            munmap(self.address, self.length);
        }
    }
}

/// An anonymous shared memory file and its writable mapping.
pub(crate) struct SharedMemory {
    fd: OwnedFd,
    mapping: Mapping,
}

impl SharedMemory {
    /// A new file of `length` bytes, mapped for writing.
    pub(crate) fn new(name: &std::ffi::CStr, length: usize) -> io::Result<Self> {
        // SAFETY: `name` is NUL-terminated for the call.
        let fd =
            owned(unsafe { memfd_create(name.as_ptr().cast(), MFD_CLOEXEC | MFD_ALLOW_SEALING) })?;
        let size =
            i64::try_from(length).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: plain call on an owned descriptor.
        check(unsafe { ftruncate(fd.as_raw_fd(), size) })?;
        let mapping = Mapping::map(fd.as_fd(), length, true)?;
        Ok(Self { fd, mapping })
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        self.mapping
            .bytes_mut()
            .expect("a shared memory file is mapped for writing")
    }
}

impl AsFd for SharedMemory {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// A System V shared memory segment for MIT-SHM images. It is marked for
/// removal as soon as it is attached, so it never outlives the process.
pub(crate) struct SysvSegment {
    id: CInt,
    address: *mut c_void,
    length: usize,
}

impl SysvSegment {
    pub(crate) fn new(length: usize) -> io::Result<Self> {
        // SAFETY: plain call that creates a private segment.
        let id = check(unsafe { shmget(IPC_PRIVATE, length, IPC_CREAT | 0o600) })?;
        // SAFETY: attaches the new segment at an address the kernel picks.
        let address = unsafe { shmat(id, std::ptr::null(), 0) };
        let attached = address as isize != -1;
        let error = io::Error::last_os_error();
        // SAFETY: marks the segment for removal after the last detach; the
        // buffer argument is unused for IPC_RMID.
        unsafe {
            shmctl(id, IPC_RMID, std::ptr::null_mut());
        }
        if !attached {
            return Err(error);
        }
        Ok(Self {
            id,
            address,
            length,
        })
    }

    /// The segment id that the X server attaches.
    pub(crate) fn id(&self) -> u32 {
        self.id.cast_unsigned()
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: the segment is attached for `length` bytes until drop,
        // and `&mut self` excludes other borrows.
        unsafe { std::slice::from_raw_parts_mut(self.address.cast::<u8>(), self.length) }
    }
}

impl Drop for SysvSegment {
    fn drop(&mut self) {
        // SAFETY: the address came from `shmat`, and no borrow outlives
        // `self`.
        unsafe {
            shmdt(self.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_fd_wakes_a_wait() {
        let event = EventFd::new().unwrap();
        let watches = [Watch {
            fd: event.as_fd(),
            read: true,
            write: false,
        }];
        let ready = wait(&watches, Some(Duration::ZERO)).unwrap();
        assert!(!ready[0].read);
        event.signal().unwrap();
        let ready = wait(&watches, Some(Duration::from_secs(1))).unwrap();
        assert!(ready[0].read);
        event.clear();
        let ready = wait(&watches, Some(Duration::ZERO)).unwrap();
        assert!(!ready[0].read);
    }

    #[test]
    fn a_timer_fd_fires_after_its_delay() {
        let timer = TimerFd::new().unwrap();
        timer.arm(Some(Duration::from_millis(5))).unwrap();
        let watches = [Watch {
            fd: timer.as_fd(),
            read: true,
            write: false,
        }];
        let ready = wait(&watches, Some(Duration::from_secs(2))).unwrap();
        assert!(ready[0].read);
        timer.clear();
        timer.arm(None).unwrap();
        let ready = wait(&watches, Some(Duration::from_millis(20))).unwrap();
        assert!(!ready[0].read);
    }

    #[test]
    fn shared_memory_is_writable() {
        let mut memory = SharedMemory::new(c"choscorui-test", 4096).unwrap();
        memory.bytes_mut()[10] = 7;
        assert_eq!(memory.bytes_mut().len(), 4096);
        // A second mapping of the same file sees the write.
        let mapping = Mapping::read_only(memory.as_fd(), 4096).unwrap();
        assert_eq!(mapping.bytes()[10], 7);
    }
}
