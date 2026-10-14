# Single component gallery replaces examples/ and samples/
- Status: Ready for implementation
- Date: 2026-10-12
- Source: `/brainstorm` request: "remove all samples/ and examples/, create 1 crate to display all components in 1 window, as the gallery/component set, and future proof (when a new component/control is added, this updates too)". The user's stated purpose: "using for development: I can run the showcase and check each component super easy". Decisions were answered one at a time in the brainstorm session on 2026-10-12. Evidence comes from the workspace at commit `2d3c34b` plus uncommitted changes.

## Outcome and current state

**Beneficiary.** choscorui developers. They should be able to run one command, pick any control, and check it in every state, theme, and language. Contributors should be unable to add a public control without the gallery covering it.

**Current state (verified in the workspace):**
- `examples/gallery` (`choscorui-gallery`) is one 1,445-line `src/main.rs`. It hand-wires Button, Checkbox, TextField, a scroll list, and a TextEditor directly against `choscorui-core`, `choscorui-controls`, and `choscorui-macos`. It reads `CHOSCORUI_GALLERY_AUTOCLOSE_MS` and `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME`. On other OSes, its non-macOS `main` prints a message.
- `examples/stress` (`choscorui-stress`) is a binary plus a lib (`src/workspace.rs`, 640 lines). It provides the evidence for the Phase 12 stress gate (`CHOSCORUI_STRESS_AUTOPILOT`, `CHOSCORUI_STRESS_SETTINGS`, `CHOSCORUI_STRESS_AUTOCLOSE_MS`).
- `examples/bench` (`choscorui-bench`) runs the release budgets (`bench/budgets.txt`, `docs/BUDGETS.md`, `docs/BENCHMARKS.md`). It depends on `choscorui-stress` for `workspace::Workspace` (`src/main.rs:67`, `src/scenarios.rs:19`). It launches `choscorui-gallery` with `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME=1` for `startup.gallery`, and it measures binary sizes of `choscorui-gallery`, `choscorui-stress`, and `choscorui-bench` (`src/main.rs:118`).
- `samples/notes` (`choscorui-notes-sample`) is an excluded, separate Cargo workspace with its own `Cargo.lock`. It depends only on `choscorui` and is `#![forbid(unsafe_code)]`. It has several windows, a per-language native menu, a brand theme switched at runtime, and en/vi/he (RTL) `.lang` resources, with 10 unit tests and a `NOTES_AUTOPILOT` run of 13 checks. It is the evidence for the Phase 14 "external sample app" gate.
- `choscorui-controls` has no common control trait. `src/lib.rs` exports about 190 public names through `pub use <module>::{...};` blocks. It also defines some items directly in `lib.rs` (`Button`, `ButtonStyle`, `CommandRegistry`, `CommandId`, `Shortcut`, …). About 55 of these are visible controls. The rest are styles, inputs, visual states, errors, ids, consts, and helper functions.
- Dependency policy (`docs/LICENSES.md`): the only third-party crate allowed is `unicode-segmentation`. No `syn`, proc-macro, or test-only crates may be added.
- The facade `crates/choscorui/src/lib.rs` re-exports `core::*`, `controls::*`, `compose::*`, and `platform::*` (macOS only). The pieces the gallery needs exist there: `TextShaper` (core trait), `CoreTextShaper` (platform), `Catalog`/`Localizer`/`Locale`, `Settings`/`SettingsSchema`/`user_settings_path`/`WindowGeometry`, `Application`/`WindowOptions`/`MainMenu`, and `capabilities()`.

**Desired state.** A single workspace crate at `gallery/` (package and binary `choscorui-gallery`) shows every public control in one window. A test fails CI whenever `choscorui-controls` gains a public name that the gallery hasn't classified. `examples/` and `samples/` no longer exist. Bench lives at `tools/bench`.

## Decisions and requirements

### D1. Removal scope and relocation (Q1)
- Delete `examples/gallery`, `examples/stress`, and `samples/notes` entirely, including `samples/notes/Cargo.lock`. The `examples/` and `samples/` directories must not exist afterwards.
- Move `examples/bench` to `tools/bench`. Package name `choscorui-bench`, CLI, flags, and output stay the same.
- Move the stress workload (`examples/stress/src/workspace.rs`) into `tools/bench` as a private module, for example `tools/bench/src/workspace.rs`, and drop the `choscorui-stress` dependency. Bench's `startup.stress` scenario keeps working through bench's own `--child startup-stress` mode. Rename the scenario only if needed, and if it is renamed, update `bench/budgets.txt` and `docs/BUDGETS.md` to match.
- Remove `choscorui-stress` from bench's binary-size list. Keep the `choscorui-gallery` and `choscorui-bench` entries.
- `bench/budgets.txt` stays where it is (repo-root `bench/`).
- Rejected alternatives: deleting bench too (this loses the release budget gate), and having the gallery absorb bench (this mixes demo code with measurement code).

### D2. Location and workspace (Q2, Q10)
- The crate lives at top-level `gallery/` with `package.name = "choscorui-gallery"`, `publish = false`, `edition.workspace`, `rust-version.workspace`, and `license.workspace`.
- It is a member of the root workspace. Root `Cargo.toml` `members` becomes the 8 `crates/*` entries plus `"gallery"` and `"tools/bench"`. Remove `exclude = ["samples"]`.
- `bylaw.toml` already names `choscorui-gallery`, so check that its analysis still passes with the new path.
- Rejected alternatives: `crates/choscorui-gallery`, `tools/gallery`, and a separate own-workspace project (that would break `-p choscorui-gallery` for bench, CI, and the scripts).

### D3. API layer (Q3)
- `gallery/Cargo.toml` has exactly one dependency, `choscorui = { path = "../crates/choscorui" }`, with no target-specific or dev dependencies on other crates.
- `gallery/src/main.rs` (crate root) has `#![forbid(unsafe_code)]`. The gallery uses only paths under `choscorui::`. It must not reference `choscorui_core`, `choscorui_controls`, `choscorui_macos`, or any FFI.
- If the facade lacks something a demo needs, do not work around it. Record the gap in the final report, and either leave that demo smaller or add the missing re-export to the facade as a separate, explicitly reported change.

### D4. Future-proofing: classification and coverage test (Q4, Q6)
- **Control crates stay untouched.** Add no showcase or demo trait, method, or module to `choscorui-controls` or any library crate. The user rejected per-control showcase code inside controls ("this one is just the showcase").
- **Registry in the gallery.** All demo code lives in `gallery/src/`, grouped by category files, not one file per control. Suggested files: `inputs.rs`, `selection.rs`, `range.rs`, `text.rs`, `data.rs` (item views, headers, model widgets, canvas), `containers.rs` (group box, splitter, stacked, scroll area, dock), `navigation.rs`, `popups.rs` (menus, tooltips), `dialogs.rs`, `shell.rs` (toolbar, status bar, main window composition), `datetime.rs`, `recipes.rs` (button recipes, custom control host), `static.rs` (label, separator). Exact category names and groupings are an implementation detail.
- The gallery has one central catalog module, for example `gallery/src/catalog.rs`, containing:
  - `PAGES`: an ordered list of pages. Each page has a stable lowercase id (for example `slider`), a display title, a category, and a build function.
  - `SHOWN`: each public export name of `choscorui-controls` that a page demonstrates, mapped to the page id that shows it. One page may show several names, for example `slider` shows `Slider` and `SliderOrientation`.
  - `EXEMPT`: each public export name that is not shown, paired with a short non-empty reason (for example `("SliderStyle", "style for Slider, varied on the slider page")` or `("ContainerError", "error type")`).
- **Every name classified (Q6).** Every public name of `choscorui-controls` must appear in exactly one of `SHOWN` or `EXEMPT`. This includes items re-exported via `pub use` and items defined directly in `lib.rs` (`pub struct|enum|trait|fn|const|static|type`). No suffix-based auto-skipping.
- These count as **shown** and get pages: every visible control, plus dialogs (rendered in-window via `DialogStack`), popup menus and tooltips, dock area and panels, toolbar, status bar and main-window composition, canvas view, button recipes (`PillButtonRecipe`, `CommandLinkRecipe`), and a `CustomControl` demo hosted by `ControlHost`.
  - Note: `PillButtonRecipe` and `CommandLinkRecipe` no longer exist. See [the component behavior fixes spec](2026-10-13-component-behavior-fixes.md).
- **Coverage test** (std only, in `gallery/tests/coverage.rs` or as a `#[cfg(test)]` module, runs on every OS):
  1. Read `concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/choscorui-controls/src/lib.rs")`.
  2. Collect export names. Handle multi-line `pub use module::{A, B, ...};` blocks, single `pub use module::Name;`, and top-level `pub` item definitions. Ignore `pub(crate)` and anything inside `impl` blocks or nested modules.
  3. Fail loudly with a clear message if `lib.rs` contains a construct the parser doesn't handle, so the parser gets updated instead of silently missing names. Examples: `pub mod`, glob `pub use ...::*`, `pub use ... as ...`, or a macro-generated `pub` item.
  4. Assert that the set of export names equals the union of `SHOWN` and `EXEMPT` names, with no name in both. On failure, list the **unclassified** names ("add a page entry in SHOWN or a reason in EXEMPT in gallery/src/catalog.rs") and the **stale** names (in the catalog but no longer exported).
  5. Assert every `SHOWN` page id exists in `PAGES`, page ids are unique and lowercase `[a-z0-9-]+`, and every `EXEMPT` reason is non-empty.
- **Headless page build test** (every OS): each page's build function takes platform-neutral inputs. These are a `Tree`/bounds, the active theme tokens, the locale and layout direction, the enabled flag, and `&dyn TextShaper` (core trait). A test builds every page with a deterministic stub `TextShaper` in LTR and RTL, both enabled and disabled. It asserts the build succeeds, produces a non-empty display list, and produces at least one semantic node with a role.

### D5. Window layout and developer flow (Q5)
- One main window with:
  - **Left sidebar.** A type-to-filter text field at the top, then pages grouped by category headers. Filtering matches the page title and id, case- and diacritic-insensitive (core `fold_for_search`). Arrow keys plus Return select. The selected page is highlighted.
  - **Right pane.** The selected page only: its title, the control in useful variants (for example a slider page shows horizontal, vertical, stepped, and disabled), and a small live readout of value and focus where meaningful.
  - **Top state bar.** A theme toggle (dark/light, plus the brand theme from notes), a language selector (en/vi/he, where he mirrors to RTL), a global Enabled/Disabled toggle applied to the current page, and "Open in new window", which opens the current page in an additional window with the same state bar.
- **CLI deep link.** `cargo run -p choscorui-gallery -- <page-id>` opens directly on that page, for example `-- slider`. `--list` prints every page id with its title and exits 0. An unknown id prints an error with the closest ids (or the full list) to stderr and exits with code 2. With no argument, the gallery restores the last selected page (D6).
- Native main menu: File (New Window, Close Window, Quit), View (theme and language items), and a Go/Pages menu is optional. Rebuild it when the language changes, as notes does.
- Keyboard and VoiceOver: sidebar, state bar, and page controls are all focusable and exposed with semantic roles and names.

### D6. Gate evidence carried by the gallery (Q9)
The gallery takes over the evidence for both roadmap gates. Bench owns only measurement.
- **Phase 14 external-app gate (from notes).** Facade-only, `#![forbid(unsafe_code)]`, several windows, a native menu rebuilt per language, brand theme switched at runtime, en/vi/he translations loaded from `gallery/resources/strings/*.lang` with a compiled-in English fallback, plurals, Hebrew mirroring, a bundle-aware `resources_dir`, and VoiceOver semantics. Carry over the existing `.lang` contents where they still apply, and add the gallery UI strings (sidebar, state bar, menus, page titles).
- **Phase 12 stress gate (from stress).** These live inside the relevant pages, not on the default page, so everyday use stays fast:
  - The `ItemView` page (or a "large data" variant on it) has 100,000 records, a folded-text search, and realized rows bounded to the viewport plus overscan.
  - The `CanvasView` page has 20,000 items with zoom, zoomed hit test, rubber-band selection, culling, and virtualized item semantics.
  - The structured-input pages (`NumberInput`, `InputMask`, `DateInput`, `DecimalSpinBox`, `DateTimeEdit`) show validation feedback.
  - The dock page uses `DockLayout`, and its arrangement is persisted.
  - Desktop integration (status item, URL handler, notification capability) is exercised where `capabilities()` reports support.
- **Settings and restore.** Use `Settings` with a `SettingsSchema` (versioned, including one migration step equivalent to stress's v1→v2) at `user_settings_path("com.choscor.gallery")`. `CHOSCORUI_GALLERY_SETTINGS=<path>` overrides the location. Persist the selected page, theme, language, main window geometry, sidebar filter text, canvas zoom, and dock arrangement. Save on exit and restore on launch. A CLI page id overrides the restored page.
- **Autopilot.** `CHOSCORUI_GALLERY_AUTOPILOT=1` runs scripted checks, prints one line per check, ends with `OK (0 failures)` or `FAILED (<n> failures)`, and exits non-zero on failure. Checks must cover at least:
  - every page in `PAGES` is selected and painted in the real window
  - theme switches repaint
  - language switches to he and mirrors the layout, and the menu is rebuilt
  - a second window opens, paints, and closing every window releases it (window count returns to zero)
  - the 100k-row search returns results with bounded realized rows over 2,000 PageDowns
  - the canvas zoomed hit test and rubber band select the expected items
  - structured input rejects invalid values
  - a settings save then a full rebuild from the settings file restores page, theme, language, geometry, dock, filter, and zoom (including the migration path)
  - desktop-integration capabilities are reported
  - translations load from the resources directory, falling back to English when it's absent
- **Unit tests** in the gallery replace notes' tests: `Catalog::validate` passes for every `.lang` file against the English keys, plus plural forms and the English fallback when resources are missing.

### D7. Platforms (Q7, settled from evidence)
- Only the macOS backend is complete, and the facade's `platform` module exists only on macOS. The windowed gallery is `#[cfg(target_os = "macos")]`. On other OSes, `main` prints `choscorui-gallery currently runs on macOS` and exits 0, keeping the existing pattern.
- `catalog.rs`, page build functions, the coverage test, the headless page test, and the catalog tests are platform-neutral and compile and run on every OS.

### D8. Environment variables and automation (Q8, settled from evidence and D6)
- Keep `CHOSCORUI_GALLERY_AUTOCLOSE_MS` (CI, install-check, ax-check) and `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME` (bench `startup.gallery`, which prints `first_frame_us=<n>` and exits) with today's semantics.
- Add `CHOSCORUI_GALLERY_AUTOPILOT`, `CHOSCORUI_GALLERY_SETTINGS`, and `CHOSCORUI_GALLERY_LANG=<en|vi|he>` (startup language override, used by ax-check in place of `NOTES_LANG=he`).
- Remove every use of `CHOSCORUI_STRESS_*` and `NOTES_*` outside of history (CHANGELOG past entries stay as written).

### D9. Repository updates required
- **CI** (`.github/workflows/ci.yml`):
  - Format becomes `cargo fmt --all --check` only.
  - Remove the "Notes sample" step (the gallery is covered by the workspace clippy, MSRV, and test steps).
  - Replace the native "Stress gallery autopilot" and "Notes sample autopilot" steps with one `CHOSCORUI_GALLERY_SETTINGS="$RUNNER_TEMP/gallery.txt" CHOSCORUI_GALLERY_AUTOPILOT=1 cargo run -p choscorui-gallery --locked`.
  - Keep "Gallery first frame".
- **Dependabot** (`.github/dependabot.yml`, currently uncommitted from the open-source setup work): remove the `/samples/notes` entry, leaving root cargo and github-actions.
- **Packaging** (`scripts/macos/package.sh`): one app, `Gallery|choscorui-gallery|.|com.choscor.gallery|choscorui-gallery|gallery/resources`. Remove the Stress and Notes rows.
- **Install check** (`scripts/macos/install-check.sh`): expect one installed signed app. Keep the gallery LaunchServices and first-frame checks. Add the gallery autopilot with real per-user settings, a check that the interactive launch with `CHOSCORUI_GALLERY_AUTOCLOSE_MS` writes `~/Library/Application Support/com.choscor.gallery/settings.txt`, and a check that translations are found at `Contents/Resources/strings/he.lang`. Keep the uninstall and leftover checks. Update the install/uninstall scripts the package ships if they name the removed apps.
- **ax-check** (`scripts/macos/ax-check.sh`): dump the gallery tree, then the gallery again with `CHOSCORUI_GALLERY_LANG=he`, in place of the notes run.
- **Other files:**
  - `.gitignore`: drop `/samples/*/target/`.
  - `bylaw.toml`: confirm it passes.
  - `CONTRIBUTING.md`: remove the samples/notes commands, use the gallery autopilot, and add a short "Adding a control" note: add a page in the right category file, add the export names to `SHOWN` or `EXEMPT` in `gallery/src/catalog.rs`, and `cargo test -p choscorui-gallery` tells you what's missing.
- **Docs** (update every reference found by `grep -rnE "examples/|samples/|choscorui-(stress|notes)|NOTES_|CHOSCORUI_STRESS"` outside `target/`, `.git/`, `CHANGELOG.md` history, and `docs/specs/`):
  - `README.md`, `docs/GUIDE.md`, `docs/INSTALL.md`, `docs/RELEASING.md` (no more second `Cargo.toml` or `cargo update -w` in samples/notes), `docs/BENCHMARKS.md` and `docs/BUDGETS.md` (`tools/bench` path), `docs/LICENSES.md` (resource row → `gallery/resources/strings/*.lang` shipped in `choscorui Gallery.app/Contents/Resources`, and the lockfile wording becomes a single workspace), `docs/COOKBOOK.md`, `docs/LOCALIZATION.md`, `docs/ROADMAP_STATUS.md`.
  - In `docs/ROADMAP_STATUS.md`, re-point the Phase 12 stress gate, the Phase 13 bench rows, and the Phase 14 example/localization/external-app rows to the gallery and `tools/bench`. Keep their "Done (automated) + Manual" status. The Manual parts (VoiceOver walk through the list, canvas, and two windows) now name the gallery. These need a person (see repo memory: manual gates are human), so do not mark them done.
  - `docs/ROADMAP.md`'s "Suggested workspace shape" is historical design text. Leave it unchanged.
- **CHANGELOG** (`CHANGELOG.md`, Unreleased): add an entry describing the consolidated gallery, the coverage test, the removal of `choscorui-stress` and `choscorui-notes-sample`, and the bench move to `tools/bench`.

### Non-goals
- No changes to control behavior, styles, or public API of library crates. The only allowed exception is an explicitly reported missing facade re-export (D3).
- No Windows or Linux windowed gallery.
- No new third-party dependencies.
- No rewrite of the in-flight `docs/specs/2026-10-12-open-source-contribution-setup.md`. Only the files it produced (dependabot, CONTRIBUTING) get updated for the new layout.
- No re-baselining of budgets on reference hardware in this task (that is a manual step, see Risks).

### Edge cases and failure behavior
- `lib.rs` constructs the parser doesn't support fail the coverage test with a message naming the line (D4 step 3). They are never skipped.
- A missing resources directory falls back to compiled-in English. The autopilot reports which source was used.
- A corrupt, old, or unknown-version settings file loads with the migration or defaults, never panics, and the `LoadReport` issues are printed in autopilot.
- A restored page id that no longer exists falls back to the first page.
- A capability reported `Unsupported` makes the related demo show a disabled state with a label. The autopilot treats it as reported, not failed.
- "Open in new window" from a secondary window works the same way. Closing the last window quits.

## Acceptance criteria and public test seams
| Criterion | Observable seam |
| --- | --- |
| `examples/` and `samples/` are gone; bench lives at `tools/bench` | `test ! -e examples && test ! -e samples && test -f tools/bench/Cargo.toml`; `cargo metadata --no-deps` lists no `choscorui-stress` or `choscorui-notes-sample` |
| Gallery is a workspace member at `gallery/` named `choscorui-gallery` | `cargo metadata --format-version 1 --no-deps` shows package `choscorui-gallery` with manifest `gallery/Cargo.toml`; `cargo run -p choscorui-gallery -- --list` exits 0 and prints page ids |
| Gallery depends only on the safe facade and forbids unsafe | Gallery test parses `gallery/Cargo.toml` and asserts the only dependency is `choscorui`; `grep -n 'forbid(unsafe_code)' gallery/src/main.rs`; `grep -rnE 'choscorui_(core\|controls\|macos)' gallery/src` returns nothing |
| Every public name of `choscorui-controls` is shown or exempted with a reason | `cargo test -p choscorui-gallery` coverage test passes |
| Adding a public control without a gallery entry fails CI | Temporarily add `pub struct GalleryProbe;` to `crates/choscorui-controls/src/lib.rs`: `cargo test -p choscorui-gallery` fails and names `GalleryProbe` as unclassified; revert |
| Removing an export leaves no stale catalog entry | Coverage test reports stale names when a catalog name isn't exported (covered by a test of the comparison function with a synthetic export list) |
| Unsupported `lib.rs` syntax is not silently ignored | Unit test feeds the parser a source string with `pub mod x;` / `pub use a::*;` and asserts an error |
| Every page builds headlessly in LTR/RTL, enabled/disabled | `cargo test -p choscorui-gallery` headless page test (stub `TextShaper`), non-empty display list and semantics per page |
| Control crates gained no showcase code | `git diff --stat main -- crates/choscorui-controls crates/choscorui-core crates/choscorui-compose crates/choscorui-macos` shows no changes (facade re-export additions, if any, are listed in the report) |
| Developer can open one control directly | `CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 cargo run -p choscorui-gallery -- slider` exits 0; `cargo run -p choscorui-gallery -- nope` exits 2 with a suggestion on stderr |
| Gate evidence: windows, themes, RTL language, large data, canvas, structured input, settings/dock restore, desktop capabilities, window release | `CHOSCORUI_GALLERY_SETTINGS=$TMPDIR/g.txt CHOSCORUI_GALLERY_AUTOPILOT=1 cargo run -p choscorui-gallery --locked` prints `OK (0 failures)` and exits 0 |
| Translations validate | Gallery unit tests (`Catalog::validate` for en/vi/he, plurals, English fallback) pass |
| Bench still measures and checks budgets | `cargo build --release -p choscorui-gallery -p choscorui-bench && target/release/choscorui-bench --quick` runs every scenario, including `startup.gallery` and `startup.stress`; `--check bench/budgets.txt` reports no *missing* budget (pass/fail against numbers is hardware-dependent) |
| First-frame contract kept | `CHOSCORUI_GALLERY_REPORT_FIRST_FRAME=1 target/release/choscorui-gallery` prints `first_frame_us=<n>` and exits 0 |
| Non-macOS builds | `cargo check -p choscorui-gallery --all-targets` on a non-macOS target (or `cargo +1.88 check --workspace --all-targets` plus code inspection of the `cfg` split); headless tests have no `cfg(target_os)` gate |
| CI, packaging, install check updated | `.github/workflows/ci.yml` has no `samples/`, `stress`, or `NOTES_` references; `CHOSCORUI_UNIVERSAL=1 scripts/macos/package.sh && scripts/macos/install-check.sh dist/choscorui-*-macos-universal.zip` prints `OK (0 failures)` with one app |
| Workspace-wide gates still pass | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo +1.88 check --workspace --all-targets --locked`, `cargo test --workspace --locked`, bylaw analysis |
| No dangling references | `grep -rnE "examples/\|samples/\|choscorui-(stress\|notes)\|NOTES_\|CHOSCORUI_STRESS" --exclude-dir=target --exclude-dir=.git . \| grep -v -e '^./CHANGELOG.md' -e '^./docs/specs/' -e '^./docs/ROADMAP.md'` returns nothing |
| Dependency policy unchanged | `cargo tree --workspace -e normal --prefix none \| sort -u` lists only workspace crates and `unicode-segmentation` |

## Constraints and risks
- **Dependency policy.** Std only for all new code and tests. This is why the coverage test hand-parses `lib.rs`. Keep the parser small and strict (D4 step 3) rather than general.
- **Budgets.** `bench/budgets.txt` has `size.choscorui-gallery_mib <= 3` and a `startup.gallery` first-frame budget, both measured against the old small gallery. The new gallery (about 55 pages, i18n, settings) may exceed them. The implementer runs bench locally and reports results. If a budget fails, report it and do not silently loosen it. Re-baselining on the reference machine (`Mac16,1`) is a human step per `docs/BUDGETS.md#process`. Keep the gallery's startup lazy: build only the selected page, and generate the 100k and 20k datasets only when their page opens.
- **Gate evidence strength.** Notes proved "builds as a separate project". The gallery now proves it through a facade-only manifest test plus `forbid(unsafe_code)` inside the workspace. The user accepted this tradeoff (Q10). Update the `docs/ROADMAP_STATUS.md` wording to say exactly this.
- **Manual gates.** VoiceOver walks for the stress and external-app gates are re-pointed at the gallery and stay Manual. Do not claim them as done.
- **Facade gaps.** Writing against the facade only may expose missing re-exports, as notes did when it exposed a window leak. Report each gap. Fixing a real bug found this way is in scope only if it is small and covered by an existing smoke test; otherwise report it.
- **Uncommitted work in the tree.** The working tree has uncommitted changes from the open-source setup task (`CONTRIBUTING.md`, `.github/dependabot.yml`, `.github/workflows/security.yml`, license files, docs). Build on them and don't revert them. `docs/specs/2026-10-12-open-source-contribution-setup.md` stays as written.
- **Deferred minor details** (implementer's choice, no scope impact): category names and grouping, the exact demo variants per control, settings key names, the wording of CLI errors and autopilot check lines, and whether the Pages menu exists.
- **Rollout.** One change set. Verify the deep-link exit codes, the autopilot, and bench before updating docs, so the docs describe real commands.
- **Affected paths:** `Cargo.toml`, `Cargo.lock`, `gallery/**` (new), `tools/bench/**` (moved), `examples/**` and `samples/**` (deleted), `bench/budgets.txt` (only if scenario names change), `bylaw.toml`, `.gitignore`, `.github/workflows/ci.yml`, `.github/dependabot.yml`, `scripts/macos/{package,install-check,ax-check}.sh` and the install/uninstall scripts they ship, `CONTRIBUTING.md`, `README.md`, `CHANGELOG.md`, and `docs/{GUIDE,INSTALL,RELEASING,BENCHMARKS,BUDGETS,LICENSES,COOKBOOK,LOCALIZATION,ROADMAP_STATUS}.md`.

Read this whole spec and inspect the current workspace before implementing.

## Amendments (2026-10-12, during implementation)
- The user asked to remove Hebrew from the gallery. The gallery ships English and Vietnamese only, and `CHOSCORUI_GALLERY_LANG` accepts `en` or `vi`. The autopilot checks the Vietnamese relabel and the menu rebuild in place of the Hebrew mirror. ax-check runs with `CHOSCORUI_GALLERY_LANG=vi`, and install-check looks for `strings/vi.lang`. The right-to-left mirroring code stays and is tested headlessly with an `ar` locale.
- Latest main has no `CHANGELOG.md` (GitHub Release notes replace it), so the CHANGELOG item does not apply.
- Latest main restructured the docs into lowercase topic guides with an ASD-STE100 rule (`scripts/check-docs.sh`). The docs updates apply to those files; `docs/ROADMAP_STATUS.md` no longer exists.
