# License audit

Checked 2026-10-08 against `Cargo.lock` and the macOS link line. Re-run this audit for every dependency-rule revision and before each release.

## Rust crates in the build graph

| Crate | Version | License | Used by | Notes |
| --- | --- | --- | --- | --- |
| `unicode-segmentation` | 1.13.3 | MIT OR Apache-2.0 | `choscorui-core` | Only third-party crate; no transitive dependencies. Dependency rule revision 1. |

Workspace crates are first-party. `cargo tree --workspace -e normal --prefix none | sort -u` must list only workspace crates and the rows above.

## Linked system frameworks (macOS)

AppKit, Foundation, CoreFoundation, CoreGraphics, CoreText, ImageIO, UniformTypeIdentifiers, ApplicationServices, and the Objective-C runtime are linked dynamically from the installed macOS SDK and ship with the operating system. They are governed by the Xcode and macOS SDK license agreements; choscorui does not redistribute them.

## Fonts and bundled assets

The toolkit bundles no fonts; text uses installed system fonts through Core Text. Gallery icons and images are generated in source code or authored for this repository and are covered by the workspace license.
