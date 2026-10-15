//! Native stress and resource-lifetime checks: window churn, resize
//! storms, rapid display-list (theme) swaps, worker-thread wakeups, and
//! timer churn. After a warm-up round, repeating each workload must not
//! grow the native window count or the process footprint without bound.
//!
//! The test runs only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, on Wayland and on X11.
#![cfg(target_os = "linux")]

use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayList, Point, Rect, Shadow, Size, WindowOptions,
};
use choscorui_linux::{Application, Window, WindowEvent, process_usage};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

/// The memory that only this process holds, in bytes.
fn footprint() -> u64 {
    process_usage()
        .expect("the kernel reports the process usage")
        .footprint
}

fn pump(app: &Application, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        app.dispatch_next_event(Duration::from_millis(2)).unwrap();
    }
}

/// Dispatches events until `done` is true, for at most `limit`.
fn pump_until(app: &Application, limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        app.dispatch_next_event(Duration::from_millis(1)).unwrap();
    }
    done()
}

fn wait_for_paint(app: &Application, window: &Window, before: usize) -> bool {
    pump_until(app, Duration::from_secs(2), || {
        window.paint_count() > before
    })
}

/// Waits until the window reports the content size `size`. Wayland
/// reports it at once; X11 reports it after the server configures the
/// window.
fn wait_for_resize(app: &Application, window: &Window, size: Size) -> bool {
    let mut resized = false;
    pump_until(app, Duration::from_secs(2), || {
        resized |= window
            .take_events()
            .iter()
            .any(|event| matches!(event, WindowEvent::Resized { size: s, .. } if *s == size));
        resized
    })
}

fn themed(size: Size, tokens: &DesignTokens, light: bool) -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), size),
        tokens.color(ColorRole::Background),
    );
    for i in 0..40 {
        let rect = Rect::new(
            Point::new(10.0 + (i % 8) as f64 * 70.0, 10.0 + (i / 8) as f64 * 40.0),
            Size::new(60.0, 28.0),
        );
        if light {
            list.draw_shadow(
                rect,
                8.0,
                Shadow {
                    offset: Point::new(0.0, 2.0),
                    blur: 4.0,
                    color: Color::rgba8(0, 0, 0, 80),
                    spread: 0.0,
                    inset: false,
                },
            )
            .unwrap();
        }
        list.fill_rect(rect, tokens.color(ColorRole::Primary));
        list.draw_text(
            Point::new(rect.origin.x + 6.0, rect.origin.y + 18.0),
            &format!("Mục {i}"),
            "Sans",
            12.0,
            tokens.color(ColorRole::Foreground),
        )
        .unwrap();
    }
    list
}

/// Opens, paints, resizes, and closes `count` windows. After the closes,
/// the native window count is back at its value before the churn.
fn window_churn(app: &Application, count: usize) {
    let before = app.window_count();
    let mut open = Vec::new();
    for i in 0..count {
        let window = app
            .create_window(WindowOptions::new(format!("churn {i}"), 240.0, 160.0))
            .unwrap();
        window.set_display_list(themed(
            Size::new(240.0, 160.0),
            &DesignTokens::light(),
            i % 2 == 0,
        ));
        window.show();
        assert!(wait_for_paint(app, &window, 0), "churn window {i} paints");
        let painted = window.paint_count();
        let size = Size::new(260.0, 180.0);
        window.set_content_size(size).unwrap();
        assert!(
            wait_for_resize(app, &window, size),
            "churn window {i} resizes"
        );
        window.set_display_list(themed(size, &DesignTokens::dark(), false));
        assert!(
            wait_for_paint(app, &window, painted),
            "churn window {i} paints after the resize"
        );
        open.push(window);
    }
    assert_eq!(app.window_count(), before + count);
    for window in &open {
        window.close();
        assert!(window.is_closed());
    }
    drop(open);
    pump(app, Duration::from_millis(200));
    assert_eq!(
        app.window_count(),
        before,
        "closed windows leave no native window"
    );
}

fn resize_and_theme_storm(app: &Application, window: &Window, steps: usize) {
    for step in 0..steps {
        // Steps alternate away from the window's current size, so every
        // step is a real resize.
        let size = if step % 2 == 0 {
            Size::new(900.0, 600.0)
        } else {
            Size::new(320.0, 240.0)
        };
        window.set_content_size(size).unwrap();
        assert!(
            wait_for_resize(app, window, size),
            "storm step {step} reports its configure event"
        );
        let light = step % 3 == 0;
        let tokens = if light {
            DesignTokens::light()
        } else {
            DesignTokens::dark()
        };
        let before = window.paint_count();
        window.set_display_list(themed(size, &tokens, light));
        assert!(
            wait_for_paint(app, window, before),
            "storm step {step} paints"
        );
    }
}

fn worker_wakeups(app: &Application, threads: usize, posts: usize) {
    let done = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let poster = app.poster();
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                for _ in 0..posts {
                    let done = Arc::clone(&done);
                    poster
                        .post(move || {
                            done.fetch_add(1, Ordering::SeqCst);
                        })
                        .unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    pump_until(app, Duration::from_secs(10), || {
        done.load(Ordering::SeqCst) >= threads * posts
    });
    assert_eq!(
        done.load(Ordering::SeqCst),
        threads * posts,
        "no wakeup is lost"
    );
}

fn timer_churn(app: &Application, count: usize) {
    let fired = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut keep = Vec::new();
    for i in 0..count {
        let fired = std::rc::Rc::clone(&fired);
        let timer = app
            .schedule_once(Duration::from_millis(1), move || fired.set(fired.get() + 1))
            .unwrap();
        if i % 2 == 0 {
            timer.cancel();
        } else {
            keep.push(timer);
        }
    }
    pump_until(app, Duration::from_secs(5), || fired.get() >= count / 2);
    // Canceled timers must not fire late.
    pump(app, Duration::from_millis(50));
    assert_eq!(fired.get(), count / 2, "exactly the uncanceled timers fire");
    assert!(keep.iter().all(|timer| !timer.is_active()));
}

#[test]
fn churn_leaks_no_window_and_keeps_the_footprint_floor() {
    if !enabled() {
        return;
    }
    let app = Application::new().unwrap();
    let baseline_windows = app.window_count();
    let window = app
        .create_window(WindowOptions::new("lifetime", 320.0, 240.0))
        .unwrap();
    window.show();
    window.set_display_list(themed(
        Size::new(320.0, 240.0),
        &DesignTokens::light(),
        true,
    ));
    assert!(wait_for_paint(&app, &window, 0), "the main window paints");

    let round = |app: &Application, window: &Window| {
        window_churn(app, 15);
        resize_and_theme_storm(app, window, 60);
        worker_wakeups(app, 8, 500);
        timer_churn(app, 200);
        pump(app, Duration::from_millis(200));
    };
    // The footprint swings as window buffers come and go, so compare the
    // low points: a leak raises the floor every round. The first round
    // warms caches, fonts, and allocator pools.
    round(&app, &window);
    let mut samples = Vec::new();
    for _ in 0..6 {
        round(&app, &window);
        samples.push(footprint());
    }
    let early = samples[..2].iter().min().copied().unwrap();
    let late = samples[4..].iter().min().copied().unwrap();
    let grown = late.saturating_sub(early);
    assert!(
        grown < 16 << 20,
        "footprint floor rose {} MiB over four rounds: {:?} MiB",
        grown >> 20,
        samples.iter().map(|bytes| bytes >> 20).collect::<Vec<_>>()
    );
    assert_eq!(
        app.window_count(),
        baseline_windows + 1,
        "only the main window"
    );
    assert_eq!(app.posted_panic_count(), 0);
    assert_eq!(app.timer_panic_count(), 0);

    window.close();
    drop(window);
    pump(&app, Duration::from_millis(300));
    assert_eq!(
        app.window_count(),
        baseline_windows,
        "every window released"
    );
    println!(
        "lifetime smoke: OK (footprint floor change {} KiB)",
        grown >> 10
    );
}
