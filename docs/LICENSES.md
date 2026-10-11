# License audit

Last checked on 2026-10-08 against `Cargo.lock` (workspace and `samples/notes`), the release link line (`otool -L` on the packaged binaries), and the contents of the packaged archive. Re-run the audit for every dependency-rule revision and before each release (see [RELEASING.md](RELEASING.md)).

## Project license

choscorui is dual licensed under `MIT OR Apache-2.0` ([`LICENSE-MIT`](../LICENSE-MIT) and [`LICENSE-APACHE`](../LICENSE-APACHE)), at the user's option. The workspace sets `license = "MIT OR Apache-2.0"` in `[workspace.package]`, every crate inherits it with `license.workspace = true`, and `samples/notes` declares it directly. `scripts/macos/package.sh` copies `LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE` into every archive.

## Rust crates in the build graph

| Crate | Version | License | Used by | Notes |
| --- | --- | --- | --- | --- |
| `unicode-segmentation` | 1.13.3 | MIT OR Apache-2.0 | `choscorui-core` | Only third-party crate; no transitive dependencies. Dependency rule revision 1. |
| Rust standard library (`std`, `core`, `alloc`) | toolchain | MIT OR Apache-2.0 | every binary | Statically linked into release binaries. |

Workspace crates are first-party. `cargo tree --workspace -e normal --prefix none | sort -u` must list only workspace crates and the rows above. The `samples/notes` workspace depends only on the `choscorui` path crate.

A packaged binary therefore contains MIT/Apache-2.0 code from `std` and `unicode-segmentation`. Both licenses require the copyright and license notice to accompany redistributed copies. [`NOTICE`](../NOTICE) carries them, using the MIT option of each component, and ships in every archive.

## Linked system libraries (macOS)

The release binaries link exactly these libraries (`otool -L dist/choscorui-*/…/Contents/MacOS/*`):

- **Frameworks:**
  - AppKit
  - Foundation
  - CoreFoundation
  - CoreGraphics
  - CoreText
  - ImageIO
  - UserNotifications
- **Libraries:**
  - `libobjc`
  - `libSystem`

They load dynamically from the operating system and ship with macOS. The Xcode and macOS SDK license agreements govern them, and choscorui does not redistribute them. Earlier revisions of this audit listed UniformTypeIdentifiers and ApplicationServices, but neither is linked or loaded.

## Fonts and bundled assets

| Asset | Origin | Shipped in |
| --- | --- | --- |
| Fonts | None bundled. Text uses installed system fonts through Core Text. | — |
| Gallery and stress-gallery icons and images | Generated in source code at run time | Binaries |
| `samples/notes/resources/strings/*.lang` (English, Vietnamese, Hebrew) | Written for this repository | `choscorui Notes.app/Contents/Resources` |
| `Info.plist`, `install.sh`, `uninstall.sh`, `README.md` (from `docs/INSTALL.md`) | Generated or written for this repository | The archive |

The archive contains no generated binding source: all FFI declarations are hand-written (see [FFI.md](FFI.md)).
