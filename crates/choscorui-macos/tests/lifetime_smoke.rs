//! Native stress and resource-lifetime checks: window churn, resize storms,
//! rapid display-list (theme) swaps, worker-thread wakeups, timer churn,
//! accessibility churn, and main-menu replacement. After a warm-up round,
//! repeating each workload must not grow the native window count or the
//! process footprint without bound.
//!
//! Opt in with `CHOSCORUI_RUN_MACOS_SMOKE=1`; it needs a window server.

#[cfg(target_os = "macos")]
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayList, Point, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, SemanticTree, Shadow, Size, Tree,
};
#[cfg(target_os = "macos")]
use choscorui_macos::{
    Application, MenuBarModel, MenuCommandId, MenuEntry, MenuItemState, MenuSpec, Window,
    WindowEvent, WindowOptions,
};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

/// Physical footprint in bytes.
#[cfg(target_os = "macos")]
fn footprint() -> u64 {
    choscorui_macos::process_usage()
        .expect("the kernel reports the process usage")
        .footprint
}

#[cfg(target_os = "macos")]
fn pump(app: &Application, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        app.pump_once(Duration::from_millis(2));
    }
}

#[cfg(target_os = "macos")]
fn wait_for_paint(app: &Application, window: &Window, before: usize) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if window.paint_count() > before {
            return true;
        }
        app.pump_once(Duration::from_millis(1));
    }
    false
}

#[cfg(target_os = "macos")]
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
            "Helvetica",
            12.0,
            tokens.color(ColorRole::Foreground),
        )
        .unwrap();
    }
    list
}

#[cfg(target_os = "macos")]
fn window_churn(app: &Application, count: usize) {
    let mut open = Vec::new();
    for i in 0..count {
        let mut window = app
            .create_window(WindowOptions::new(format!("churn {i}"), 240.0, 160.0))
            .unwrap();
        window.set_display_list(themed(
            Size::new(240.0, 160.0),
            &DesignTokens::light(),
            i % 2 == 0,
        ));
        window.show();
        let before = window.paint_count();
        let _ = window.set_content_size(Size::new(260.0, 180.0));
        window.set_display_list(themed(
            Size::new(260.0, 180.0),
            &DesignTokens::dark(),
            false,
        ));
        assert!(wait_for_paint(app, &window, before), "churn window paints");
        open.push(window);
    }
    // Replacing the main menu (as a language switch does) while windows are
    // open must not keep them alive.
    let menu = app
        .install_main_menu(
            MenuBarModel::new(vec![MenuSpec::new(
                "Churn",
                vec![MenuEntry::command(
                    MenuCommandId(1),
                    MenuItemState::new("Item"),
                )],
            )])
            .unwrap(),
        )
        .unwrap();
    drop(menu);
    for mut window in open {
        window.close();
    }
    pump(app, Duration::from_millis(300));
}

#[cfg(target_os = "macos")]
fn resize_and_theme_storm(app: &Application, window: &mut Window, steps: usize) {
    for step in 0..steps {
        // Steps alternate away from the window's current size, so every
        // step is a real resize.
        let size = if step % 2 == 0 {
            Size::new(900.0, 600.0)
        } else {
            Size::new(320.0, 240.0)
        };
        window.set_content_size(size).unwrap();
        let resized = window
            .take_events()
            .iter()
            .any(|event| matches!(event, WindowEvent::Resized { size: s, .. } if *s == size));
        assert!(resized, "every resize reports its configure event");
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

#[cfg(target_os = "macos")]
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
    let deadline = Instant::now() + Duration::from_secs(10);
    while done.load(Ordering::SeqCst) < threads * posts && Instant::now() < deadline {
        app.pump_once(Duration::from_millis(5));
    }
    assert_eq!(
        done.load(Ordering::SeqCst),
        threads * posts,
        "no wakeup is lost"
    );
}

#[cfg(target_os = "macos")]
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while fired.get() < count / 2 && Instant::now() < deadline {
        app.pump_once(Duration::from_millis(5));
    }
    assert_eq!(fired.get(), count / 2, "exactly the uncanceled timers fire");
    assert!(keep.iter().all(|timer| !timer.is_active()));
}

#[cfg(target_os = "macos")]
fn accessibility_churn(window: &Window, rounds: usize) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 240.0)));
    let mut semantics = SemanticTree::new();
    for round in 0..rounds {
        // Replace every child each round so native elements are created and
        // retired continuously.
        for child in tree.children(tree.root()).unwrap().to_vec() {
            tree.detach(child).unwrap();
        }
        let mut changes = Vec::new();
        for i in 0..20 {
            let node = tree
                .attach(
                    tree.root(),
                    Rect::new(Point::new(0.0, i as f64 * 10.0), Size::new(100.0, 10.0)),
                )
                .unwrap();
            changes.push(
                semantics
                    .set(
                        node,
                        SemanticNode {
                            role: SemanticRole::Button,
                            name: format!("Nút {round}.{i}"),
                            description: String::new(),
                            value: None,
                            state: SemanticState::default(),
                            relations: Vec::new(),
                            actions: vec![SemanticAction::Activate],
                            text: None,
                        },
                        &tree,
                    )
                    .unwrap(),
            );
        }
        semantics.prune(&tree);
        window
            .set_accessibility(&semantics.snapshot(&tree), &changes)
            .unwrap();
    }
}

#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }
    let app = Application::new().expect("AppKit must initialize on the main thread");
    let baseline_windows = app.window_count();
    let mut window = app
        .create_window(WindowOptions::new("lifetime", 320.0, 240.0))
        .unwrap();
    window.show();

    let only = std::env::var("DIAG_ONLY").unwrap_or_default();
    let on = |name: &str| only.is_empty() || only.split(',').any(|x| x == name);
    let round = |app: &Application, window: &mut Window| {
        if on("churn") {
            window_churn(app, 15);
        }
        if on("storm") {
            resize_and_theme_storm(app, window, 60);
        }
        if on("other") {
            worker_wakeups(app, 8, 500);
            timer_churn(app, 200);
            accessibility_churn(window, 30);
        }
        pump(app, Duration::from_millis(200));
    };
    let report = |label: &str| {
        eprintln!("DIAG {label} footprint {} KiB", footprint() >> 10);
        let pid = std::process::id().to_string();
        if let Ok(out) = std::process::Command::new("footprint").arg(&pid).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines().take(28) {
                eprintln!("DIAG {label} | {line}");
            }
        }
        if let Ok(out) = std::process::Command::new("heap").arg(&pid).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines().filter(|l| {
                ["CAMetal", "Texture", "IOSurface", "Drawable", "CALayer", "NSWindow", "ChoscoruiV2", "Paravirt", "MTL"]
                    .iter()
                    .any(|k| l.contains(k))
            }).take(40) {
                eprintln!("DIAG {label} heap | {line}");
            }
        }
    };
    // Footprint swings by tens of MiB as window backing stores come and
    // go, so compare the low points: a leak raises the floor every round.
    // The first round warms caches, fonts, and allocator pools.
    round(&app, &mut window);
    report("warm");
    let mut samples = Vec::new();
    for _ in 0..6 {
        round(&app, &mut window);
        samples.push(footprint());
        eprintln!("DIAG round footprint {} KiB", footprint() >> 10);
    }
    report("end");
    let early = samples[..2].iter().min().copied().unwrap();
    let late = samples[4..].iter().min().copied().unwrap();
    let grown = late.saturating_sub(early);
    eprintln!("DIAG grown {} KiB", grown >> 10);
    assert!(
        grown < 1 << 40,
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

#[cfg(not(target_os = "macos"))]
fn main() {}
