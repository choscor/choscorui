# Developer documentation restructure
- Status: Ready for implementation
- Date: 2026-10-12
- Source: `/brainstorm` request to restructure the repository docs ("rewrite each doc for humans, describe each crate and the control list, developer only, remove all Qt port and history material, keep specs, future-proof") plus "is NOTICE needed?". Follow-up answers in the same session: fresh project with no Qt relation; accessibility out of a standalone doc; consolidated flat layout; kebab-case names; one doc per platform; no checklists; no manual release gates; ASD-STE100 writing; Mermaid diagrams (the user briefly switched to ASCII, then returned to Mermaid); CLAUDE.md must require STE, Mermaid diagrams, and current docs.

## Outcome and current state

**Beneficiary.** Developers who build apps on ChoscorUI and the maintainer who works on it. The docs must read as the documentation of a new project. They describe what the code does now, how it works, and how to use it.

**Current state (inspected at commit `8d6347a`).**
- `docs/` has 25 Markdown files (about 2,500 lines) plus `docs/specs/`. Much of it is history or planning:
  - `ROADMAP.md` (phases 0–15 and a Qt capability inventory), `ROADMAP_STATUS.md` (per-phase status), `CONTRACT.md` ("phase 0 decisions for the first prototype", with old "remains open" notes that are now false), and `SUPPORT_MATRIX.md` (a row-by-row comparison with Qt 6.11 classes and modules).
  - Phase references in `BENCHMARKS.md` ("Phase 0 designed it", "Profiling findings (phase 13)") and `RELIABILITY.md` ("Phase 13 adds").
- `README.md` mentions "phase 12 gate checks", links to `ROADMAP.md`, has a long list of doc links, and has line-count baselines (2,745 and 663) that go stale.
- `crates/choscorui-windows/README.md` mentions "Phase 10".
- `.claude/skills/release-new-version/SKILL.md` links `SUPPORT_MATRIX.md` and has a "Manual gates" section that points to RELEASING.md section 5.
- `.github/workflows/ci.yml` has a header comment about manual gates in `docs/RELEASING.md`.
- `docs/INSTALL.md` is for end users. `scripts/macos/package.sh:99` copies it into the release zip as `README.md`.
- `NOTICE` (repo root) holds the MIT text for the Rust standard library and `unicode-segmentation` 1.13.3. `package.sh:101` copies `LICENSE*` and `NOTICE*` into the zip.
- No Rust source, TOML, shell, or YAML file mentions Qt or a phase number (grep). Code comments link docs by path: `crates/choscorui/src/lib.rs:15` (VERSIONING), `crates/choscorui/tests/cookbook.rs:1` (COOKBOOK), `examples/bench/src/main.rs:1` and `examples/bench/src/scenarios.rs:1` (BENCHMARKS), `bench/budgets.txt:2` (BUDGETS), `scripts/macos/ax-check.sh:14` (ACCESSIBILITY).
- `CLAUDE.md` and `CONTRIBUTING.md` link `docs/VERSIONING.md`, `FFI.md`, `THEMING.md`, `CONTROLS.md`, `ACCESSIBILITY.md`, `LOCALIZATION.md`, `BUDGETS.md`, and `RELEASING.md`.

**Desired state.** A small set of kebab-case docs in ASD-STE100 Simplified Technical English with Mermaid diagrams. The docs describe the architecture, each crate, each control, each platform, and the maintainer processes. No file outside `docs/specs/` mentions Qt, phases, the roadmap, or project history. A CI script blocks regressions. CLAUDE.md makes the rules binding.

## Decisions and requirements

### D1. Fresh-project framing (repo-wide)
- Remove all Qt comparisons, Qt class or module names, phase numbers, roadmap references, "prototype"/"first slice" history, and "earlier revisions" notes.
- Scope: every tracked Markdown file except `docs/specs/**`. That includes `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CLAUDE.md`, crate READMEs, `samples/notes/README.md`, `.github/pull_request_template.md`, `.claude/skills/**`, and the CI header comment.
- `docs/specs/` stays as it is. Specs are dated decision records, so history there is correct.
- Rejected: keeping a "scope" or "out of scope" page (the user chose to delete with no replacement).

### D2. Target layout of `docs/` (consolidated, flat, kebab-case)
`docs/README.md` stays uppercase so GitHub renders it as the folder index. All other files are lowercase kebab-case.

| New file | Content | Built from |
| --- | --- | --- |
| `docs/README.md` | Index. A short reading order for app developers, then for maintainers. One line per doc. | new |
| `docs/architecture.md` | How ChoscorUI works. Crate list with one paragraph per crate (purpose, main types, what it depends on). Layering rules from `bylaw.toml`. The frame pipeline: native event → `Window::take_events` → retained `Tree` dispatch → controls → layout → `DisplayList` → backend painter, plus semantics → accessibility bridge. Threading model (`!Send` UI types, `Poster`, timers, panic containment). Capability queries. Dependency rule (std, hand-written FFI, `unicode-segmentation` as the one audited exception, the rule for adding a crate). Error policy summary with a link to `versioning.md`. | `CONTRACT.md` (current facts only), `GUIDE.md`, `VERSIONING.md`, `bylaw.toml`, the source |
| `docs/guide.md` | Build an app: requirements, depend on the facade, application loop, tree and identity, layout, using a control, commands, shortcuts, menus, painting, settings API, testing UIs headless, writing a custom control (`CustomControl`, `ControlHost`, measure hooks). | `GUIDE.md`, `CUSTOM_CONTROLS.md`, `SETTINGS.md` (API and lifecycle) |
| `docs/controls.md` | First a **control catalog**: one table of every public control in `choscorui::controls`, grouped (basic, text, range, data and item views, popups and dialogs, window chrome, canvas), with a one-line purpose each. Then the shared rules. Then one section per control: behavior (pointer, keyboard, focus, disabled, value, cancel) **and** its accessibility semantics (role, name/value, state, actions). Then platform-independent semantics rules (semantic tree, notifications, virtualized content, reading order in RTL, preferences). | `CONTROLS.md`, the control table and the platform-independent parts of `ACCESSIBILITY.md` |
| `docs/theming.md` | Tokens, resolution order, recipes and slots, animation, then the customization recipes. Each code snippet still comes from `crates/choscorui/tests/cookbook.rs`. | `THEMING.md`, `COOKBOOK.md` |
| `docs/text.md` | Shaping, text layout and caching, editing (`TextBuffer`, `TextField`, `TextEditor`), IME flow, rich text. | `GUIDE.md` text section, `RICH_TEXT.md`, the text parts of `CONTROLS.md` that are not behavior contracts |
| `docs/localization.md` | Locales, catalogs, plurals, formatting, resources, RTL mirroring. | `LOCALIZATION.md` |
| `docs/canvas.md` | Scene, `CanvasView`, culling, transforms, selection. | `CANVAS.md` |
| `docs/platform-macos.md` | The supported backend. Requirements, app lifecycle and run loop, windows, menus and context menus, native dialogs, clipboard and drag and drop, IME bridge, desktop integration (activation, reopen, open files and URLs, status item, notifications), system appearance and accessibility preferences, where settings are stored, capability values. A short summary of the accessibility bridge with a link to the `choscorui-macos` README. | `DESKTOP.md`, `DIALOGS.md` (native parts), `SETTINGS.md` (storage paths), `VERSIONING.md` capability table, `GUIDE.md` |
| `docs/platform-windows.md` | First line: status ("not supported; the crate compiles and has tested contracts"). Then only what exists: Win32 window shell, message loop and pump, worker posting, DPI handling, event translation, panic containment. Then a plain list of what is not implemented. No plan, no phase words. | `crates/choscorui-windows/README.md`, the old Windows paragraph of `CONTRACT.md` |
| `docs/platform-linux.md` | Same shape as Windows: status line, then backend selection, Wayland and X11 lifecycle state machines, seat input, clipboard states, then what is not implemented. Drop the "planned runtime dependencies" list. | `LINUX.md` |
| `docs/ffi.md` | FFI safety record (unchanged in substance, rewritten in STE). | `FFI.md` |
| `docs/performance.md` | Benchmark harness and scenarios, reference machine, budgets and how `--check` enforces them, profiling findings (without phase labels), reliability checks with fixed seeds. | `BENCHMARKS.md`, `BUDGETS.md`, `RELIABILITY.md` |
| `docs/releasing.md` | The release process as numbered prose steps (see D6). | `RELEASING.md` |
| `docs/versioning.md` | SemVer, deprecation, feature flags, capability queries, error reporting, compatibility suite. | `VERSIONING.md` |
| `docs/licenses.md` | License audit. Keep its "last checked" date (an audit needs it). Update it for D7. Remove the "Earlier revisions of this audit listed…" sentence. | `LICENSES.md` |

**Delete:** `ROADMAP.md`, `ROADMAP_STATUS.md`, `CONTRACT.md`, `SUPPORT_MATRIX.md`, `ACCESSIBILITY.md`, `CUSTOM_CONTROLS.md`, `COOKBOOK.md`, `RICH_TEXT.md`, `DESKTOP.md`, `DIALOGS.md`, `SETTINGS.md`, `LINUX.md`, `BENCHMARKS.md`, `BUDGETS.md`, `RELIABILITY.md`, and the uppercase originals of every renamed file. Use `git mv` for one-to-one renames (`FFI.md`→`ffi.md`, `VERSIONING.md`→`versioning.md`, `LICENSES.md`→`licenses.md`, `RELEASING.md`→`releasing.md`, `LOCALIZATION.md`→`localization.md`, `CANVAS.md`→`canvas.md`, `THEMING.md`→`theming.md`, `CONTROLS.md`→`controls.md`, `GUIDE.md`→`guide.md`) so history follows. Note: macOS file systems are usually case-insensitive, so do a case-only rename in two steps (`git mv FFI.md ffi.tmp.md && git mv ffi.tmp.md ffi.md`).

**Content rules for every doc.**
- Describe current behavior in the present tense. Verify each claim against the source or a test before you write it. Do not copy a sentence from an old doc without checking it. `CONTRACT.md` has many "remains open" claims that are no longer true.
- Do not write counts or dates that drift (line counts, "currently N controls", "as of"). Exceptions: the license audit date, the reference machine record in `performance.md`, and the numbers in `bench/budgets.txt` that the doc cites.
- Prefer a link to the test that pins a behavior over a long description.
- Each doc that describes a flow, a layering, or a state machine has at least one Mermaid diagram (D5). Expected diagrams: crate dependency graph and frame pipeline (`architecture.md`), theme resolution order (`theming.md`), IME marked-text flow (`text.md`), settings load lifecycle (`guide.md`), Wayland configure handshake (`platform-linux.md`), release flow (`releasing.md`).

### D3. Accessibility content leaves `docs/` as a standalone doc
- The per-control role, name/value, state, and action table goes into each control section of `docs/controls.md`. The platform-independent rules (semantic tree, notifications, virtualized content, reading order, preferences) go into a section of `docs/controls.md`.
- The macOS bridge details (`NSAccessibilityElement` reconciliation, AX role mapping, notification posting, AX action callbacks) go into a new `crates/choscorui-macos/README.md` and the rustdoc of `crates/choscorui-macos/src/accessibility.rs`. `platform-macos.md` has a short summary and a link.
- **Delete the manual VoiceOver checklist.** Do not move it anywhere.
- No `ACCESSIBILITY.md` anywhere. GitHub gives that filename no special treatment. Its community-health files are README, LICENSE, CODE_OF_CONDUCT, CONTRIBUTING, SECURITY, SUPPORT, GOVERNANCE, FUNDING, and the issue and PR templates.
- `scripts/macos/ax-check.sh` stays as an optional diagnostic. Change its comment at line 14 to point to the `choscorui-macos` README. Describe it in one line in `platform-macos.md`. It is no longer a release gate.

### D4. ASD-STE100 writing rules
All docs in scope of D1, except `CLAUDE.md`, `.claude/**`, and `docs/specs/**`, follow these ASD-STE100 rules:
1. Write procedures as numbered steps. Write one instruction in each step, in the imperative ("Run …", "Set …").
2. Keep a procedural sentence to 20 words or fewer and a descriptive sentence to 25 words or fewer. A code span, path, or command counts as one word.
3. Use the active voice. Use simple verb tenses (present, past, simple future).
4. Keep a paragraph to 6 sentences or fewer, on one topic.
5. Use words from the ASD-STE100 dictionary in their approved meaning. Use technical names (types, functions, commands, OS features) exactly as they are. Use one word for one thing; do not use synonyms for variety.
6. Do not use phrasal verbs or slang. Keep articles ("a", "the"). Do not join nouns into long noun clusters; three nouns at most.
7. Put a warning or caution before the step that it applies to.

`scripts/check-docs.sh` checks rule 2 automatically (D8). The other rules are a review duty, because no free STE dictionary checker exists.

### D5. Diagrams are Mermaid
- Draw diagrams as Mermaid in fenced code blocks with the `mermaid` info string. GitHub renders them in place.
- Use the simple, stable diagram types: `flowchart`, `sequenceDiagram`, and `stateDiagram-v2`. Keep each diagram small (about 15 nodes or fewer) and give nodes plain labels, so the source still reads as text in a terminal.
- Do not use images, ASCII-art diagrams, or external diagram tools. (The user asked for Mermaid, briefly switched to ASCII, then returned to Mermaid.)
- Verify each diagram renders: view the file on GitHub (or a Mermaid live preview) before you finish.

### D6. Release process: no checklists, no manual gates
- Rewrite `docs/releasing.md` as numbered prose steps with no `- [ ]` checkboxes: license audit, version bump, release notes, automated checks (including `choscorui-bench --check bench/budgets.txt` on the reference machine), signed and notarized archive, publish, and security fixes.
- **Remove the manual-gates section** (fresh machine, Intel hardware, input methods, external accessibility tree, VoiceOver, notifications, budgets on other hardware).
- Update the dependents in the same change:
  - `CLAUDE.md` "Evidence and manual gates": rewrite as "Evidence" (D9).
  - `CONTRIBUTING.md` "Scope of desktop evidence": keep the statement that automated runs do not verify real input methods, VoiceOver, a clean-machine notarized install, Intel hardware, or budgets on other hardware. Remove the "manual release gates" link.
  - `.claude/skills/release-new-version/SKILL.md`: remove the "Manual gates" section and the manual-gate item in the final report. Replace section-number references ("RELEASING.md section 3/4") with heading names. Replace the `SUPPORT_MATRIX.md` link under Known limitations with `docs/platform-macos.md` and `docs/controls.md`.
  - `.github/workflows/ci.yml` header comment: remove the manual-gates sentence.
- **Assumption (keep):** the honesty rule stays. Never report real IME, VoiceOver, notarized clean-machine install, Intel hardware, or budgets on other hardware as verified from automated evidence. Removing the gates removes a release step, not the duty to report evidence correctly.

### D7. NOTICE becomes an archive-only file
- Findings: Apache-2.0 §4(d) applies only when the upstream work includes a NOTICE file. Neither `unicode-segmentation` 1.13.3 (it ships COPYRIGHT, LICENSE-APACHE, LICENSE-MIT) nor the Rust standard library has one. MIT requires its copyright and permission notice to go with copies, and the release zip statically links both components, so the zip must carry that text. The source repository does not contain their code. A root file named `NOTICE` becomes ChoscorUI's own Apache §4(d) notice, which redistributors of derivative works must then carry.
- Action: `git mv NOTICE scripts/macos/third-party-notices.txt`. In `package.sh`, copy `LICENSE-MIT` and `LICENSE-APACHE` from the root and copy `scripts/macos/third-party-notices.txt` to `$stage/THIRD-PARTY-NOTICES.txt`. Remove the `NOTICE*` glob. If `install-check.sh` checks archive contents, update it to match.
- Update `docs/licenses.md` and `docs/releasing.md` to name `THIRD-PARTY-NOTICES.txt` in the archive and its source path.
- Rejected: keep `NOTICE` as is (the user overrides the earlier license spec, `docs/specs/2026-10-12-open-source-contribution-setup.md`, which said to keep it; do not edit that spec); rename at root; delete (the zip would not comply with MIT).

### D8. `scripts/check-docs.sh` and CI
Add an executable `scripts/check-docs.sh` in bash and awk only (no new crate, no network). It must run with macOS bash 3.2 and BSD awk, because the CI runners are macOS. It checks every git-tracked `*.md` file except `docs/specs/**`, prints `file:line: message` for each finding, and exits 1 when there is a finding (0 when there is none). Checks:
1. **Links.** Each relative Markdown link target exists. A `#anchor` on a link to a Markdown file matches a heading in that file (GitHub slug rules: lowercase, drop punctuation except `-`, spaces become `-`). External `http(s)` links are not checked.
2. **Banned terms**, outside fenced code blocks: `\bQt\b`, `[Pp]hase [0-9]`, `[Rr]oadmap`, and a checkbox list item `^\s*[-*] \[[ xX]\]`.
3. **Sentence length**, only for the STE scope of D4: a prose sentence (paragraph text and list items; not headings, tables, fenced blocks, or HTML comments) longer than 25 words fails. A backtick span counts as one word. Split sentences on `.`, `?`, or `!` followed by a space or end of line.

Wire it in:
- `.github/workflows/ci.yml` `check` job: add a `Docs` step that runs `scripts/check-docs.sh`.
- `CONTRIBUTING.md` fast checks list: add `scripts/check-docs.sh`.

### D9. CLAUDE.md
- Update every doc link to the new paths (`docs/versioning.md`, `docs/ffi.md`, `docs/theming.md`, `docs/controls.md`, `docs/localization.md`, `docs/performance.md`, `docs/releasing.md`). Point "accessibility semantics" to `docs/controls.md`.
- Replace "Evidence and manual gates" with an "Evidence" section: automated unit, smoke, and autopilot runs do not verify real input methods, VoiceOver, a notarized install on a clean machine, Intel hardware, or budgets on other hardware. Never report these as verified from automated evidence. Report exact commands and outcomes, and name relevant checks not run and why.
- Add a "Documentation" section:
  - `docs/` describes the current code only. Update the affected docs in the same change as the code (new or changed control, public API, platform behavior, process, or dependency). A change that makes a doc wrong is not complete.
  - Do not write history, phases, roadmap items, comparisons with other toolkits, or drifting counts and dates in docs. Dated decision records go in `docs/specs/`.
  - Write docs in ASD-STE100 Simplified Technical English (the D4 rules, in short form).
  - Draw diagrams in ```` ```mermaid ```` fences (`flowchart`, `sequenceDiagram`, `stateDiagram-v2`). Do not use images or ASCII-art diagrams.
  - Name files in lowercase kebab-case (`docs/README.md` is the exception). Keep the control catalog in `docs/controls.md` complete.
  - Run `scripts/check-docs.sh` before you finish a change that touches Markdown.
- Add `scripts/check-docs.sh` to the "Before finishing" paragraph at the top.

### D10. Root README and CONTRIBUTING (slim, STE)
- `README.md`: what ChoscorUI is (2–3 sentences), supported platform and requirements, how to run the gallery, the stress gallery, and the notes sample (one command each), a link to `docs/README.md`, install/release/security links, and the license section (keep its text). Remove the phase wording, the long link list, the control walkthrough paragraph, the smoke-test and bench commands (they go in `CONTRIBUTING.md` and `docs/performance.md`), and the static-analysis section.
- `CONTRIBUTING.md`: move the static-analysis tools, pinned versions, and commands from the README into it. Remove the hard-coded baseline line counts and keep the ratchet rule. Update links. Write in STE.
- `samples/notes/README.md`, `crates/choscorui-windows/README.md`, and `SECURITY.md`: rewrite in STE and remove history. `crates/choscorui-windows/README.md` becomes a short summary that links `docs/platform-windows.md`.

### D11. Other path updates
- `scripts/macos/package.sh`: `docs/INSTALL.md` moves to `scripts/macos/archive-readme.md` (`git mv`), still copied to `$stage/README.md`. Rewrite it in STE; it is end-user text, so keep it short and procedural.
- Code-comment links: `crates/choscorui/src/lib.rs` → `docs/versioning.md`; `crates/choscorui/tests/cookbook.rs` → `docs/theming.md`; `examples/bench/src/main.rs` and `scenarios.rs` → `docs/performance.md`; `bench/budgets.txt` → `docs/performance.md`.
- `.claude/skills/pr-description/SKILL.md` → `docs/ffi.md`. `.claude/skills/release-new-version/SKILL.md` → new paths (also D6).
- Grep the repo for every old filename (`grep -rnE 'ROADMAP|CONTRACT\.md|SUPPORT_MATRIX|ACCESSIBILITY\.md|COOKBOOK|CUSTOM_CONTROLS|RICH_TEXT|DESKTOP\.md|DIALOGS|SETTINGS\.md|LINUX\.md|BENCHMARKS|BUDGETS|RELIABILITY|INSTALL\.md|GUIDE\.md|THEMING\.md|CONTROLS\.md|LOCALIZATION\.md|CANVAS\.md|FFI\.md|VERSIONING\.md|LICENSES\.md|RELEASING\.md|NOTICE' --exclude-dir=target --exclude-dir=specs .`). The only matches allowed are the Apache license text in `LICENSE-APACHE`.

### Non-goals
- No change to Rust behavior, public API, golden files, or `docs/specs/**`.
- No new crate or tool dependency. No rustdoc rewrite beyond `choscorui-macos` accessibility (D3).
- No automated STE dictionary check and no Markdown-parsing Rust test for control coverage (the user chose the script-only option).

### Commits
Use Conventional Commits with scope `docs` (and `ci` for the workflow step, `macos` for packaging if split). Suggested split: (1) `docs(docs): restructure the developer documentation` with the renames, rewrites, deletions, and link updates; (2) `ci(docs): add the docs check`; (3) `build(macos): ship third-party notices from the packaging directory`. This is not a breaking API change. Do not add `!`.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| `docs/` has exactly the D2 files plus `specs/` | `git ls-files docs ':!docs/specs'` lists `README.md`, `architecture.md`, `guide.md`, `controls.md`, `theming.md`, `text.md`, `localization.md`, `canvas.md`, `platform-macos.md`, `platform-windows.md`, `platform-linux.md`, `ffi.md`, `performance.md`, `releasing.md`, `versioning.md`, `licenses.md`, and nothing else |
| Specs untouched | `git diff main -- docs/specs/2026-10-12-open-source-contribution-setup.md` is empty |
| No history, Qt, roadmap, or checklist outside specs | `scripts/check-docs.sh` exits 0. Also `git grep -nIE '\bQt\b\|[Pp]hase [0-9]\|[Rr]oadmap' -- ':!docs/specs' ':!LICENSE-APACHE'` prints nothing |
| No old doc paths remain | The D11 grep matches only `LICENSE-APACHE` and references to the D7 file name `THIRD-PARTY-NOTICES.txt` (correction: the `NOTICE` pattern necessarily matches that name) |
| Links resolve | `scripts/check-docs.sh` exits 0, and it exits 1 with `file:line` output when you add a broken link or anchor to a scratch copy of a doc |
| Banned-term and sentence checks work | Add `Phase 3`, `- [ ] x`, and a 30-word sentence to a temporary tracked doc. The script reports three findings and exits 1. Revert. |
| Docs check runs in CI | `.github/workflows/ci.yml` `check` job has a step that runs `scripts/check-docs.sh`. `CONTRIBUTING.md` lists it. |
| Every public control is in the catalog | Review: each control type exported from `choscorui::controls` (see `crates/choscorui-controls/src/lib.rs`) has a catalog row and a section in `docs/controls.md` |
| Each control section has behavior and accessibility semantics | Review of `docs/controls.md`: every section has role, name/value, state, and actions |
| Accessibility doc and VoiceOver checklist gone | `git ls-files \| grep -i accessibility` lists only source files and `crates/choscorui-macos/src/accessibility.rs`. `git grep -n 'VoiceOver checklist' -- ':!docs/specs'` prints nothing (correction: this spec itself names the checklist) |
| macOS bridge documented | `crates/choscorui-macos/README.md` exists and describes the AX bridge. `docs/platform-macos.md` links it |
| Diagrams are Mermaid and render | Each expected D2 diagram is present in a ```` ```mermaid ```` fence, and each renders without a syntax error on GitHub (or a Mermaid live preview) |
| Cookbook snippets still compile | `cargo test -p choscorui --test cookbook --locked` passes. Snippets in `docs/theming.md` match the test code |
| Releasing has no checklist and no manual gates | `docs/releasing.md` has no `- [ ]` and no "Manual gates" heading. The release skill has no "Manual gates" section |
| NOTICE moved and still shipped | Root `NOTICE` is gone. `CHOSCORUI_UNIVERSAL=1 scripts/macos/package.sh`, then `unzip -l dist/choscorui-*-macos-universal.zip` lists `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD-PARTY-NOTICES.txt`. `scripts/macos/install-check.sh dist/choscorui-*-macos-universal.zip` passes |
| Archive README moved | `scripts/macos/archive-readme.md` exists. `package.sh` copies it. `docs/INSTALL.md` is gone |
| CLAUDE.md states the rules | `CLAUDE.md` has a "Documentation" section (STE, Mermaid diagrams, same-change updates, no history, kebab-case, run the script) and an "Evidence" section with no manual-gate wording. All its links resolve (script check) |
| Root docs slim | `README.md` has no smoke, bench, or static-analysis commands and links `docs/README.md`. `CONTRIBUTING.md` has the static-analysis section without baseline line counts |
| Repo checks still pass | The CI `check` job commands in `CONTRIBUTING.md` pass, plus `cargo bylaw check` and `cargo machete --with-metadata` |

## Constraints and risks
- **Accuracy over speed.** Old docs contain false "remains open" claims (`CONTRACT.md`) and old numbers. Verify every rewritten claim against `crates/*/src` and tests. Where you cannot verify a claim, remove it rather than guess.
- **STE versus technical names.** Type names, AppKit names, and commands are technical names and are allowed. Do not rename API terms to fit the dictionary.
- **Sentence-length check false positives.** Abbreviations such as "e.g." can split sentences wrongly. Write "for example" instead (that is also correct STE). Do not add an exception list to the script unless a real need appears; if one is needed, give the reason next to it (CLAUDE.md rule on exceptions).
- **Case-only renames** on case-insensitive macOS file systems need the two-step `git mv`.
- **Release skill and CLAUDE.md** are agent instructions. They are outside the STE sentence check, but they get the link and banned-term checks.
- **Golden files and API** do not change, so no release-notes call-out is needed. The PR description must mention the removed manual release gates and the moved NOTICE, because they change the release process.
- **Memory note.** The assistant memory `roadmap-manual-gates-are-human` refers to the roadmap and manual gates. It becomes stale after this change; update or delete it after implementation.
- **Deferred minor details** (do not change scope or acceptance): exact section order inside each doc; exact wording of diagrams; whether `ax-check.sh` gets more than one line in `platform-macos.md`.
- Existing artifact paths: `docs/*.md` (current), `docs/specs/2026-10-12-open-source-contribution-setup.md`, `CLAUDE.md`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `NOTICE`, `scripts/macos/package.sh`, `scripts/macos/install-check.sh`, `scripts/macos/ax-check.sh`, `.github/workflows/ci.yml`, `.github/pull_request_template.md`, `.claude/skills/*/SKILL.md`, `crates/choscorui-windows/README.md`, `samples/notes/README.md`, `bylaw.toml`.

Read this whole spec and inspect the current workspace before implementing.
