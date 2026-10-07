# Release budgets and baselines (macOS)

[`bench/budgets.txt`](../bench/budgets.txt) holds the release budgets, and `choscorui-bench --check bench/budgets.txt` enforces them (see [BENCHMARKS.md](BENCHMARKS.md)). A release candidate must pass every budget on the reference machine. Correctness counters must stay at zero: missed frames, missed configure events, caret mismatches, leaked windows, and startup failures.

## Reference machine and run conditions

| Field | Value |
| --- | --- |
| Model | `Mac16,1`, Apple M4, 24 GiB, arm64 |
| OS | macOS 26.5 (25F71) |
| Display | Built-in Liquid Retina XDR (3024 × 1964), backing scale 2.0 |
| Power | AC power |
| Toolchain | rustc 1.97.1, `--release` |
| Instrumentation | VoiceOver off; no screen recording or profiler attached |
| Workload | Full mode: 30 launches per startup target after a warm-up, 60 s idle, 100 resizes, a 10,000-point wheel path each way, 1,000 edits, 100 theme switches, 2 × 20 windows |

Every result file repeats this manifest (model, CPU, memory, power source, OS build, Rust version, Git revision, profile, and scale), so results from different machines are never mixed.

## Baselines and budgets

Measured on 2026-10-08 at the revision that introduced the harness. The baseline column is the p95 from two consecutive full runs, or the value for non-percentile metrics.

| Metric | Baseline | Budget | Basis |
| --- | --- | --- | --- |
| Gallery startup to first frame, p95 | 144–146 ms (median 119–125) | ≤ 250 ms | Margin of about 1.7× for cold disk caches and background load |
| Stress-gallery startup (100,000 rows, 20,000 items), p95 | 223–245 ms | ≤ 400 ms | About 1.6× margin |
| Idle CPU | 0.10 % | ≤ 1 % | No timers or animation run at idle |
| Idle repaints | 0 | 0 | No repeated drawing |
| Idle wakeups | 4.3 /s | ≤ 20 /s | |
| Idle footprint | 122 MiB (with stress data loaded) | ≤ 200 MiB | |
| Resize to presented frame, p95 | 19 ms | ≤ 33 ms | One 30 Hz frame during live resize |
| Scroll input to presented frame, p95 | 14.8 ms | ≤ 25 ms | Stays near 60 Hz; the worst case has all 20,000 canvas items visible |
| Scroll frame interval, p95 | 15.5 ms | ≤ 25 ms | |
| Realized list rows | 32 | ≤ 40 | Viewport rows plus overscan |
| Allocations per scroll frame, p95 | 6,933 | ≤ 10,000 | One per visible ellipse path |
| Keystroke to presented frame, 100,000 characters, p95 | 20.7 ms | ≤ 33 ms | Typing stays under one 30 Hz frame |
| Paragraph reshape per edit, p95 | 6.8 ms | ≤ 12 ms | |
| Theme switch to presented frame, p95 | 2.7–2.9 ms | ≤ 16.7 ms | One 60 Hz frame |
| New window to first frame, p95 | 18 ms | ≤ 33 ms | |
| CPU after closing 40 windows | 0.6 % | ≤ 3 % | Measured over the next 2 s |
| Release gallery executable | 1.13 MiB | ≤ 3 MiB | Unstripped |

## Process

- **Re-measuring.** Re-measure after changes to painting, layout, text, the display list, or the run loop. Record the new baseline in this table.
- **Changing a budget.** Raise a budget only with a stated user-visible reason. A slower implementation is not a reason.
- **Platform independence.** Each platform has its own budgets. This delivery is macOS-only (see [ROADMAP_STATUS.md](ROADMAP_STATUS.md)).
- **Other machines.** Measuring on another machine, such as an Intel Mac or an older Apple silicon model, means adding its manifest and baseline rows here. Its budgets can differ, but only with the hardware recorded.
- **Not covered by budgets.** On-screen latency under assistive technology and IME composition are validated by the manual gates in [ACCESSIBILITY.md](ACCESSIBILITY.md) and the IME notes in [ROADMAP_STATUS.md](ROADMAP_STATUS.md).
