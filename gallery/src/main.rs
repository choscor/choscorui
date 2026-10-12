//! choscorui gallery: every public control in one window.
//!
//! It depends on the `choscorui` facade alone and forbids `unsafe`.
//!
//! Usage: `choscorui-gallery [--list | <page-id>]`. With no page id the
//! last page is restored.
//!
//! Environment:
//! - `CHOSCORUI_GALLERY_AUTOPILOT=1` runs the scripted gate checks, prints
//!   one line per check, and exits non-zero on failure.
//! - `CHOSCORUI_GALLERY_SETTINGS=<path>` stores settings there instead of
//!   the per-user location.
//! - `CHOSCORUI_GALLERY_LANG=<en|vi>` overrides the restored language.
//! - `CHOSCORUI_GALLERY_AUTOCLOSE_MS=<n>` quits (saving settings) after n
//!   milliseconds.
//! - `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME=1` prints `first_frame_us=<n>`
//!   after the first presented frame and exits.

#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod autopilot;
// Only the AppKit build shows the pages. Tests still run them headlessly on
// every OS.
#[cfg(any(target_os = "macos", test))]
mod catalog;
#[cfg(any(target_os = "macos", test))]
mod cli;
#[cfg(test)]
mod coverage;
#[cfg(any(target_os = "macos", test))]
mod page;
#[cfg(any(target_os = "macos", test))]
mod settings;
#[cfg(any(target_os = "macos", test))]
mod strings;
#[cfg(any(target_os = "macos", test))]
mod ui;

#[cfg(target_os = "macos")]
fn main() {
    // Process-entry timestamp for the startup benchmark.
    let entered = std::time::Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let page = match cli::parse(&args) {
        cli::Command::Run(page) => page,
        cli::Command::List => {
            print!("{}", cli::list());
            return;
        }
        cli::Command::Fail(message) => {
            eprintln!("choscorui-gallery: {message}");
            std::process::exit(2);
        }
    };
    let (gallery, mut cx) = match ui::app::start(&ui::app::Launch { page }) {
        Ok(started) => started,
        Err(error) => {
            eprintln!("choscorui-gallery: {error}");
            std::process::exit(1);
        }
    };
    if std::env::var_os("CHOSCORUI_GALLERY_AUTOPILOT").is_some() {
        let failures = autopilot::run(&gallery, &mut cx);
        std::process::exit(i32::from(failures > 0));
    }
    for note in &gallery.notes {
        eprintln!("choscorui-gallery: {note}");
    }
    // `choscorui-bench` launches the gallery with this set and times the
    // first presented frame.
    if std::env::var_os("CHOSCORUI_GALLERY_REPORT_FIRST_FRAME").is_some() {
        match ui::app::first_frame(&gallery, &mut cx, entered) {
            Some(elapsed) => println!("first_frame_us={}", elapsed.as_micros()),
            None => {
                eprintln!("choscorui-gallery: no window opened, so no first frame to report");
                std::process::exit(1);
            }
        }
        return;
    }
    let autoclose = std::env::var("CHOSCORUI_GALLERY_AUTOCLOSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(std::time::Duration::from_millis);
    ui::app::run(&gallery, &mut cx, autoclose);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("choscorui-gallery currently runs on macOS");
}
