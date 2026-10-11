# Repository instructions

Before finishing a Rust change, run the fast checks from the CI `check` job in
[CONTRIBUTING.md](CONTRIBUTING.md#before-submitting-a-maintainer-change). For
backend or UI changes, also run the native checks from the CI `native` job. The
[CI workflow](.github/workflows/ci.yml) and the documented commands are
canonical. Fix findings rather than adding suppressions or exceptions; when an
exception is necessary, give the reason next to it.

## Crate ownership

- `choscorui` is the safe facade applications depend on. It forbids `unsafe`
  and exposes no native handles. Its `prelude`, `core`, `controls`, `compose`,
  `platform`, and `capabilities()` are the stable surface
  ([VERSIONING.md](docs/VERSIONING.md)); the other crates are implementation
  details.
- `choscorui-core` is the base of the workspace: retained tree, layout, text,
  models, theme tokens, settings, and semantics. Platform-independent behavior
  and policy live here or in `controls`/`compose`, not in a backend.
- `choscorui-controls`, `choscorui-compose`, `choscorui-render`, and the
  platform crates depend only on core. No library depends on the gallery.
  `bylaw.toml` enforces this; run `cargo bylaw check` when adding a dependency
  or module that crosses crates.
- Hand-written `unsafe` FFI belongs in the platform crates. Each use documents
  its safety invariant, is recorded in [docs/FFI.md](docs/FFI.md), and has a
  focused test. A panic in a callback is caught at the FFI boundary and never
  unwinds into AppKit.
- Query `capabilities()` instead of the OS version or `cfg!(target_os)` to
  decide behavior. New capability fields go on the `#[non_exhaustive]` struct.
- Fallible public operations return a domain error enum, not a panic. Error
  types implement `Debug`, `Display`, and `Error`, are `Send + Sync + 'static`,
  and use lowercase messages without a trailing period.

## Theming, controls, and accessibility

- Visual values come from theme tokens and recipes
  ([THEMING.md](docs/THEMING.md)). Do not hard-code colors, sizes, radii, or
  font sizes in controls; add a token only when it has a real second use.
- Control behavior follows [CONTROLS.md](docs/CONTROLS.md). Update that contract
  in the same change when behavior changes, and keep focus order, keyboard
  activation, and accessibility semantics intact
  ([ACCESSIBILITY.md](docs/ACCESSIBILITY.md)).
- User-visible strings in samples go through localization catalogs
  ([LOCALIZATION.md](docs/LOCALIZATION.md)); right-to-left layouts must mirror.
- The golden files in `crates/choscorui/tests/golden/` pin covered behavior.
  Regenerate them with `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility`
  only for an intended change, review the diff, and call the behavior change
  out in the commit body and PR so it reaches the release notes.

## Performance and reliability

- Large lists, trees, tables, and the canvas are virtualized; work on paint,
  layout, and input paths must not scale with the whole model. Check changes on
  these paths against [BUDGETS.md](docs/BUDGETS.md) with `choscorui-bench --check`.
- Never weaken a test to hide a race. Required tests run once; do not add
  retries, skips, or timeout-only fixes. Prefer relative measurements to
  wall-clock thresholds in unit tests.
- Native smoke tests, the gallery, and autopilot runs create foreground AppKit
  windows. Run them one at a time, never in parallel.

## Code size, dead code, and suppressions

- `rustquty.toml` limits files to 3,000 lines and functions to 700 lines, and
  `quality/baseline.json` ratchets the current maxima. Split by responsibility;
  do not compress formatting or delete useful comments to fit.
- An `#[allow(...)]` or `#[expect(...)]` names one specific lint at the
  narrowest location and states why the code is correct. No blanket allowances.
- Delete code when its last caller goes; do not keep wrappers or unused
  parameters for tests. Run `cargo machete --with-metadata` after dependency
  changes.

## Evidence and manual gates

- Unit, smoke, and autopilot runs do not verify IME with real input methods,
  VoiceOver, notarized installation on a clean machine, Intel hardware, or
  budgets on non-reference hardware. Those are the manual gates in
  [RELEASING.md](docs/RELEASING.md); they need a person. Never report one as
  passed from automated evidence.
- Report exact commands run and their outcomes; name relevant checks not run
  and why.

## Release verification

- Before preparing a release, read `.claude/skills/release-new-version/SKILL.md`
  and [RELEASING.md](docs/RELEASING.md).
- There is no changelog file. Release notes are written per release from
  `scripts/release-notes.sh <previous-tag> <tag>` plus hand-written highlights,
  and published as the GitHub Release text.
- Keep the tag, the packaged archive, and the release notes on one exact source
  commit. Never move or force-push a published tag or replace published assets.

## Commit messages

Use Conventional Commits. Include a type, scope, description, body, and footer,
with a blank line between each section:

```
<type>(<scope>): <description>

<body>

<footer>
```

Scopes name the area a contributor recognizes, such as `core`, `controls`,
`compose`, `render`, `macos`, `facade`, `gallery`, `bench`, `docs`, or `ci`.
Mark breaking changes with `!` or a `BREAKING CHANGE:` footer so
`scripts/release-notes.sh` lists them.
