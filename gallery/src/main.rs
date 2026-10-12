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
mod app;
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
mod pages;
#[cfg(any(target_os = "macos", test))]
mod settings;
#[cfg(any(target_os = "macos", test))]
mod strings;
#[cfg(any(target_os = "macos", test))]
mod ui;
#[cfg(any(target_os = "macos", test))]
mod view;

#[cfg(target_os = "macos")]
fn main() {
    // Process-entry timestamp for the startup benchmark.
    let entered = std::time::Instant::now();
    use std::time::Duration;
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
    if std::env::var_os("CHOSCORUI_GALLERY_VIEWS").is_some() {
        let autoclose = std::env::var("CHOSCORUI_GALLERY_AUTOCLOSE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis);
        if let Err(error) = ui::app::run(ui::app::Launch { page, autoclose }) {
            eprintln!("choscorui-gallery: {error}");
            std::process::exit(1);
        }
        return;
    }
    let app =
        choscorui::platform::Application::new().expect("AppKit must start on the main thread");
    let mut gallery = app::Gallery::new(&app, app::Launch { page });
    if std::env::var_os("CHOSCORUI_GALLERY_AUTOPILOT").is_some() {
        let failures = autopilot::run(&app, &mut gallery);
        std::process::exit(i32::from(failures > 0));
    }
    for note in &gallery.notes {
        eprintln!("choscorui-gallery: {note}");
    }

    // `choscorui-bench` launches the gallery with this set and times the
    // first presented frame.
    if std::env::var_os("CHOSCORUI_GALLERY_REPORT_FIRST_FRAME").is_some() {
        loop {
            if gallery.windows.is_empty() {
                eprintln!("choscorui-gallery: no window opened, so no first frame to report");
                std::process::exit(1);
            }
            gallery.pump(&app, Duration::from_millis(1));
            if let Some(paint) = gallery.windows.first().and_then(|w| w.window.last_paint()) {
                println!("first_frame_us={}", (paint.finished - entered).as_micros());
                return;
            }
        }
    }

    let close_requested = std::rc::Rc::new(std::cell::Cell::new(false));
    let _auto_close = std::env::var("CHOSCORUI_GALLERY_AUTOCLOSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| {
            let flag = std::rc::Rc::clone(&close_requested);
            app.schedule_once(Duration::from_millis(milliseconds), move || flag.set(true))
                .expect("gallery timer must be scheduled")
        });
    while gallery.window_count() > 0 && !gallery.quit_requested() && !close_requested.get() {
        gallery.pump(&app, Duration::from_millis(100));
    }
    if let Err(error) = gallery.save() {
        eprintln!("choscorui-gallery: settings not saved: {error}");
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("choscorui-gallery currently runs on macOS");
}
