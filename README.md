# ChoscorUI

ChoscorUI is a desktop UI toolkit for Rust. It uses only the standard library, hand-written system SDK FFI, and one audited crate. Applications depend on the safe `choscorui` facade, which has no `unsafe` code and exposes no native handles.

## Requirements

macOS and Linux are the supported platforms. The application API is portable: it compiles on every target, and on a target without a backend `App::run` returns `AppError::Unsupported`.

On macOS you need:

- macOS 15 or later, on Apple silicon or Intel
- the Xcode Command Line Tools: `xcode-select --install`
- Rust 1.88 or later: `rustup toolchain install 1.88`

On Linux you need:

- a Wayland compositor or an X11 server
- Rust 1.88 or later: `rustup toolchain install 1.88`
- the system libraries that [platform-linux.md](docs/platform-linux.md) lists. The backend loads them at run time, so a build needs no system library packages.

There are no other dependencies.

## Run the gallery

The gallery shows each public control on its own page. It uses only the `choscorui` facade.

- Run `cargo run -p choscorui-gallery` to open the gallery on the last page.
- Run `cargo run -p choscorui-gallery -- slider` to open one page.
- Run `cargo run -p choscorui-gallery -- --list` to print each page id.
- Run `CHOSCORUI_GALLERY_SETTINGS=/tmp/gallery.txt CHOSCORUI_GALLERY_AUTOPILOT=1 cargo run -p choscorui-gallery` to run the scripted checks. The private settings file keeps your own settings.

## Documentation

Start with the [documentation index](docs/README.md).

## Install, release, and security

- Add ChoscorUI to an application with `cargo add choscorui`. Each release publishes the crates to crates.io.
- [docs/releasing.md](docs/releasing.md) describes the release procedure. Each GitHub Release contains the release notes.
- [SECURITY.md](SECURITY.md) describes how to report a vulnerability.
- [CONTRIBUTING.md](CONTRIBUTING.md) describes the contribution policy and the checks. Report bugs and suggestions in issues.

## License

<!-- check-docs: no-sentence-check (standard dual-license text, kept verbatim) -->

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in ChoscorUI by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
