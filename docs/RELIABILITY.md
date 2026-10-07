# Reliability checks

Phase 13 adds automated checks at public seams. Each has a fixed seed, so a failure reproduces from the seed in its message.

## Property and fuzz tests

These run with `cargo test --workspace`. Set `CHOSCORUI_PROPTEST_CASES=<n>` for a longer run, for example 20,000 cases in `--release`.

| Seam | Test | Property |
| --- | --- | --- |
| `TextBuffer` | `core/tests/properties.rs` | Random multilingual edits, deletes, and cuts keep the caret in bounds. Undo revisits every distinct text in order. Redo returns to the last one. |
| `Tree` | same | Random attach, detach, visibility, and focus changes keep parent/child links consistent. Focus never rests on a detached node. Hit tests return live nodes. |
| `Scene` | same | After random adds, nesting, transforms, moves, z changes, visibility, and removals, `items_in_rect` and `hit_test` equal a brute-force scan in paint order, and paint output stays balanced. |
| `damage` | same | Identical lists produce no damage. Changing any command's color damages its whole rectangle. |
| Settings | same | Typed values with separators, quotes, escapes, and Unicode round-trip through the text format. Randomly mutated files load or fail cleanly. |
| Markdown | same | Parsing never panics. Serializing then reparsing reproduces the document exactly. |
| SVG and path data | same | Random documents and path strings, including huge and NaN-like numbers, parse or fail without panicking. |
| `SortFilterProxy` | `controls/tests/properties.rs` | After random inserts, removes, edits, sort changes, and filter changes, the view holds exactly the filtered rows, each once, in collation order. |
| `NumberInput`, `DateInput`, `InputMask` | same | Any input in five locales validates without panicking. A valid value formats and re-parses to itself. Masks are idempotent. |

Defects found by these tests and fixed:
- **Markdown runs.** Markdown output for adjacent runs with overlapping formats was ambiguous (`*a****b***`). Inline formats are now written as toggle transitions that mirror the parser, with `_` for italic.
- **Link brackets.** Brackets inside a code span closed a link early. Link scanning now skips code spans.
- **Decimal places.** `NumberInput` with two decimal places accepted `2.1229` in locales whose decimal separator is `,`, because the parser also accepts `.`. The value must now survive formatting at the configured precision.

## Native stress and resource lifetime

`CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test lifetime_smoke` repeats one round seven times. A round contains:
- **Window churn.** 15 windows are created, painted, resized, and closed.
- **Resize and theme storm.** 60 resizes alternate between 320×240 and 900×600, with display lists that alternate between a dark theme and a light theme with shadows. Every step must report its configure event and present a frame.
- **Worker wakeups.** 8 threads post 500 tasks each, and none may be lost.
- **Timer churn.** 200 timers are scheduled and half are canceled. Exactly the others fire.
- **Accessibility churn.** 30 rounds replace 20 accessible elements.

After the rounds:
- **Windows.** Only the main window is alive, and dropping it releases every native window (`Application::window_count`).
- **Footprint floor.** The minimum footprint across the last rounds must not exceed that of the first rounds by 16 MiB. Footprint swings with window backing stores, so the floor is compared rather than single samples.
- **Panics.** No posted task or timer panicked.

This test found that closed windows were never released. `Window::close` and `Drop`, and several other AppKit calls, ran without an autorelease pool, so AppKit's internal autoreleases kept every window alive. Each such call now drains its own pool. The benchmark's `windows.leaked_native_windows` budget guards against a regression.

Device loss does not apply: painting is CPU Core Graphics, with no GPU device. The benchmark's idle, document, and windows scenarios cover memory under load (see [BUDGETS.md](BUDGETS.md)).
