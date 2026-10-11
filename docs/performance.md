# Performance and reliability

This document describes the benchmark harness, the release budgets, the profiling findings, and the reliability checks with fixed seeds.

## Benchmark harness

`choscorui-bench` ([`tools/bench`](../tools/bench/src/main.rs)) runs the reference scenarios on real AppKit windows. It runs on macOS only.

The harness measures these values:

- **Presented frames.** Each latency sample ends when the `drawRect:` replay of the window completes (`Window::last_paint`). The harness does not use a "done" flag from the application.
- **Startup.** Each startup sample is a separate process. The gallery (with `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME=1`) and a stress-workload child process report the time from process entry to the first presented frame. The driver also records the time from spawn to report.
- **Allocations.** A counting global allocator gives the number of Rust heap allocations in each scroll frame.
- **Resources.** CPU time comes from `getrusage`. Memory footprint and wakeups come from `proc_pid_rusage`.

### Scenarios

| Scenario | Workload | Main metrics |
| --- | --- | --- |
| `startup` | Launch the gallery and the stress workload 30 times each, after one warm-up launch | `startup.*.first_frame_ms`, `startup.*.failures` |
| `idle` | Keep one unchanged window open for 60 seconds | `idle.cpu_percent`, `idle.paints`, `idle.wakeups_per_sec`, `idle.footprint_mib.after` |
| `resize` | Alternate between 400×300 and 1600×1000 points, 100 times | `resize.present_ms`, `resize.missed_configure` |
| `scroll` | Scroll the 100,000-row list of the stress workload by 10,000 points in each direction. Each frame also paints the 20,000-item canvas. | `scroll.present_ms`, `scroll.frame_interval_ms`, `scroll.peak_realized_rows`, `scroll.allocations_per_frame` |
| `theme` | Switch between a light theme and a structurally different dark theme on 240 controls, 100 times | `theme.present_ms` |
| `document` | Type 1,000 edits in the middle of a 100,000-character document with Vietnamese, CJK, emoji, combining marks, and right-to-left text | `document.present_ms`, `document.shape_ms`, `document.caret_mismatches` |
| `windows` | Open, resize, and close 20 windows, two times | `windows.first_frame_ms`, `windows.leaked_native_windows`, `windows.idle_cpu_percent_after_close` |
| `size` | Read the size of the release executables | `size.choscorui-gallery_mib` |

For each sampled metric, the report gives the median, the p95, the maximum, the mean, and the sample count. Most scenarios also record a `missed_present` count. It counts the steps that presented no frame within one second.

### Run the benchmarks

> **Caution:** The benchmark windows are foreground AppKit windows. Do not run the benchmark at the same time as a smoke test or the gallery.

1. Build the release binaries with `cargo build --release -p choscorui-gallery -p choscorui-bench`.
2. Run all scenarios with `target/release/choscorui-bench`.

The driver accepts these options:

| Option | Effect |
| --- | --- |
| `--quick` | Runs shorter workloads, for development. |
| `--only scroll,document` | Runs only the named scenarios. An unknown name exits with status 2. |
| `--out <path>` | Writes the report to `<path>`. |
| `--check bench/budgets.txt` | Compares the metrics with the budgets and exits with status 1 when a budget fails. |

Each run prints a machine manifest and the metrics. It writes the same report and every raw sample to `target/bench/results-<unix time>.txt`, or to the `--out` path. Raw results stay out of the source tree.
## Reference machine

The budgets apply to this machine. Results from other hardware need their own manifest.

| Field | Value |
| --- | --- |
| Model | `Mac16,1`, Apple M4, 24 GiB, arm64 |
| OS | macOS 26.5 (25F71) |
| Display | Built-in Liquid Retina XDR (3024 × 1964), backing scale 2.0 |
| Power | AC power |
| Toolchain | rustc 1.97.1, `--release` |
| Instrumentation | VoiceOver off; no screen recording or profiler attached |
| Workload | Full mode |

Each result file repeats the manifest: model, CPU, memory, architecture, power source, OS version and build, Rust version, Git revision, build profile, and display scale. Results from different machines therefore do not mix.

## Budgets

[`bench/budgets.txt`](../bench/budgets.txt) holds the release budgets for macOS. Each line has the form `metric <= limit`, and `#` starts a comment. Each limit is above the measured p95 by a stated margin, or at a visible goal. A 60 Hz frame is 16.7 ms, and a 30 Hz frame is 33 ms.

The baselines are from the reference machine, measured on 2026-10-08.

| Metric | Baseline | Budget |
| --- | --- | --- |
| `startup.gallery.first_frame_ms.p95` | 144–146 ms | ≤ 250 |
| `startup.stress.first_frame_ms.p95` | 223–245 ms | ≤ 400 |
| `idle.paints` | 0 | ≤ 0 |
| `idle.cpu_percent` | 0.10 % | ≤ 1 |
| `idle.wakeups_per_sec` | 4.3 | ≤ 20 |
| `idle.footprint_mib.after` | 122 MiB | ≤ 200 |
| `resize.present_ms.p95` | 19 ms | ≤ 33 |
| `scroll.present_ms.p95` | 14.8 ms | ≤ 25 |
| `scroll.frame_interval_ms.p95` | 15.5 ms | ≤ 25 |
| `scroll.peak_realized_rows` | 32 | ≤ 40 |
| `scroll.allocations_per_frame.p95` | 6,933 | ≤ 10000 |
| `document.present_ms.p95` | 20.7 ms | ≤ 33 |
| `document.shape_ms.p95` | 6.8 ms | ≤ 12 |
| `theme.present_ms.p95` | 2.7–2.9 ms | ≤ 16.7 |
| `windows.first_frame_ms.p95` | 18 ms | ≤ 33 |
| `windows.idle_cpu_percent_after_close` | 0.6 % | ≤ 3 |
| `size.choscorui-gallery_mib` | 1.13 MiB, unstripped | ≤ 3 |

These correctness counters have a budget of 0:

- `startup.gallery.failures` and `startup.stress.failures`
- `resize.missed_configure`
- `resize.missed_present`, `scroll.missed_present`, `document.missed_present`, `theme.missed_present`, and `windows.missed_present`
- `document.caret_mismatches`
- `windows.leaked_native_windows`

### How `--check` enforces the budgets

1. The driver reads the budget file before it runs a scenario.
2. After the run, it compares each metric with its limit and writes `PASS` or `FAIL` in the report.
3. A budget for a scenario that ran but did not produce the metric is a `FAIL`.
4. A budget for a scenario that did not run is a `SKIP`.
5. The driver exits with status 1 when there is at least one `FAIL`.

Run `target/release/choscorui-bench --check bench/budgets.txt` on the reference machine before a release. [releasing.md](releasing.md) gives the release steps.

### Re-measure and change budgets

- Re-measure after a change to painting, layout, text, the display list, or the run loop. Record the new baselines in the table above.
- Raise a budget only for a stated reason that the user can see. A slower implementation is not a reason.
- Each platform has its own budgets.
- A measurement on a different machine needs its own manifest and baselines. Its budgets can be different only when the manifest records the hardware.

## Profiling findings

These design facts keep the paint, layout, and input paths independent of the model size:

| Path | Design | Where |
| --- | --- | --- |
| Canvas paint | `Scene::paint` paints only the items in the visible rectangle, in paint order. The scene keeps its sorted roots, so a frame does not sort them again. | [`scene.rs`](../crates/choscorui-core/src/scene.rs) |
| Canvas transforms | An item with an identity transform does not push a transform. | [`scene.rs`](../crates/choscorui-core/src/scene.rs) |
| Damage | `damage` compares two display lists as sorted multisets of draw regions, with two allocations for each call. Identical lists produce no damage. | [`display.rs`](../crates/choscorui-core/src/display.rs) |
| Presentation | `Window::set_display_list` invalidates only the damaged region. `drawRect:` replays an `Arc<DisplayList>` and does not copy the list. | [`surface.rs`](../crates/choscorui-macos/src/surface.rs) |
| Long documents | `TextLayoutCache::layout_paragraphs` reshapes only the edited paragraph and uses the cached layout of every other paragraph. | [`text_layout.rs`](../crates/choscorui-core/src/text_layout.rs) |

`paragraph_layout_matches_whole_text_and_reshapes_only_edits` in [`tests/text_layout.rs`](../crates/choscorui-macos/tests/text_layout.rs) shows that the paragraph layout equals the layout of the whole text.

Native calls that close or release windows drain their own autorelease pool. Without a pool, AppKit autoreleases kept closed windows alive. The `windows.leaked_native_windows` budget and the lifetime smoke test guard this behavior.

Core Graphics paints on the CPU, so there is no GPU device to lose and no software fallback.

## Reliability checks

The reliability checks use fixed seeds. A failure message gives the seed, so the failure occurs again with the same seed.

### Property and fuzz tests

`cargo test --workspace` runs these tests. Set `CHOSCORUI_PROPTEST_CASES=<n>` to run more cases, for example 20,000 cases with `--release`.

| Seam | Test file | Property |
| --- | --- | --- |
| `TextBuffer` | [`core/tests/properties.rs`](../crates/choscorui-core/tests/properties.rs) | Random multilingual edits keep the caret in bounds and match a model. Undo restores each state. |
| `Tree` | Same | Random attach, detach, visibility, and focus changes keep the links consistent. Focus never stays on a detached node. |
| `Scene` | Same | After random changes, `items_in_rect` and `hit_test` equal a brute-force scan in paint order. |
| `damage` | Same | Identical lists produce no damage. A color change damages the whole rectangle of the command. |
| Settings | Same | Typed values round-trip through the text format. Random changes to a file load or fail without a panic. |
| Markdown | Same | Parsing never panics. A parse of the serialized text gives the same document. |
| SVG and path data | Same | Random documents and path strings parse or fail without a panic. |
| `SortFilterProxy` | [`controls/tests/properties.rs`](../crates/choscorui-controls/tests/properties.rs) | After random edits, sorts, and filters, the view holds each filtered row once, in collation order. |
| `NumberInput`, `DateInput`, `InputMask` | Same | Any input in five locales validates without a panic. A valid value formats and parses to the same value. A mask applied two times gives the same text. |

### Lifetime smoke test

The lifetime smoke test creates foreground windows and needs a window server. Run it alone:

```sh
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test lifetime_smoke
```

[`tests/lifetime_smoke.rs`](../crates/choscorui-macos/tests/lifetime_smoke.rs) runs one warm-up round and six measured rounds. Each round has these workloads:

- **Window churn.** Create, paint, resize, and close 15 windows.
- **Resize and theme storm.** Do 60 resizes between 320×240 and 900×600 points, with dark and light display lists. Each step must report its resize and present a frame.
- **Worker wakeups.** Post 500 tasks from each of 8 threads. No task may be lost.
- **Timer churn.** Schedule 200 timers and cancel half of them. Only the other timers fire.
- **Accessibility churn.** Replace 20 accessible elements, 30 times.

After the rounds, the test makes these checks:

- **Windows.** Only the main window is alive. A drop of the main window releases every native window (`Application::window_count`).
- **Footprint floor.** The lowest footprint of the last rounds is less than 16 MiB above the lowest footprint of the first rounds.
- **Panics.** No posted task or timer panicked.

The benchmark `idle`, `document`, and `windows` scenarios measure memory under load.
