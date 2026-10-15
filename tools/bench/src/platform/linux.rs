//! The bench on `choscorui-linux`: Wayland or X11 windows, the Linux
//! text system, and the process usage of `getrusage` and `/proc`.

use crate::sys::{Usage, command};
use choscorui_core::Rect;
use choscorui_linux::process_usage;
use std::time::{Duration, Instant};

pub use choscorui_linux::{
    Application, CountingAllocator, LinuxTextShaper as Shaper, PaintTiming, Window, WindowEvent,
    allocation_count,
};

/// True when windows report when a frame reached the display. The Linux
/// windows report only the end of the paint.
pub const DISPLAY_TIMING: bool = false;

pub fn application() -> Application {
    Application::new().expect("the display server must accept a connection")
}

/// Paints the ready windows and handles the events of up to `timeout`.
pub fn pump(app: &Application, timeout: Duration) {
    app.dispatch_next_event(timeout)
        .expect("the display connection must stay open");
}

/// Waits up to `timeout` for an event and handles it.
pub fn wait(app: &Application, timeout: Duration) {
    pump(app, timeout);
}

/// The display servers have no activation request for a client.
pub fn activate(_app: &Application) {}

pub fn show(window: &mut Window) {
    window.show();
}

pub fn close(window: &mut Window) {
    window.close();
}

/// The logical rectangle of each screen.
pub fn screens(app: &Application) -> Vec<Rect> {
    app.screens()
}

pub fn display_count(_window: &Window) -> usize {
    0
}

pub fn last_display(_window: &Window) -> Option<Instant> {
    None
}

/// The resource usage of this process, or zeros when the kernel does not
/// report it. The wakeups are the voluntary and involuntary context
/// switches of all threads.
pub fn usage() -> Usage {
    process_usage().map_or_else(
        |_| Usage::default(),
        |usage| Usage {
            cpu: usage.cpu,
            peak_rss: usage.peak_rss,
            resident: usage.resident,
            footprint: usage.footprint,
            wakeups: usage.voluntary_switches + usage.involuntary_switches,
        },
    )
}

/// The value of the first `key: value` line of `text`.
fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The machine and OS lines of the manifest, from `/proc/cpuinfo`,
/// `/proc/meminfo`, `/etc/os-release`, and `uname`.
pub fn machine() -> Vec<(String, String)> {
    let cpuinfo = read("/proc/cpuinfo");
    let cpus = cpuinfo
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count();
    // x86 names the CPU in `model name`; arm64 gives only the part codes.
    let model = value(&cpuinfo, "model name")
        .map(str::to_owned)
        .or_else(|| {
            let implementer = value(&cpuinfo, "CPU implementer")?;
            let part = value(&cpuinfo, "CPU part")?;
            Some(format!("implementer {implementer} part {part}"))
        })
        .unwrap_or_else(|| "unknown".into());
    let memory = value(&read("/proc/meminfo"), "MemTotal")
        .and_then(|total| total.strip_suffix("kB"))
        .and_then(|kib| kib.trim().parse::<u64>().ok())
        .map(|kib| format!("{} GiB", kib >> 20))
        .unwrap_or_else(|| "unknown".into());
    let os = read("/etc/os-release")
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_owned())
        .unwrap_or_else(|| "unknown".into());
    let session = match std::env::var("CHOSCORUI_BACKEND").ok().as_deref() {
        Some(backend @ ("wayland" | "x11")) => backend.to_owned(),
        _ if std::env::var_os("WAYLAND_DISPLAY").is_some() => "wayland".into(),
        _ => "x11".into(),
    };
    vec![
        ("machine.cpu".into(), format!("{model}, {cpus} CPUs")),
        ("machine.memory".into(), memory),
        ("machine.arch".into(), std::env::consts::ARCH.into()),
        ("os.version".into(), os),
        ("os.kernel".into(), command("uname", &["-sr"])),
        ("os.display_server".into(), session),
        (
            "run.painter".into(),
            std::env::var("CHOSCORUI_PAINTER").unwrap_or_else(|_| "default".into()),
        ),
    ]
}
