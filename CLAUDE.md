# Repository instructions

## Checks

- Rust change: run the CI `check` job commands from
  [CONTRIBUTING.md](CONTRIBUTING.md#before-submitting-a-maintainer-change).
  Backend or UI change: also run the CI `native` job commands.
- Markdown change: run `scripts/check-docs.sh`.
- Fix findings; do not add suppressions. An unavoidable `#[allow]`/`#[expect]`
  names one lint at the narrowest scope, with the reason beside it.
- Report exact commands and outcomes, and name relevant checks not run and why.
- Automation never verifies real input methods, VoiceOver, a notarized clean
  install, Intel hardware, or budgets off the reference machine. Do not claim it.

## Architecture

Read [architecture.md](docs/architecture.md) before a cross-crate change.

- Platform-independent behavior and policy live in core, `controls`, or
  `compose`, never in a backend.
- Branch on `capabilities()`, not the OS version or `cfg!(target_os)`.
- `unsafe` FFI lives only in platform crates, with a safety comment, a
  [docs/ffi.md](docs/ffi.md) entry, and a focused test. Catch callback panics
  at the FFI boundary.
- Fallible public APIs return a domain error enum, never panic.

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
