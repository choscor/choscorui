# Security policy

## Supported versions

Only the latest release receives security fixes. After 1.0, the latest minor release of the current major version receives security fixes.

## Reporting a vulnerability

Do not open a public issue for a vulnerability.

Report it privately through the **Security → Report a vulnerability** form of the repository. Include this information:

- the affected version or commit, and the macOS version and architecture
- the effect, for example memory unsafety from safe Rust, a crash from untrusted input, or data exposure
- a minimal reproduction, if possible a small program that uses only the `choscorui` facade

We try to reply to a report in 5 working days. We try to agree on a disclosure date in 30 days. A fix ships in a patch release with an advisory. The advisory gives credit to the reporter, unless the reporter does not want it.

## Scope

These problems are in scope:

- **Memory safety.** Undefined behavior, use after free, or access out of bounds from safe Rust through the public API. This includes the hand-written FFI in `choscorui-macos` (see [docs/ffi.md](docs/ffi.md)).
- **Untrusted input.** A panic or a crash from data that can come from an untrusted source. Examples are images, SVG, rich text, Markdown, HTML, settings files, and translation files. Pasted and dropped data are also untrusted.
- **Data exposure.** A leak of the text in a password field through accessibility, the clipboard, or an input method.

These problems are not in scope:

- Bugs in the macOS system frameworks.
- Denial of service through very large input that is valid. Report it as a performance issue.
