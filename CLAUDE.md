# Repository instructions

## Checks

- Rust change: run the CI `check` job commands from
  [CONTRIBUTING.md](CONTRIBUTING.md#before-submitting-a-maintainer-change).
  Backend or UI change: also run the CI `native` job commands.
- Linux backend, facade, or controls change: also run the `linux-check` and
  `linux-native` jobs (`scripts/linux/run-in-container.sh check|native`,
  see [CONTRIBUTING.md](CONTRIBUTING.md#linux-checks)).
- Markdown change: run `scripts/check-docs.sh`.
- Fix findings; do not add suppressions. An unavoidable `#[allow]`/`#[expect]`
  names one lint at the narrowest scope, with the reason beside it.
- Report exact commands and outcomes, and name relevant checks not run and why.
- Automation never verifies real input methods, VoiceOver, Orca, a notarized
  clean install, Intel hardware, real Linux GPU drivers, GNOME or KDE,
  HiDPI monitors, or budgets off the reference machine. Do not claim it.

## Architecture

Read [architecture.md](docs/architecture.md) before a cross-crate change.

- Platform-independent behavior and policy live in core, `controls`, or
  `compose`, never in a backend.
- Branch on `capabilities()`, not the OS version or `cfg!(target_os)`.
- Fallible public APIs return a domain error enum, never panic.

## FFI

- `unsafe` lives only in platform crates. In `choscorui-macos` it lives only
  in `src/ffi/`: each block is one raw call with a `SAFETY` comment.
- Outside `ffi` the code is safe. Send messages through a registered `Msg`,
  hold objects in `Retained`/`CfRetained`, and declare C functions, SDK
  constants, structs, and callbacks through the `ffi` macros.
- `cargo test -p choscorui-macos` checks each binding against the runtime and
  the SDK headers, deprecations included. Fix the binding; never weaken, skip,
  or suppress the check.
- Catch callback panics at the boundary, and record each boundary in
  [docs/ffi.md](docs/ffi.md).

## Controls and theming

- Take visual values from theme tokens and recipes
  ([theming.md](docs/theming.md)). Add a token only when it has a second use.
- Follow [controls.md](docs/controls.md) and update it in the same change.
  Keep focus order, keyboard activation, and accessibility semantics intact.
- Sample strings go through [localization](docs/localization.md) catalogs;
  right-to-left layouts mirror.
- Golden files in `crates/choscorui/tests/golden/` pin behavior. Bless them
  (`CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility`) only for an
  intended change, review the diff, and call it out in the commit and PR.

## Performance and tests

- Paint, layout, and input work must not scale with the whole model. Check
  these paths with `choscorui-bench --check bench/budgets.txt`
  ([performance.md](docs/performance.md)).
- Do not weaken tests to hide races: no retries, skips, or timeout-only fixes.
- Native smoke tests, the gallery, and autopilot open AppKit windows. Run them
  one at a time.

## Code

- Over a size limit: split by responsibility; do not squash formatting or
  delete comments.
- Delete code with its last caller. No test-only wrappers or unused parameters.

## Docs

- Update affected `docs/` in the same change as the code.
- Before editing Markdown, read the rules in
  [CONTRIBUTING.md](CONTRIBUTING.md#documentation).

## Commit messages

Use short Conventional Commits.
