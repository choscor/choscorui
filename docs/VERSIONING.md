# Versioning, deprecation, and error policy

## Versions

choscorui follows [Semantic Versioning](https://semver.org/) for the `choscorui` facade crate. Every workspace crate shares one version number.

- **Before 1.0 (`0.y.z`).** A breaking change bumps `y`, and `z` releases are compatible. The current series is `0.0.z`. The API is experimental, and any release may break it. The release notes list every break.
- **From 1.0.** Breaking changes need a major release. Additions, such as new types, methods, enum variants on `#[non_exhaustive]` types, and new capability fields, ship in minor releases. Patch releases only fix bugs.
- **What is stable.** The `choscorui` facade, including `prelude`, `core`, `controls`, `compose`, `platform`, and `capabilities()`, is the stable surface. The backend crates (`choscorui-macos`, `-windows`, `-linux`) and `choscorui-render` are implementation details. Their own versions may change without notice. Reach them through `choscorui::platform`.
- **MSRV.** The minimum Rust version is part of the contract: 1.88, with edition 2024. Raising it is a minor-version change before 1.0, and a minor change announced one release ahead after 1.0.
- **Not covered.** Behavior that is not part of the API is not covered: pixel-exact rendering, `Debug` output, the wording of error messages, and timing. The [compatibility suite](#compatibility-suite) pins the behavior that is covered.

## Deprecation

1. Mark the old item `#[deprecated(since = "x.y.z", note = "use … instead")]`. Ship the replacement in the same release.
2. Keep a deprecated item for at least one minor release before 1.0, and until the next major release after 1.0.
3. List deprecations and their replacements in the release notes under **Deprecated**.

The flat re-exports at the root of `choscorui` (for example `choscorui::Button`) predate the modules. They stay for compatibility, but new code should use `choscorui::prelude` or the modules. Any later removal will follow this process.

## Feature flags

The crates define no Cargo features today: everything listed in [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) is always built. Future features must be additive. Enabling one must never remove or change an API. Features must not gate safety or accessibility behavior. Each feature must be listed here with its purpose and default.

## Backend capability queries

`choscorui::capabilities()` returns `BackendCapabilities`, with one `Support` value per optional feature:
- `Supported`
- `Unsupported`
- `Conditional(reason)`: available once a stated precondition holds.

Query capabilities instead of the OS version or `cfg!(target_os)`. For example, on macOS `notifications` is `Conditional("run from an app bundle with a bundle identifier")` until the binary runs from a bundle. The struct is `#[non_exhaustive]`, so new capabilities are not breaking changes. Construct one with `BackendCapabilities::none` in tests.

| Capability | macOS |
| --- | --- |
| multiple windows, native menus, context menus, native dialogs, clipboard, drag and drop, input methods, accessibility, status items, open URLs, system appearance, text shaping, image codecs | Supported |
| notifications | Conditional: needs an app bundle with a bundle identifier |
| GPU painting | Unsupported (Core Graphics on the CPU) |

## Error reporting

- **No panics on bad input.** A fallible operation returns `Result<T, E>`, where `E` is a domain enum such as `TreeError`, `SettingsError`, `DockError`, or `choscorui::platform::Error`. It does not panic on invalid input. Panics are reserved for broken internal invariants.
- **Error traits.** Every public error type implements `Debug`, `Display`, and `std::error::Error`, and is `Send + Sync + 'static`. Errors that wrap another error, such as `CommandSyncError::Tree` or `ContainerError::Tree`, return it from `source()`. `crates/choscorui/tests/errors_and_capabilities.rs` checks this.
- **Messages.** A `Display` message is a lowercase sentence fragment without a trailing period. It names the violated rule ("the selection is outside the text"), so it composes with context such as `"save failed: {error}"`.
- **Recovery reports.** Recoverable conditions are reported as values, not errors. For example, `Settings::load_or_recover` returns the problems it repaired, and dock restore reports unknown keys.
- **Callbacks.** A panic in a posted task, timer, or menu callback is caught at the FFI boundary and counted (`posted_panic_count`, `timer_panic_count`, `callback_panic_count`). It never unwinds into AppKit.

## Compatibility suite

`crates/choscorui/tests/compatibility.rs` records focus order, layout geometry, accessibility semantics, theme tokens, and display-list output for fixed scenes. It compares them against the golden files in `crates/choscorui/tests/golden/`. A change that alters any of these fails the suite.

If the change is intended, regenerate the files with `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility`. Review the diff, and record the behavior change in the release notes.
