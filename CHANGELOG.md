# Changelog

All notable changes to choscorui. The versioning policy is in [docs/VERSIONING.md](docs/VERSIONING.md). The per-commit change list for a release comes from `scripts/release-notes.sh <previous-tag> <tag>`. Paste its output under the release heading, after the hand-written highlights.

## Unreleased (first public release candidate, 0.1.0)

### Highlights

- **Safe facade.** Applications depend on `choscorui` alone. It forbids `unsafe`, exposes no native handles, and groups the API into `prelude`, `core`, `controls`, `compose`, and `platform`, plus `capabilities()`.
- **macOS backend.** Built on hand-written AppKit, Core Graphics, and Core Text FFI, with these features:
  - Multiple windows, native menus, context menus, and dialogs.
  - Clipboard, drag and drop, and input methods.
  - VoiceOver accessibility, status items, notifications (in app bundles), and opening URLs.
  - System appearance and accessibility preferences.
- **Controls.** The basic widgets, validated text, number, and date inputs, and virtualized list, tree, table, and grid views with sort and filter proxies. Also combo boxes, popups, tooltips, dialogs, toolbars, status bars, dockable panels, an interactive canvas, rich text, and custom controls without FFI.
- **Theming.** Design tokens, scoped and instance overrides, swappable visual recipes, animation that respects Reduce Motion, and runtime light/dark switching.
- **Localization.** Catalogs with plurals, locale number and date formatting, runtime language switching, right-to-left mirroring, translation validation, and packaged resources.
- **Reliability.** Measured release budgets (`choscorui-bench --check`), seeded property tests, native leak and lifetime checks, a golden compatibility suite, and an external sample app (`samples/notes`).
- **Packaging.** Signed app bundles, optional notarization, install and uninstall scripts, and an install gate (`scripts/macos/install-check.sh`).

### Fixed during release hardening

- Windows stayed alive after closing when they were open during a main-menu replacement (a missing autorelease pool).
- An app launched from a `.app` bundle failed to start with `AppKitUnavailable`.
- On Intel Macs, every app crashed on its first accessibility update: an `NSRect` return used the wrong `objc_msgSend` variant.
- `NumberInput` mis-parsed `.` in locales that group with `,`. Markdown serialization was ambiguous for adjacent styles. Code-span brackets ended links early.

### Known limitations

- **Intel:** the x86-64 build passes every native check under Rosetta 2, but has not yet run on real Intel hardware (see [docs/FFI.md](docs/FFI.md)).
- **Platform and distribution:**
  - **macOS 15+ only.** The Windows and Linux crates compile but have no backend.
  - **Not on crates.io.** Depend on the repository by path.
- **Rendering and graphics:**
  - **CPU only.** Painting uses Core Graphics on the CPU. There is no GPU path, so device loss does not apply.
  - **Not in 1.0 (deferred):** printing, charts, MDI subwindows, plugins, OpenGL and shaders, and extra image formats. See [docs/SUPPORT_MATRIX.md](docs/SUPPORT_MATRIX.md).
- **Locale:** the toolkit does not read the system's preferred languages. Apps choose the locale.
- **Notifications:** need an app bundle with a bundle identifier (`capabilities().notifications` is `Conditional`).
- **Controls:** some behaviors differ between controls, for example Space activation timing and wrapping. They are listed in [docs/CONTROLS.md](docs/CONTROLS.md#known-inconsistencies-to-resolve-before-10) and may change before 1.0.
- **Manual checks:**
  - IME composition with real input methods.
  - VoiceOver walkthroughs.
  - Notarized installation on a clean machine.
  - Budgets on hardware other than the reference M4.

  See [docs/RELEASING.md](docs/RELEASING.md).

### Upgrading

This is the first release, so there is nothing to migrate. For later releases:
- Breaking changes are listed under **Breaking changes**, and deprecations under **Deprecated**, each with its replacement.
- If a golden file changes in `crates/choscorui/tests/golden/`, the notes describe the behavior change.
- New code should use `choscorui::prelude` and the facade modules rather than the flat root re-exports, which are kept only for compatibility.
