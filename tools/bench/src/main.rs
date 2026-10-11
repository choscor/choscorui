//! Benchmark driver for the reference scenarios in docs/BENCHMARKS.md.
//!
//! ```sh
//! cargo build --release -p choscorui-gallery -p choscorui-bench
//! target/release/choscorui-bench [--quick] [--only startup,scroll] \
//!     [--out results.txt] [--check bench/budgets.txt]
//! ```
//!
//! Startup launches the gallery and a stress-workload child process and
//! times their first presented frame. The other scenarios drive real
//! windows in this process. Results carry a machine manifest. With
//! `--check`, the process exits 1 when a release budget fails or a
//! budgeted metric is missing.

mod stats;

#[cfg(target_os = "macos")]
mod scenarios;
#[cfg(target_os = "macos")]
mod sys;
#[cfg(target_os = "macos")]
mod workspace;

/// Counts Rust heap allocations so frames can report allocation churn.
mod alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

    pub struct Counting;

    // SAFETY: Forwards to the system allocator unchanged.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            // SAFETY: Same contract as the caller's.
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: Same contract as the caller's.
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            // SAFETY: Same contract as the caller's.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    #[allow(dead_code)]
    pub fn count() -> u64 {
        ALLOCATIONS.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "macos")]
const SCENARIOS: [&str; 8] = [
    "startup", "idle", "resize", "scroll", "theme", "document", "windows", "size",
];

#[cfg(target_os = "macos")]
fn main() {
    use choscorui_core::Size;
    use choscorui_macos::{Application, WindowOptions};
    use stats::{BudgetOutcome, Results};
    use std::time::Instant;
    use workspace::Workspace;

    let entered = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    if value("--child").as_deref() == Some("startup-stress") {
        scenarios::startup_stress_child(entered);
        return;
    }
    let quick = args.iter().any(|arg| arg == "--quick");
    let only: Vec<String> = value("--only")
        .map(|list| list.split(',').map(str::to_owned).collect())
        .unwrap_or_else(|| SCENARIOS.iter().map(|s| s.to_string()).collect());
    if let Some(unknown) = only.iter().find(|name| !SCENARIOS.contains(&name.as_str())) {
        eprintln!("unknown scenario `{unknown}`; expected one of {SCENARIOS:?}");
        std::process::exit(2);
    }
    let budgets = value("--check").map(|path| {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read budgets {path}: {error}"));
        stats::parse_budgets(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
    });
    let config = if quick {
        scenarios::Config::quick()
    } else {
        scenarios::Config::full()
    };
    let runs = |name: &str| only.iter().any(|item| item == name);
    let mut results = Results::default();
    let executable = std::env::current_exe().expect("current executable path");
    let bin_dir = executable
        .parent()
        .expect("executable directory")
        .to_owned();

    if runs("startup") {
        startup(
            &bin_dir,
            &executable,
            if quick { 5 } else { 30 },
            &mut results,
        );
    }
    if runs("size") {
        for name in ["choscorui-gallery", "choscorui-bench"] {
            match std::fs::metadata(bin_dir.join(name)) {
                Ok(metadata) => {
                    results.metric(format!("size.{name}_mib"), sys::mib(metadata.len()))
                }
                Err(_) => results.notes.push(format!("size: {name} not built")),
            }
        }
    }

    let app = Application::new().expect("AppKit must initialize on the main thread");
    let size = Size::new(1200.0, 800.0);
    let mut window = app
        .create_window(WindowOptions::new(
            "choscorui bench",
            size.width,
            size.height,
        ))
        .expect("window creation must succeed");
    let mut workspace = Workspace::new(size);
    window.set_display_list(workspace.paint());
    window.show();
    app.activate();
    scenarios::present(&app, &window, 0);
    results.metric("display.scale", window.backing_scale_factor());

    let mut step = |name: &str, run: &mut dyn FnMut(&mut Results)| {
        if runs(name) {
            eprintln!("running {name}…");
            let started = Instant::now();
            run(&mut results);
            eprintln!("  {name} took {:.1}s", started.elapsed().as_secs_f64());
        }
    };
    step("idle", &mut |r| {
        scenarios::idle(&app, &mut window, &config, r)
    });
    step("resize", &mut |r| {
        scenarios::resize(&app, &mut window, &mut workspace, &config, r)
    });
    step("scroll", &mut |r| {
        scenarios::scroll(&app, &mut window, &mut workspace, &config, r)
    });
    step("theme", &mut |r| {
        scenarios::theme(&app, &mut window, &config, r)
    });
    step("document", &mut |r| {
        scenarios::document(&app, &mut window, &config, r)
    });
    window.close();
    drop(window);
    step("windows", &mut |r| scenarios::windows(&app, &config, r));

    let mut report = String::from("# choscorui benchmark results\n");
    for (key, value) in sys::manifest() {
        report.push_str(&format!("{key}={value}\n"));
    }
    report.push_str(&format!(
        "run.mode={}\n",
        if quick { "quick" } else { "full" }
    ));
    report.push_str("\n## Metrics\n");
    for (name, value) in &results.metrics {
        report.push_str(&format!("{name} {value:.3}\n"));
    }
    for note in &results.notes {
        report.push_str(&format!("note: {note}\n"));
    }
    let mut failures = 0;
    if let Some(budgets) = &budgets {
        report.push_str("\n## Budgets\n");
        for budget in budgets {
            let outcome = stats::check(budget, &results.metrics);
            let applies = runs(budget.metric.split('.').next().unwrap_or(""));
            let line = match outcome {
                BudgetOutcome::Pass(value) => format!("PASS {} = {value:.3}", budget.metric),
                BudgetOutcome::Fail(value) => {
                    failures += 1;
                    format!(
                        "FAIL {} = {value:.3} (limit {:?} {})",
                        budget.metric, budget.comparison, budget.limit
                    )
                }
                BudgetOutcome::Missing if applies => {
                    failures += 1;
                    format!("FAIL {} missing", budget.metric)
                }
                BudgetOutcome::Missing => format!("SKIP {} (scenario not run)", budget.metric),
            };
            report.push_str(&line);
            report.push('\n');
        }
    }
    let mut raw = String::from("\n## Samples\n");
    for (name, samples) in &results.samples {
        let values: Vec<String> = samples.iter().map(|value| format!("{value:.3}")).collect();
        raw.push_str(&format!("{name} {}\n", values.join(" ")));
    }
    print!("{report}");
    let out = value("--out")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let target = bin_dir.parent().unwrap_or(&bin_dir).join("bench");
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            target.join(format!("results-{stamp}.txt"))
        });
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&out, format!("{report}{raw}")) {
        Ok(()) => eprintln!("wrote {}", out.display()),
        Err(error) => eprintln!("cannot write {}: {error}", out.display()),
    }
    if failures > 0 {
        eprintln!("{failures} budget failure(s)");
        std::process::exit(1);
    }
}

/// Launches each startup target `runs + 1` times (the first is a discarded
/// warm-up) and records the child's first-frame time and the driver's
/// spawn-to-report time.
#[cfg(target_os = "macos")]
fn startup(
    bin_dir: &std::path::Path,
    executable: &std::path::Path,
    runs: usize,
    results: &mut stats::Results,
) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let gallery = bin_dir.join("choscorui-gallery");
    let mut targets = vec![(
        "startup.stress",
        Command::new(executable),
        vec!["--child", "startup-stress"],
    )];
    if gallery.exists() {
        let mut command = Command::new(&gallery);
        command.env("CHOSCORUI_GALLERY_REPORT_FIRST_FRAME", "1");
        targets.insert(0, ("startup.gallery", command, Vec::new()));
    } else {
        results
            .notes
            .push("startup.gallery: build choscorui-gallery with the same profile".into());
    }
    for (name, mut command, args) in targets {
        eprintln!("running {name}…");
        let mut first_frame = Vec::new();
        let mut launch = Vec::new();
        let mut failures = 0;
        for run in 0..=runs {
            let started = Instant::now();
            let child = command
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                failures += 1;
                continue;
            };
            let mut line = String::new();
            let reader = BufReader::new(child.stdout.take().unwrap());
            for candidate in reader.lines().map_while(Result::ok) {
                if candidate.starts_with("first_frame_us=") {
                    line = candidate;
                    break;
                }
            }
            let reported = started.elapsed();
            let _ = child.kill();
            let _ = child.wait();
            let micros = line
                .strip_prefix("first_frame_us=")
                .and_then(|value| value.parse::<f64>().ok());
            match micros {
                Some(_) if run == 0 => {}
                Some(micros) => {
                    first_frame.push(micros / 1000.0);
                    launch.push(reported.as_secs_f64() * 1000.0);
                }
                None => failures += 1,
            }
        }
        results.samples(&format!("{name}.first_frame_ms"), first_frame);
        results.samples(&format!("{name}.launch_ms"), launch);
        results.metric(format!("{name}.failures"), failures as f64);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("choscorui-bench currently supports macOS only");
}
