//! The bench on `choscorui-macos`: AppKit windows, Core Text, and the
//! process usage of `proc_pid_rusage`.

use crate::sys::{Usage, command};
use choscorui_core::Rect;
use choscorui_macos::process_usage;
use std::time::{Duration, Instant};

pub use choscorui_macos::{
    Application, CoreTextShaper as Shaper, CountingAllocator, PaintTiming, Window, WindowEvent,
    allocation_count,
};

/// True when windows report when a frame reached the display.
pub const DISPLAY_TIMING: bool = true;

pub fn application() -> Application {
    Application::new().expect("AppKit must initialize on the main thread")
}

/// Processes at most one run-loop source or timer, for up to `timeout`.
pub fn pump(app: &Application, timeout: Duration) {
    app.pump_once(timeout);
}

/// Waits up to `timeout` for one native event and dispatches it.
pub fn wait(app: &Application, timeout: Duration) {
    app.dispatch_next_event(timeout);
}

pub fn activate(app: &Application) {
    app.activate();
}

pub fn show(window: &mut Window) {
    window.show();
}

pub fn close(window: &mut Window) {
    window.close();
}

/// The visible frame of each screen, the main screen first.
pub fn screens(_app: &Application) -> Vec<Rect> {
    choscorui_macos::screen_visible_frames().expect("screen frames")
}

pub fn display_count(window: &Window) -> usize {
    window.display_count()
}

pub fn last_display(window: &Window) -> Option<Instant> {
    window.last_display()
}

/// The resource usage of this process, or zeros when the kernel does not
/// report it. The wakeups are the wakeups from idle and from interrupts.
pub fn usage() -> Usage {
    process_usage().map_or_else(
        |_| Usage::default(),
        |usage| Usage {
            cpu: usage.cpu,
            peak_rss: usage.peak_rss,
            resident: usage.resident,
            footprint: usage.footprint,
            wakeups: usage.idle_wakeups + usage.interrupt_wakeups,
        },
    )
}

/// The machine and OS lines of the manifest.
pub fn machine() -> Vec<(String, String)> {
    let memory = command("sysctl", &["-n", "hw.memsize"])
        .parse::<u64>()
        .map(|bytes| format!("{} GiB", bytes / (1 << 30)))
        .unwrap_or_else(|_| "unknown".into());
    let power = command("pmset", &["-g", "batt"])
        .lines()
        .next()
        .and_then(|line| line.split('\'').nth(1))
        .unwrap_or("unknown")
        .to_owned();
    vec![
        (
            "machine.model".into(),
            command("sysctl", &["-n", "hw.model"]),
        ),
        (
            "machine.cpu".into(),
            command("sysctl", &["-n", "machdep.cpu.brand_string"]),
        ),
        ("machine.memory".into(), memory),
        ("machine.arch".into(), std::env::consts::ARCH.into()),
        ("machine.power".into(), power),
        (
            "os.version".into(),
            command("sw_vers", &["-productVersion"]),
        ),
        ("os.build".into(), command("sw_vers", &["-buildVersion"])),
    ]
}
