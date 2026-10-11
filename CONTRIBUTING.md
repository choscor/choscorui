# Contributing to choscorui

Anyone is welcome to create an issue to report a bug or suggest a change.
choscorui does not accept external pull requests. The maintainer implements
changes and opens any pull requests needed for the project.

## Maintainer pull request title and description

Keep changes small enough to explain and verify independently, include
behavior-focused tests, and avoid unrelated cleanup. One approving review is not
required while the project has a single maintainer; the automated quality gates
are the required review surface.

Use a specific, action-oriented title. The preferred format is
`<type>(<scope>): <imperative summary>`, matching the project's Conventional
Commit subjects, for example `fix(macos): keep field focus after a theme switch`.
The title should make sense when skimmed in a PR list or release history.

The [PR template](.github/pull_request_template.md) prompts for the reason,
meaningful changes, verification, and any context a reviewer cannot see in the
diff. State exact checks run and their outcomes; explain relevant checks that
were blocked or not run. Add screenshots for visible UI changes when useful, and
call out compatibility or migration effects. Link an issue if one exists; use
`Closes #N` only when the PR fully resolves it. Delete unused template sections
for small changes. Revisit the description if the change evolves during review.

## Before submitting a maintainer change

Install macOS 15 or later, the Xcode Command Line Tools
(`xcode-select --install`), Rust stable with `rustfmt` and `clippy`, and Rust
1.88 for the minimum supported version check
(`rustup toolchain install 1.88`). Run the fast checks from the CI `check` job:

```sh
cargo fmt --all --check
(cd samples/notes && cargo fmt --check)
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88 check --workspace --all-targets --locked
cargo test --workspace --locked
(cd samples/notes && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked)
```

For backend or UI changes, also run the native checks from the CI `native` job.
They create foreground AppKit windows, so run them one at a time; concurrent
runs can change focus and make input assertions unreliable.

```sh
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test window_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test menu_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test lifetime_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 CHOSCORUI_RUN_MACOS_CLIPBOARD=1 cargo test -p choscorui-macos --test clipboard_smoke --locked
CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 cargo run -p choscorui-gallery --locked
CHOSCORUI_STRESS_SETTINGS=/tmp/stress.txt CHOSCORUI_STRESS_AUTOPILOT=1 cargo run -p choscorui-stress --locked
(cd samples/notes && NOTES_AUTOPILOT=1 cargo run --locked)
```

The static-analysis tools (rustquty, cargo-machete, and cargo-bylaw), their
pinned versions, and their commands are in the README's
[Static analysis](README.md#static-analysis) section. The
[CI workflow](.github/workflows/ci.yml) and the documented commands are
canonical.

## Source file size

`rustquty.toml` limits Rust sources to 3,000 physical lines per file and 700
lines per function. `quality/baseline.json` ratchets the current maxima, so
either limit only becomes stricter as large files are split. Run the check with
the `rustquty qa` command from the README.

Split by responsibility before reaching the limit. Keep related operations
together and keep private implementation details out of public APIs. Prefer
smaller functions with one logical task; moving a large function to another
file alone does not simplify it. Do not meet the limit by compressing
formatting or removing useful comments. New modules must keep the crate layering
in `bylaw.toml` intact; check it with `cargo bylaw check`.

## Exceptions and suppressions

Fix first-party warnings and lint findings when practical. An `#[allow(...)]`
or `#[expect(...)]` must name only the specific lint at the narrowest useful
location and state why the code is correct. Blanket lint allowances, test
retries, and timeout-only fixes are not accepted. The `choscorui` facade
forbids `unsafe`. Hand-written `unsafe` FFI belongs in the platform crates; each
use must document its safety invariant, be recorded in [docs/FFI.md](docs/FFI.md),
and have a focused test.

Never weaken a test to hide a race. Required tests run once. If a gate is
temporarily removed, document the precise reason and tracking issue.

## Scope of desktop evidence

Unit tests and the opt-in smoke and autopilot runs verify behavior on real
AppKit windows. They do not verify the manual release gates in
[docs/RELEASING.md](docs/RELEASING.md), including IME composition with real
input methods, VoiceOver walkthroughs, notarized installation on a clean
machine, and budgets on non-reference hardware.

## License

choscorui is dual licensed under `MIT OR Apache-2.0`, as described in the
[README](README.md#license). Changes to the project are licensed the same way.
