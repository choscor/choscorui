# Open-source contribution setup and MIT OR Apache-2.0 dual license
- Status: Ready for implementation
- Date: 2026-10-12
- Source: The user's request: "Help me setup this repo to be open-source, just like /Users/ngthluu/choscor/opensource/choscordb, with the same CONTRIBUTING". The user then chose the full set of companion files and enabling private vulnerability reporting only. In a follow-up, the user asked to add the MIT OR Apache-2.0 dual license. This spec draws on an inspection of both repositories and their GitHub settings on 2026-10-12.

## Outcome and current state

The goal is for outside users of choscorui to have the same contribution experience as choscordb. Anyone can open issues. External pull requests are not accepted. The maintainer implements every change and follows a documented PR standard. The repository also gets the same dependency hygiene automation as choscordb. The project is also relicensed from MIT to `MIT OR Apache-2.0`. That is the Rust ecosystem norm used by egui, Tauri, Dioxus, wgpu, and the Rust standard library: Apache-2.0 adds an explicit patent grant, and MIT keeps GPLv2 compatibility.

Current state (verified 2026-10-12):
- GitHub: `choscor/choscorui` is already **public** and detected as MIT. Issues are enabled. `main` is at `2d3c34b` and matches local. There is no branch protection.
- **Private vulnerability reporting is disabled** (`gh api repos/choscor/choscorui/private-vulnerability-reporting` → `{"enabled":false}`). `SECURITY.md` tells reporters to use that form, so as things stand the documented reporting channel does not work.
- Already present in the repo: `LICENSE` (MIT), `NOTICE`, `SECURITY.md`, `CHANGELOG.md`, `README.md`, `.github/workflows/ci.yml`, and `docs/RELEASING.md`.
- Missing compared with choscordb: `CONTRIBUTING.md`, `.github/pull_request_template.md`, `.github/dependabot.yml`, `.github/workflows/security.yml`, the hardened `.gitignore` entries, and a README link to the contribution policy.
- Reference files in choscordb (read them before writing): `CONTRIBUTING.md`, `.github/pull_request_template.md`, `.github/dependabot.yml`, `.github/workflows/security.yml`, and `.gitignore`. All are under `/Users/ngthluu/choscor/opensource/choscordb/`.
- Project facts used in the adaptation:
  - A Rust-only workspace. MSRV is 1.88 (`Cargo.toml`), and edition 2024.
  - macOS 15+ only.
  - A separate cargo project lives in `samples/notes`, with its own `Cargo.lock`.
  - Static-analysis limits are in `rustquty.toml`: 3,000 lines per file and 700 per function, ratcheted by `quality/baseline.json`.
  - Layering rules are in `bylaw.toml`.
  - Hand-written FFI lives in `choscorui-macos` and is documented in `docs/FFI.md`. The `choscorui` facade forbids `unsafe`.
  - Commits use Conventional Commit subjects.
  - There is no `scripts/ci/quality.py`. The canonical checks are the commands in `.github/workflows/ci.yml` and `README.md`.

## Decisions and requirements

Ledger (all settled):
- Q1. Contribution policy: the same as choscordb. Issues are welcome, external PRs are not accepted, and the maintainer opens all PRs. Source: the user ("the same CONTRIBUTING").
- Q2. Companion files: the full set. That is the PR template, Dependabot, the Security workflow, the hardened `.gitignore`, and a README link. Source: the user's answer.
- Q3. GitHub settings: enable private vulnerability reporting only. Do not change the description, topics, or branch protection. Source: the user's answer.
- Q4. Adapt CONTRIBUTING to this Rust/macOS project and keep choscordb's structure and wording wherever it applies. Source: inspection of the workspace.
- Q5. Add no issue templates, because choscordb has none. Source: inspection of choscordb.
- Q6. License: dual `MIT OR Apache-2.0`, using the Rust-convention files `LICENSE-MIT` and `LICENSE-APACHE`. This replaces the earlier "keep MIT" default. Source: the user ("add MIT OR Apache-2.0 dual license to the spec"). The file layout follows egui, Tauri, and unicode-segmentation.
- Q7. Relicensing consent: every commit in the history (50 commits on 2026-10-12) is by a single author, `ngthluu <ngthluu211@gmail.com>`, the maintainer. No other contributor's consent is needed. Source: `git log --format='%an <%ae>' | sort -u`.

### 1. `CONTRIBUTING.md` (new, repo root)

Mirror choscordb's section order and wording. Change only what the different stack requires.

1. **`# Contributing to choscorui`**. Copy the opening paragraph verbatim, with only the project name changed. Anyone may create an issue to report a bug or suggest a change. choscorui does not accept external pull requests. The maintainer implements changes and opens any pull requests the project needs.
2. **`## Maintainer pull request title and description`**. Copy verbatim, with two changes:
   - Use a choscorui scope in the example, e.g. `fix(macos): keep field focus after a theme switch`.
   - Keep the link to `.github/pull_request_template.md`. Change "Add screenshots for visible UI changes when useful" only if needed for accuracy.
3. **`## Before submitting a maintainer change`**. Replace the Python/LLVM/Qt content with this project's real prerequisites and commands:
   - Prerequisites: macOS 15+, the Xcode Command Line Tools, Rust stable with `rustfmt` and `clippy`, and Rust 1.88 for the MSRV check.
   - The fast checks are the commands from the `check` job in `ci.yml`:
     - `cargo fmt --all --check`, and the same check in `samples/notes`
     - `cargo clippy --workspace --all-targets --locked -- -D warnings`
     - `cargo +1.88 check --workspace --all-targets --locked`
     - `cargo test --workspace --locked`
     - `cargo clippy` and `cargo test` in `samples/notes`
   - For backend or UI changes, also run the native checks from the `native` job:
     - the `CHOSCORUI_RUN_MACOS_SMOKE=1` smoke tests
     - the gallery first-frame run
     - the stress autopilot
     - the notes autopilot
   - Say that native window checks must not run concurrently. This is already stated in `README.md`.
   - Point to the README's "Static analysis" section for rustquty, cargo-machete, and cargo-bylaw.
   - Say that the CI workflow and the documented commands are canonical.
4. **`## Source file size`**. This replaces "C++ file size". Explain the `rustquty.toml` limits: 3,000 physical lines per file and 700 per function. Explain that `quality/baseline.json` ratchets the current maxima, so the limits only get stricter. Keep choscordb's guidance verbatim in spirit:
   - Split by responsibility before reaching the limit.
   - Keep private details out of public APIs.
   - Prefer small single-purpose functions.
   - Moving code alone is not simplification.
   - Don't meet the limit by compressing formatting or removing useful comments.

   Also mention that `bylaw.toml` layering must hold (`cargo bylaw check`).
5. **`## Exceptions and suppressions`**. Adapt this section:
   - Fix warnings first.
   - Any `#[allow(...)]` or `#[expect(...)]` must name only the specific lint at the narrowest location and state why the code is correct. Blanket allowances, test retries, and timeout-only fixes are not accepted.
   - The `choscorui` facade forbids `unsafe`.
   - Hand-written `unsafe` FFI belongs in the platform crates. Each use documents its safety invariant, is recorded in `docs/FFI.md`, and has a focused test.

   Keep the paragraph about not weakening a test to hide a race verbatim.
6. **`## Scope of desktop evidence`**. Adapt this section:
   - Unit tests and the opt-in smoke and autopilot runs verify behavior on real AppKit windows.
   - They do not verify these, which remain manual release gates in `docs/RELEASING.md`:
     - IME composition with real input methods
     - VoiceOver walkthroughs
     - notarized installation on a clean machine
     - budgets on non-reference hardware

Every command and path named in CONTRIBUTING must exist and match `ci.yml` and `README.md` at implementation time.

### 2. `.github/pull_request_template.md` (new)

Copy choscordb's template verbatim, with one change. In the Verification comment, replace the offscreen-CTest sentence with: "Do not call unit, smoke, or autopilot runs an IME, VoiceOver, or notarized-install check."

### 3. `.github/dependabot.yml` (new)

Use the same structure as choscordb: weekly on Monday, timezone `Asia/Ho_Chi_Minh`, `open-pull-requests-limit: 5`, and one group per ecosystem with `patterns: ['*']`. Drop the pip entry, because the project has no Python. Define these entries:
- `cargo` at `/`, at 04:00, in group `cargo-weekly`
- `cargo` at `/samples/notes`, at 04:10, in group `samples-notes-weekly`. This project is outside the workspace (`exclude = ["samples"]`) and has its own lockfile.
- `github-actions` at `/`, at 04:20, in group `actions-weekly`

### 4. `.github/workflows/security.yml` (new)

Copy choscordb's `security.yml` verbatim. It has these parts:
- Triggers: `pull_request`, push to `main`/`master`, the weekly cron, and `workflow_dispatch`.
- `permissions: contents: read` and the concurrency group.
- A single `dependency-review` job that runs only on PRs. It uses `actions/checkout` and `actions/dependency-review-action`, both pinned by SHA, with `fail-on-severity: high`.

Do not modify the existing `ci.yml`. Pinning its actions by SHA is a non-goal.

### 5. `.gitignore` (edit)

Keep every existing entry. Append the choscordb categories that apply:
- `.DS_Store`
- the "Local configuration, credentials, logs" block (`.env`, `.env.*`, `*.[lL][oO][gG]`, `*.key`, `*.pem`, `*.p12`, `*.pfx`)
- a "Local screenshots" block with `*.[pP][nN][gG]`

Omit the Python, SQLite, Qt `*.user`, and choscordb-specific `!` exceptions. No file that is currently tracked may become ignored.

### 6. `README.md` (edit)

In the "Install, release, and security" paragraph, add one sentence linking to the policy, in the same style as choscordb's README: "Contribution policy is in [CONTRIBUTING.md](CONTRIBUTING.md); bugs and suggestions go in issues." Make no other README changes.

### 7. GitHub setting (outward-facing; the user authorized it)

Enable private vulnerability reporting with `gh api -X PUT repos/choscor/choscorui/private-vulnerability-reporting`. Do not change any other repository setting.

### 8. Dual license (MIT OR Apache-2.0)

Find every reference first: `grep -rnE 'LICENSE|license = |MIT' --exclude-dir=target --exclude-dir=.git .`. As of 2026-10-12 the references are the ones listed below.

- **License files.**
  - Rename `LICENSE` to `LICENSE-MIT` with `git mv`, so history is kept. Leave its text and the `Copyright (c) 2026 choscorui contributors` line unchanged.
  - Add `LICENSE-APACHE` containing the unmodified Apache License 2.0 text. Copy it from `https://www.apache.org/licenses/LICENSE-2.0.txt`, or from the local copy at `~/.cargo/registry/src/*/unicode-segmentation-1.13.3/LICENSE-APACHE`. Check that the copy is the full standard text, with no project-specific edits.
  - Do not keep a plain `LICENSE` file.
- **Cargo metadata.**
  - In the root `Cargo.toml` `[workspace.package]`, set `license = "MIT OR Apache-2.0"`. All member crates inherit it through `license.workspace = true`.
  - Set the same value in `samples/notes/Cargo.toml`.
  - `Cargo.lock` does not record licenses, so no lockfile change is expected.
- **`NOTICE`.** Keep it as it is. It lists third-party notices only. Under Apache-2.0 §4(d), redistributors must keep it, which is the intent.
- **README.** Add a final `## License` section in the standard Rust wording:

  > Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
  >
  > Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in choscorui by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

- **`CONTRIBUTING.md`.** Add a short `## License` section at the end. It says that changes are dual licensed under `MIT OR Apache-2.0`, as described in the README.
- **Docs that state "MIT".** Update each one to say `MIT OR Apache-2.0` and to point at `LICENSE-MIT` and `LICENSE-APACHE`:
  - `docs/LICENSES.md`, "Project license" paragraph.
  - `docs/RELEASING.md` line 7. Write "`LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE` are current…".
  - `docs/ROADMAP_STATUS.md` line 184, the license audit row. Change only the license wording and file links; keep the status Done.
- **Docs that should not change.** Leave alone every phrase that describes third-party components, such as "using the MIT option of each component" in LICENSES.md and NOTICE.
- **`CHANGELOG.md`.** Under **Unreleased → Highlights**, add: "**License.** Dual licensed under MIT OR Apache-2.0 (previously MIT only)." Under **Upgrading**, add one line saying that users may now choose either license.
- **Packaging.** `scripts/macos/package.sh` already copies `"$root"/LICENSE*`, so both new files ship automatically. Confirm this with the package and install check. Do not edit the script unless that check fails.
- **GitHub.** No settings change. GitHub detects the license from the files after the push.

### Non-goals

- Any license other than `MIT OR Apache-2.0`; changing the repository visibility, the description, topics, or branch protection.
- Adding issue templates, a code of conduct, or `CODEOWNERS`.
- Changing `ci.yml`.
- Creating a release.
- Rewriting history.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| CONTRIBUTING states the issue-only policy, using choscordb's wording | `CONTRIBUTING.md` opening paragraph, compared with `choscordb/CONTRIBUTING.md` |
| CONTRIBUTING has choscordb's section structure, adapted to Rust/macOS, with no Qt, C++, Python, or `quality.py` references | `grep -nE 'Qt\|CTest\|C\+\+\|quality.py\|LLVM\|cxx' CONTRIBUTING.md` returns nothing; the six sections are present |
| Every command in CONTRIBUTING is real | Each cargo command matches a step in `.github/workflows/ci.yml` or `README.md`. Running the fast checks locally passes. |
| PR template matches choscordb except for the evidence sentence | `diff` against `choscordb/.github/pull_request_template.md` shows only the Verification comment line |
| Dependabot covers the root cargo workspace, `samples/notes`, and actions | `.github/dependabot.yml` has the three entries. GitHub shows no config error under Insights → Dependency graph → Dependabot after push. |
| Workflows are valid | `actionlint .github/workflows/*.yml` passes, if actionlint is installed. Otherwise the Security workflow runs on the next push or PR. |
| `.gitignore` hides no tracked file | `git ls-files -ci --exclude-standard` prints nothing |
| README links to CONTRIBUTING | `grep -n 'CONTRIBUTING.md' README.md` |
| The SECURITY.md reporting channel works | `gh api repos/choscor/choscorui/private-vulnerability-reporting` returns `{"enabled":true}` |
| The project is dual licensed | `LICENSE-MIT` and `LICENSE-APACHE` exist and `LICENSE` does not. The Apache file matches the standard text: `diff` against the downloaded `LICENSE-2.0.txt` ignoring whitespace differences shows nothing. |
| Every crate reports the dual license | `cargo metadata --format-version 1 --no-deps \| python3 -c 'import json,sys; print({p["license"] for p in json.load(sys.stdin)["packages"]})'` prints `{'MIT OR Apache-2.0'}`. The same check in `samples/notes` gives the same output. |
| No stale project-license references | `grep -rnE 'LICENSE\b[^-]\|license = "MIT"\|under the MIT license\|\(MIT\)' --exclude-dir=target --exclude-dir=.git --exclude-dir=specs .` shows no reference to choscorui's own license as MIT-only. Third-party wording in NOTICE and LICENSES.md is expected to remain. |
| README and CONTRIBUTING state the dual license and the contribution clause | The `## License` sections in `README.md` and `CONTRIBUTING.md` |
| Release archives carry both licenses | `CHOSCORUI_UNIVERSAL=1 scripts/macos/package.sh`, then `unzip -l dist/choscorui-*-macos-universal.zip` lists `LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE`. `scripts/macos/install-check.sh` on that zip passes. |
| GitHub shows the new license after push | The repository page sidebar lists both licenses. Optional: `gh api repos/choscor/choscorui/license` may report `NOASSERTION` for multiple licenses, which is acceptable. |
| Nothing else regresses | `cargo fmt --all --check` and `cargo test --workspace --locked` pass |

## Constraints and risks

- The GitHub setting change is public-facing, and the user authorized only vulnerability reporting. If the `gh` call fails because the token lacks admin scope, report it, and give the user the manual path: Settings → Code security → Private vulnerability reporting → Enable.
- The new workflow and Dependabot take effect only once pushed. Commit with Conventional Commit subjects, e.g. `docs(contributing): ...` and `ci: ...`. Push only if the user asks.
- The SHA pins in `security.yml` are copied from choscordb. Dependabot's `github-actions` entry will keep them current.
- Dependency review needs the dependency graph, which is available on public repos by default. If the job errors with "dependency graph not enabled", enable it under Settings → Code security. That counts as the same security-reporting authorization only if the user confirms.
- Dependabot security updates and secret scanning are currently disabled in both repos. Matching choscordb means leaving them as they are.
- Relicensing: adding Apache-2.0 as an option is a license grant by the copyright holder. This is safe only because the history has a single author (Q7). If any commit by another author appears before implementation, stop and ask the user. The MIT grant on code released before the change stays in effect. The dual license applies from this change onward and, through the copyright holder's grant, to existing code.
- `LICENSE-APACHE` must not be modified. A stray edit stops GitHub and cargo-deny-style tools from recognizing it.
- Adding the PNG ignore means future doc screenshots need a `!` exception. This is acceptable because no PNGs are tracked today.

Read this whole spec and inspect the current workspace before implementing.
