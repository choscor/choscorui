# choscorui

An experimental Rust desktop UI toolkit using the standard library and system SDK FFI. The [roadmap](docs/ROADMAP.md) describes the full target; the current prototype is macOS-only and draws a basic custom control tree.

Applications depend on the experimental `choscorui` facade. It provides the safe retained-tree, control, composition, and macOS lifecycle APIs without exposing native handles. The lower-level workspace crates are implementation details while this API is validated.

## Run the macOS window prototype

```sh
cargo run -p choscorui-gallery
```

Click **Switch theme** to alternate between the dark plain-button theme and the light leading-rail theme; Enter or Space activates it after focus. The single-line field accepts ordinary and IME-composed text and supports selection, deletion, word navigation, Command-A/C/X/V/Z/Shift-Z, and Enter commit. Editing gives it an intermediate indicator; Enter accepts nonempty text or shows an invalid indicator for empty text. Drag across the multilingual document to select text; Arrow, Option-Arrow, Home, and End move its caret, Shift extends selection, and Up/Down preserve the horizontal goal while scrolling into view. The checkbox shows or hides selection. Control-Enter activates the theme command anywhere. Tab and Shift-Tab move focus. Scroll the rows or document with a wheel or trackpad. Resizing rewraps the document. Close the window to exit. For an unattended launch and first-frame check:

```sh
CHOSCORUI_GALLERY_AUTOCLOSE_MS=200 cargo run -p choscorui-gallery
```

The opt-in native smoke test creates real AppKit windows:

```sh
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test window_smoke
```

Run the native smoke test and gallery separately. Both create foreground AppKit windows, so concurrent runs can change focus and make input assertions unreliable.

Core tests run on the workspace with `cargo test --workspace`; formatting and lint checks use `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`. See the [initial contract](docs/CONTRACT.md), [capability matrix](docs/SUPPORT_MATRIX.md), [FFI safety record](docs/FFI.md), [theming guide](docs/THEMING.md), and [accessibility contract](docs/ACCESSIBILITY.md), [dialogs](docs/DIALOGS.md) for scope and current limits.

## Static analysis

Install the pinned tools:

```sh
rustup component add clippy
cargo install rustquty --version 0.4.3 --locked
cargo install cargo-machete --version 0.9.2 --locked
cargo install cargo-bylaw --version 0.1.0 --locked
```

`cargo-bylaw` 0.1.0 requires Rust 1.95 or newer, so install and run these tools
with a current toolchain even though the library workspace supports Rust 1.85.

Run the checks from the workspace root:

```sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
rustquty qa --profile full --verbose \
  --disable-collector tests \
  --disable-collector coverage \
  --disable-collector deny \
  --disable-collector audit \
  --disable-collector hack \
  --disable-collector mutants \
  --disable-collector duplicates \
  --disable-collector complexity
cargo machete --with-metadata
cargo bylaw check
```

`rustquty.toml` hard-limits source files to 3,000 lines and functions to 700
lines. The committed baseline ratchets the current maxima of 2,745 and 663
lines, so either limit becomes stricter as those prototype files are split.
`bylaw.toml` keeps `choscorui-core` at the base of the workspace, permits the
composition, controls, render, and platform crates to depend only on core, prevents libraries
from depending on the gallery composition root, and rejects crate cycles.
