# choscorui

choscorui is a desktop UI toolkit for Rust. It uses only the standard library, hand-written system SDK FFI, and one audited crate. Applications depend on the safe `choscorui` facade, which has no `unsafe` code and exposes no native handles.

## Requirements

macOS is the supported platform. You need:

- macOS 15 or later, on Apple silicon or Intel
- the Xcode Command Line Tools: `xcode-select --install`
- Rust 1.88 or later: `rustup toolchain install 1.88`

There are no other dependencies.

## Run the examples

- Run `cargo run -p choscorui-gallery` to see the basic controls, text input, and theme switching.
- Run `cargo run -p choscorui-stress` to see a large list, a large canvas, and docking panels.
- Run `cd samples/notes && cargo run` to see a small application with many windows. The [notes sample](samples/notes/README.md) uses only the facade.

## Documentation

Start with the [documentation index](docs/README.md).

## Install, release, and security

- The demo apps ship as a zip archive with `install.sh` and `uninstall.sh`. `scripts/macos/package.sh` builds the archive, and its [README](scripts/macos/archive-readme.md) gives the install steps.
- [docs/releasing.md](docs/releasing.md) describes the release procedure. Each GitHub Release contains the release notes.
- [SECURITY.md](SECURITY.md) describes how to report a vulnerability.
- [CONTRIBUTING.md](CONTRIBUTING.md) describes the contribution policy and the checks. Report bugs and suggestions in issues.

## License

<!-- check-docs: no-sentence-check (standard dual-license text, kept verbatim) -->

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in choscorui by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
