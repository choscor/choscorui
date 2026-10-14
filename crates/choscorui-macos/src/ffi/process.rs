//! Process resource usage from libSystem: `getrusage` for the CPU time
//! and the peak resident size, and `proc_pid_rusage` for the resident
//! size, the physical footprint, and the wakeups.

use super::sdk::{c_functions, ffi_struct, sdk_const};
use std::ffi::c_int;

ffi_struct! {
    /// `struct timeval`.
    #[derive(Clone, Copy, Default)]
    pub(crate) struct Timeval = timeval {
        pub(crate) seconds as tv_sec: i64,
        pub(crate) microseconds as tv_usec: i32,
    }

    /// `struct rusage`. The fields after `ru_maxrss` are not used.
    #[derive(Default)]
    pub(crate) struct Rusage = rusage {
        pub(crate) user as ru_utime: Timeval,
        pub(crate) system as ru_stime: Timeval,
        /// Bytes on macOS.
        pub(crate) max_rss as ru_maxrss: i64,
        rest as ru_ixrss: [i64; 13],
    }

    /// `struct rusage_info_v2`. The fields that the crate does not read
    /// are in groups.
    #[derive(Default)]
    pub(crate) struct RusageInfoV2 = rusage_info_v2 {
        head as ri_uuid: [u8; 16],
        times as ri_user_time: [u64; 2],
        pub(crate) idle_wakeups as ri_pkg_idle_wkups: u64,
        pub(crate) interrupt_wakeups as ri_interrupt_wkups: u64,
        pageins as ri_pageins: u64,
        wired as ri_wired_size: u64,
        pub(crate) resident as ri_resident_size: u64,
        pub(crate) footprint as ri_phys_footprint: u64,
        tail as ri_proc_start_abstime: [u64; 10],
    }
}

sdk_const! {
    pub(crate) RUSAGE_SELF: c_int = 0, sdk = RUSAGE_SELF;
    pub(crate) RUSAGE_INFO_V2: c_int = 2, sdk = RUSAGE_INFO_V2;
}

c_functions! {
    functions {
        pub(crate) safe fn getrusage(who: c_int, usage: &mut Rusage) -> c_int;
        safe fn getpid() -> c_int;
        unsafe fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: &mut RusageInfoV2) -> c_int;
    }
}

/// Fills `info` with the `RUSAGE_INFO_V2` usage of this process. Returns
/// false when the call fails.
pub(crate) fn process_rusage(info: &mut RusageInfoV2) -> bool {
    // SAFETY: The `RUSAGE_INFO_V2` flavor writes one `rusage_info_v2`, and
    // `info` is one of the same layout, which the SDK header check checks.
    let status = unsafe { proc_pid_rusage(getpid(), RUSAGE_INFO_V2, info) };
    status == 0
}
