# Versioning, deprecation, and error policy

## Versions

ChoscorUI follows [Semantic Versioning](https://semver.org/) for the `choscorui` facade crate. All workspace crates use one version number.

- **Before 1.0 (`0.y.z`).** A breaking change increments `y`. A `z` release is compatible. The API is experimental, and the release notes list each breaking change.
- **From 1.0.** A breaking change needs a major release. Additions go in minor releases. Examples of additions are new types, new methods, new enum variants on `#[non_exhaustive]` types, and new capability fields. A patch release only fixes bugs.
- **Stable surface.** The stable surface is the `choscorui` facade: `prelude`, `core`, `controls`, `compose`, and `capabilities()`. `App`, `Scene`, `AppContext`, `WindowId`, `AppError`, `WindowOptions`, and the services are in the prelude on every target. The backend crates and `choscorui-render` are implementation details, and their versions can change without notice.
- **Platform escape hatch.** `choscorui::platform` is the backend of the target. On macOS it is `choscorui-macos` with the `NativeAccess` trait, and on other targets it is empty. It follows the version of the backend crate, so a minor release can change it. Code that uses it is not portable, so put it behind your own check of the target.
- **MSRV.** The minimum Rust version is part of the contract: 1.88, with edition 2024. Before 1.0, an increase is a minor-version change. After 1.0, an increase is a minor change that the previous release announces.
- **Not covered.** Pixel-exact rendering, `Debug` output, the text of error messages, and timing are not part of the API. The [compatibility suite](#compatibility-suite) pins the behavior that is part of the API.

## Deprecation

1. Mark the old item with `#[deprecated(since = "x.y.z", note = "use … instead")]`.
2. Release the replacement in the same release.
3. Keep the deprecated item for one minor release or more before 1.0. After 1.0, keep it until the next major release.
4. List each deprecation and its replacement under **Deprecated** in the release notes.

The root of `choscorui` also re-exports many types directly, for example `choscorui::Button`. These re-exports stay for compatibility. Use `choscorui::prelude` or the modules in new code. A removal of a root re-export follows the deprecation steps.

## Feature flags

The crates define no Cargo features, so all code is always built. A new feature must only add API. A feature must not remove or change an API, and it must not control safety or accessibility behavior. List each feature in this section with its purpose and its default.

## Backend capability queries

`choscorui::capabilities()` returns `BackendCapabilities`. It has one `Support` value for each optional feature:

- `Supported`
- `Unsupported`
- `Conditional(reason)`: the feature is available when the stated condition is true.

Query capabilities, not the OS version or `cfg!(target_os)`. For example, on macOS, `notifications` is `Conditional` until the binary runs from an app bundle. `BackendCapabilities` is `#[non_exhaustive]`, so a new capability is not a breaking change. In tests, construct one with `BackendCapabilities::none`.

[platform-macos.md](platform-macos.md) lists the values on macOS.

On a target without a backend, `App::start` and `App::run` return `AppError::Unsupported`. The message is "no backend exists for this operating system". A service of `Services` returns `ServiceError::Unsupported` when the backend does not have it, and `ServiceError::Unavailable` with the condition when the capability is `Conditional`.

## Error reporting

- **No panics on bad input.** A fallible operation returns `Result<T, E>`. `E` is a domain enum, for example `TreeError`, `SettingsError`, `DockError`, or `choscorui::platform::Error`. A panic occurs only when the code breaks an internal invariant.
- **Error traits.** Each public error type implements `Debug`, `Display`, and `std::error::Error`, and is `Send + Sync + 'static`. An error that wraps another error returns it from `source()`, for example `CommandSyncError::Tree`. [`errors_and_capabilities.rs`](../crates/choscorui/tests/errors_and_capabilities.rs) checks these rules.
- **Messages.** A `Display` message is a lowercase sentence fragment without a period at the end. It names the rule that failed, for example "the selection is outside the text". Thus, you can add context, as in `"save failed: {error}"`.
- **Recovery reports.** A condition that the toolkit can repair is a value, not an error. For example, `Settings::load_or_recover` returns the problems that it repaired, and a dock restore reports unknown keys.
- **Callbacks.** A panic in a posted task, a timer, or a menu callback stops at the FFI boundary. `posted_panic_count`, `timer_panic_count`, and `callback_panic_count` count these panics.

## Compatibility suite

[`compatibility.rs`](../crates/choscorui/tests/compatibility.rs) records fixed scenes. For each scene, it records focus order, layout geometry, accessibility semantics, theme tokens, and display-list output. It compares the records with the golden files in `crates/choscorui/tests/golden/`. A change to any of these values makes the suite fail. It also records every hosted control from the [catalog](../crates/choscorui-controls/tests/catalog/mod.rs) and each variant. For each control, it records the bounds, semantics, and display list, in both directions. It also records the Tab order through all hosted controls.

`control_semantics.txt` and `focus_order.txt` start with fixed scenes of a few controls. Then they record every hosted control: the catalog and the built-in controls in [builtins.rs](../crates/choscorui-controls/tests/catalog/builtins.rs). For each control, `control_semantics.txt` records the role, name, value, state, and actions, in both directions. `focus_order.txt` records the forward and backward Tab order through all of them, in both directions. At the end, `control_semantics.txt` records a button with a tooltip, in both directions. It records the description of the button, and the `Tooltip` node with its `DescribedBy` relation while the tooltip shows.

`views.txt` records views that the compose `ViewHost` mounts without a window, with `EstimatedText`. For `padding` then `background`, and for the reverse order, it records the bounds of each node and the display list. It also records the bounds of each node of a stack and of a `Form`, under the `en` and `ar` locales.

The `painted_<group>.txt` files pin the default recipe of each `ControlKind`, with one file for each recipe group. For each kind, variant, size, and slot, the general slot painter paints a box, an icon, and a label. The file records the `rest` paint, and each key state that paints differently. A list names the states that paint the same as `rest`. A size shows only if it paints differently from `default`. Slots with equal styles in all cases share one record.

To accept an intended change, do these steps:

1. Run `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility`.
2. Review the diff of the golden files.
3. Describe the behavior change in the commit body and the pull request, so that it goes into the release notes.
