# Contributing to ChoscorUI

Anyone can create an issue to report a bug or to suggest a change. ChoscorUI does not accept external pull requests. The maintainer makes the changes and opens the pull requests for the project.

## Maintainer pull request title and description

Keep each change small, so that you can explain and verify it alone. Include behavior tests, and do not include cleanup that is not related. The project has one maintainer, so an approving review is not necessary. The automated checks are the required review.

Give the pull request a specific title that starts with the action. Use the Conventional Commits format `<type>(<scope>): <imperative summary>`, for example `fix(macos): keep field focus after a theme switch`. The title must be clear in a list of pull requests or in the release history.

The [PR template](.github/pull_request_template.md) asks for the reason, the important changes, the verification, and the context that the diff does not show. Use these rules for the description:

- Write the exact checks that you ran and their results. Name each relevant check that you did not run, and give the reason.
- Add screenshots for visible UI changes when they help the reviewer.
- Describe effects on compatibility and the migration steps.
- Link the related issue. Write `Closes #N` only when the pull request fully solves the issue.
- Remove the template sections that you do not use.
- Update the description when the change is different after review.

## Before submitting a maintainer change

Install these tools:

- macOS 15 or later
- the Xcode Command Line Tools: `xcode-select --install`. `cargo test` uses their `xcrun clang++` to compile the SDK header check of `choscorui-macos`.
- Rust stable with `rustfmt` and `clippy`
- Rust 1.88 for the minimum version check: `rustup toolchain install 1.88`
- the nightly toolchain that `tools/api-docs/nightly.txt` names, for the [API reference](#api-reference). Install it with `rustup toolchain install <nightly> --profile minimal --target aarch64-apple-darwin`.
- the Windows and Linux targets for the portable app check: `rustup target add x86_64-pc-windows-gnu x86_64-unknown-linux-gnu`

Run the fast checks from the CI `check` job. The macOS jobs leave out the Windows and Linux backend crates. The [Linux jobs](#linux-checks) check the Linux backend:

```sh
scripts/check-docs.sh
cargo run -p choscorui-api-docs --locked -- --check
cargo fmt --all --check
cargo clippy --workspace --exclude choscorui-windows --exclude choscorui-linux --all-targets --locked -- -D warnings
cargo +1.88 check --workspace --exclude choscorui-windows --exclude choscorui-linux --all-targets --locked
cargo test --workspace --exclude choscorui-windows --exclude choscorui-linux --locked
cargo check -p choscorui -p choscorui-gallery --all-targets --locked --target x86_64-pc-windows-gnu
cargo check -p choscorui -p choscorui-gallery --all-targets --locked --target x86_64-unknown-linux-gnu
cargo check -p choscorui-linux --all-targets --locked --target x86_64-unknown-linux-gnu
cargo publish --workspace --dry-run --locked
```

The `--target` commands check that the facade and the gallery build on a target without a backend. They only compile, so they need no other SDK. The dry run checks that the published crates package and build from their own files.

For backend or UI changes, also run the native checks from the CI `native` job.

> **Caution:** The native checks open foreground AppKit windows. Run them one at a time. Runs at the same time can change the focus and make input tests fail.

The native checks run with `NSZombieEnabled=YES` and `MallocScribble=1`. Then a message to a released object crashes with a zombie report, and freed memory is scribbled. The lifetime check also sets `NSDeallocateZombies=YES`, because its footprint check needs freed memory back.

```sh
export NSZombieEnabled=YES MallocScribble=1
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test window_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test menu_smoke --locked
NSDeallocateZombies=YES CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test lifetime_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui --test app_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui --test metal_parity_smoke --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 CHOSCORUI_RUN_MACOS_CLIPBOARD=1 cargo test -p choscorui-macos --test clipboard_smoke --locked
CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 cargo run -p choscorui-gallery --locked
CHOSCORUI_GALLERY_SETTINGS=/tmp/gallery.txt CHOSCORUI_GALLERY_AUTOPILOT=1 cargo run -p choscorui-gallery --locked
```

### Linux checks

For changes to `choscorui-linux`, to the facade, or to the controls, also run the CI `linux-check` and `linux-native` jobs. They run in the image of [scripts/linux/Dockerfile](scripts/linux/Dockerfile), which is Ubuntu 22.04 with the Rust toolchains and the system packages.

On a Mac, use Docker Desktop. On Apple silicon, the container is aarch64 Linux.

1. Start Docker Desktop.
2. Run the fast checks: `scripts/linux/run-in-container.sh check`.
3. Run the native checks: `scripts/linux/run-in-container.sh native`.

The native checks run each test alone in [scripts/linux/headless-session.sh](scripts/linux/headless-session.sh). That script starts a D-Bus session bus, the AT-SPI registry, a headless sway compositor, and Xvfb. It also builds and starts a virtual keyboard and pointer for sway, from [scripts/linux/virtual-input](scripts/linux/virtual-input/virtual-input.c). A test writes commands to the FIFO in `CHOSCORUI_VIRTUAL_INPUT` to press keys and buttons. The checks run on Wayland and on X11, with the GLES painter and with the Cairo painter.

To run one command in a headless session, use `scripts/linux/run-in-container.sh -- COMMAND`. For example:

```sh
scripts/linux/run-in-container.sh -- env CHOSCORUI_RUN_LINUX_SMOKE=1 CHOSCORUI_BACKEND=x11 cargo test -p choscorui-linux --test window_smoke
```

On a Linux machine, run [scripts/linux/jobs.sh](scripts/linux/jobs.sh) `check` and `native` from the repository root. The binding check compiles C code against the system headers, so it needs these Debian or Ubuntu packages:

- `build-essential` and `pkg-config`
- `libwayland-dev`, `libxkbcommon-dev`, and `libxkbcommon-x11-dev`
- `libxcb1-dev`, `libxcb-xkb-dev`, `libxcb-xinput-dev`, `libxcb-shm0-dev`, `libxcb-randr0-dev`, `libxcb-render0-dev`, `libxcb-present-dev`, and `libxcb-cursor-dev`
- `libx11-dev` and `libx11-xcb-dev`
- `libegl-dev` and `libgles-dev`
- `libcairo2-dev`, `libfreetype-dev`, `libharfbuzz-dev`, and `libfontconfig-dev`
- `libdbus-1-dev`
- `libpng-dev`, `libturbojpeg0-dev`, `libgif-dev`, `libtiff-dev`, `libwebp-dev`, `libheif-dev`, and `libcanberra-dev`
- `wayland-protocols` 1.38 or later, and the `wayland.xml` of Wayland 1.23 or later

The native checks also need `sway`, `xvfb`, `dbus`, `at-spi2-core`, and Mesa. A missing package makes the binding check fail with the package name.

For changes to paint, layout, text, the display list, or the run loop, also run the benchmark check in [docs/performance.md](docs/performance.md).

### Metal shaders

The Metal painter loads the precompiled library `crates/choscorui-macos/src/shaders/ui.metallib`, so a build does not need the Metal toolchain. You need the toolchain only to change `ui.metal`. To change the shaders, do these steps:

1. Install the Metal toolchain: `xcodebuild -downloadComponent MetalToolchain`.
2. Edit `crates/choscorui-macos/src/shaders/ui.metal`.
3. Run `scripts/macos/build-shaders.sh`. It writes `ui.metallib` and the stamp file `ui.metallib.stamp`.
4. Commit the source, the library, and the stamp together.

The test `shader_library_matches_source` of `choscorui-macos` fails when the source or the library does not match the stamp.

The [CI workflow](.github/workflows/ci.yml) and the commands in this document are the reference.

## API reference

`api-docs/` is the reference for application authors. The `choscorui-api-docs` tool generates it from the doc comments, so do not edit it by hand. The tool reads the rustdoc JSON of the pinned nightly toolchain. The JSON format is not stable, so the tool accepts only one `format_version`.

After a change to the public API or to a doc comment, do these steps:

1. Run `cargo run -p choscorui-api-docs --locked`.
2. If the tool names an item with no topic, add a line for it to `tools/api-docs/topics.txt`.
3. If the tool names an item with no summary or no example, add it to the doc comment.
4. Commit the changes in `api-docs/` with the code change.

The tool requires a doc summary on each item. It also requires an `# Examples` section with a Rust code block on these items:

- each view type
- each `ViewExt` method
- each public method of `App`, `AppContext`, and `Scene`

The examples are doctests in the crate that defines the item. Start an example that opens a window with `no_run`.

The `--check` option writes nothing. It exits 1 and lists each stale, missing, or extra file. The tool exits 2 when the toolchain or rustdoc fails.

To change the nightly toolchain, do these steps:

1. Write the new toolchain name in `tools/api-docs/nightly.txt`.
2. Install the toolchain.
3. Set `FORMAT_VERSION` in `tools/api-docs/src/model.rs` to the version that the tool reports.
4. Run `tools/api-docs/tests/fixture/generate.sh`.
5. Run `cargo test -p choscorui-api-docs`.
6. Run `cargo run -p choscorui-api-docs --locked`, and examine the changes in `api-docs/`.

## Static analysis

Install the tools at their pinned versions:

```sh
rustup component add clippy
cargo install rustquty --version 0.4.3 --locked
cargo install cargo-machete --version 0.9.2 --locked
cargo install cargo-bylaw --version 0.1.0 --locked
```

`cargo-bylaw` 0.1.0 needs Rust 1.95 or later. Install and run these tools with a current toolchain. The library workspace still supports Rust 1.88.

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

`bylaw.toml` keeps `choscorui-core` at the base of the workspace. The controls, composition, render, and platform crates can depend only on core. No library can depend on the gallery, and the crates cannot have cycles. [docs/architecture.md](docs/architecture.md) describes the layers.

## Adding a control

The gallery shows each public name of `choscorui-controls`. A test fails when a name is not in the gallery.

1. Add a view function to the category file in `gallery/src/ui/pages/`. Return it from the `page` function of that file.
2. Add the page to `PAGES` in `gallery/src/catalog.rs`.
3. Add each new export name to `SHOWN`, or to `EXEMPT` with a reason.
4. Show each variant and size of the control on its page. Also show its invalid, checked, and open states when it has them. Add the control to `expectations` in `gallery/src/ui/pages/variants.rs`.
5. Run `cargo test -p choscorui-gallery`.

The coverage test names each export that is missing or stale. The headless page test builds each page with and without the enabled state. The variant test names each variant, size, or state that a page does not paint.

## Source file size

`rustquty.toml` limits a Rust source file to 3,000 lines and a function to 700 lines. `quality/baseline.json` records the current maximum values as a ratchet. Thus, a limit can only become stricter when you split a large file. Run the `rustquty qa` command above to check the limits.

Use these rules for code size:

- Split a file by responsibility before it reaches the limit. Keep related operations together.
- Keep private implementation details out of public APIs.
- Write small functions that each do one logical task. A move of a large function to a different file does not make it simpler.
- Do not compress the formatting or remove useful comments to meet the limit.
- Keep the crate layers in `bylaw.toml`. Run `cargo bylaw check` when a new module or dependency crosses crates.

## Exceptions and suppressions

Fix first-party warnings and lint findings. An `#[allow(...)]` or `#[expect(...)]` names one specific lint at the narrowest location. Write the reason why the code is correct next to it. Do not add blanket lint allowances, test retries, or fixes that only increase a timeout.

The `choscorui` facade forbids `unsafe`. Hand-written `unsafe` FFI goes in the platform crates. Each use documents its safety invariant, has an entry in [docs/ffi.md](docs/ffi.md), and has a focused test.

In `choscorui-macos`, `unsafe` code is only in `src/ffi/`. The crate root denies `unsafe_code`, and the one `#[allow(unsafe_code)]` is on `mod ffi`. The `[lints]` table in its `Cargo.toml` also denies undocumented `unsafe` blocks and blocks with more than one `unsafe` operation.

The native smoke tests are the exception: the public API has no seam to read AppKit state. Each `unsafe` item in a smoke test allows `unsafe_code` by itself, with its reason.

Do not make a test weaker to hide a race. Required tests run one time. If you must remove a check for a short time, write the exact reason and the tracking issue.

## Documentation

Update the documents in [docs/](docs/README.md) in the same change as the code. Write them in ASD-STE100 Simplified Technical English, and draw diagrams in Mermaid. `scripts/check-docs.sh` checks links, banned terms, and the sentence length. It also fails when a tracked file outside `docs/specs/` names the reference design system.

Use these rules:

- Describe only the current code. Do not write history, phases, future plans, comparisons with other toolkits, or counts and dates that drift. Put dated decision records in `docs/specs/`.
- Write procedures as numbered steps with one imperative instruction in each step.
- Use at most 20 words in a procedural sentence and 25 words in a descriptive sentence. A code span counts as one word.
- Use the active voice and simple tenses. Use at most 6 sentences in a paragraph.
- Use approved words in their approved meaning. Keep technical names unchanged, and use one word for one thing.
- Do not use phrasal verbs or slang. Use noun clusters of at most three nouns.
- Put a warning before the step that it applies to.
- Draw diagrams in `mermaid` fences with `flowchart`, `sequenceDiagram`, or `stateDiagram-v2`. Do not use images or ASCII-art diagrams.
- Name document files in lowercase kebab-case. `docs/README.md` is the exception.
- Keep the control catalog in `docs/controls.md` complete.

## Scope of desktop evidence

The unit tests and the smoke and autopilot runs check behavior on real AppKit windows, and on headless sway and Xvfb. They do not verify these items:

- text input with real input methods (IBus, Fcitx, and the macOS input methods)
- VoiceOver and Orca
- real GPU drivers on Linux
- GNOME (mutter) and KDE (KWin) behavior, including client-side decorations under mutter
- HiDPI and fractional scaling on real monitors
- a notarized installation on a clean machine
- Intel hardware
- budgets on hardware other than the reference machine

Do not report these items as verified from automated evidence.

## License

ChoscorUI has a dual license, `MIT OR Apache-2.0`, as the [README](README.md#license) describes. Changes to the project use the same licenses.
