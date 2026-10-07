# Benchmarks

The harness runs the reference scenarios below on real windows. Phase 0 designed it, and `examples/bench` (`choscorui-bench`) implements it for macOS. Measured baselines and the release budgets derived from them are in [BUDGETS.md](BUDGETS.md).

## Running

```sh
cargo build --release -p choscorui-gallery -p choscorui-bench
target/release/choscorui-bench                       # full run, about 2 minutes
target/release/choscorui-bench --quick               # shorter workloads
target/release/choscorui-bench --only scroll,document
target/release/choscorui-bench --check bench/budgets.txt   # exit 1 on a failed budget
```

Each run prints a machine manifest and its metrics. It writes the same report plus every raw sample to `target/bench/results-<unix time>.txt`, or to the path given with `--out`. Raw results stay out of the source tree.

What the driver records:
- **Presented frames.** Every latency sample ends when the window's `drawRect:` replay completes (`Window::last_paint`). The driver does not trust an app-side "done" flag.
- **Startup.** Startup times separate processes. The gallery (`CHOSCORUI_GALLERY_REPORT_FIRST_FRAME=1`) and a stress-gallery child each report the time from process entry to first paint. The driver also records spawn-to-report time.
- **Allocations.** A counting global allocator reports Rust heap allocations per scroll frame.
- **Resources.** CPU time comes from `getrusage`. Memory footprint and wakeups come from `proc_pid_rusage`. Both use hand-written libSystem declarations.


## Reference machine

Initial development machine: `Mac16,1`, Apple M4, 24 GiB RAM, macOS 26.5, arm64. Each result must also record display model, active scale, power source, OS build, Rust version, Git revision, build profile, and whether accessibility or screen recording instrumentation was enabled. Windows, Wayland, and X11 baselines require named machines and sessions before those backends pass a gate.

## Scenarios and measurements

| Scenario | Workload | Metric and observation point |
| --- | --- | --- |
| Startup | Cold launch to first presented gallery frame, 30 runs after one warm-up | Monotonic elapsed time from process entry to frame callback completion; median and p95. |
| Idle | Visible, unchanged gallery window for 60 seconds after settling | Process CPU time divided by wall time, resident memory before/after, and frame callback count. Expect no repeated drawing. |
| Resize | Scripted alternation between 400×300 and 1600×1000 logical points, 100 transitions | Input or configure receipt to presented frame; median, p95, maximum, missed configure count. |
| Large list scroll | 100,000 keyed rows; scripted 10,000 logical-point wheel path in both directions | Presented frame intervals, p95, maximum, live node count, peak resident memory. |
| Multilingual document | 100,000 characters mixing Vietnamese, CJK, emoji, combining marks, and RTL; type 1,000 edits at caret | Key/IME commit to paint completion, p95; caret geometry mismatches; memory. |
| Theme switch | Alternate light and structurally distinct dark themes 100 times | Switch request to presented frame, p95; layout and paint invalidation counts. |
| Multiple windows | Open, resize, close, reopen 20 windows | Time to first frame per window, final native handle count, idle CPU after close. |
| Build size | Release gallery executable and installable package | Binary and package byte counts, with symbol-stripping policy recorded. |

The gallery will emit monotonic timestamps at input receipt, dispatch completion, layout completion, display-list completion, and present callback. A separate driver will trigger input and collect timestamps so app-side instrumentation does not define its own success. Store raw samples and summary statistics beside a machine manifest, not in the source tree. Repeat runs under a fixed display scale and theme, then repeat scale-sensitive scenarios at 1x and 2x. Record background load and thermal state; discard a run only with a stated reason.

The prototype will establish observed distributions. Release budgets will be chosen from those distributions and user-visible responsiveness goals, then checked on each platform independently. A green core unit test cannot stand in for an on-screen latency measurement.

## Profiling findings (phase 13)

The first harness runs found these hotspots, all now fixed:

| Path | Before | Cause | Fix | After |
| --- | --- | --- | --- | --- |
| Large-canvas paint, 20,000 visible items | 16 ms build, 85k allocations per frame | Per frame, two `BTreeSet`s, a clone and sort of all roots, a save/restore per item, and a path per rectangle. In `damage`, one `Vec` per distinct command. | Paint-order filtering. Cached sorted roots. Identity transforms skipped. Solid rectangles use `FillRect`. Paths pre-sized. `damage` is a sorted merge. | 8 ms build, 7k allocations (one per ellipse path) |
| `drawRect:` | Cloned the whole display list on every paint | | Paint state holds `Arc<DisplayList>` | Replay only |
| Typing in a 100,000-character document | 94 ms reshape, 106 ms to present | Every edit reshaped the whole document | `TextLayoutCache::layout_paragraphs` reshapes only the edited paragraph. It reuses cached paragraphs and gives the same result as whole-text shaping (`paragraph_layout_matches_whole_text_and_reshapes_only_edits`). | 5 ms reshape, 18 ms to present |

The remaining costs:
- **Scroll frames.** Display-list diffing (`damage`) takes about half the build time of a 20,000-item scroll frame. Core Graphics replay takes about 7 ms.
- **Software fallback.** None is needed. The macOS painter is Core Graphics on the CPU, so there is no GPU device to lose.
