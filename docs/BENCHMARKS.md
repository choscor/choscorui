# Phase 0 benchmark harness design

The harness will run the same gallery scenarios on every supported backend. This is a measurement plan, not a performance claim. No release threshold is set until the window, display list, and text prototypes produce a baseline.

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
