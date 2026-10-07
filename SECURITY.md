# Security policy

## Supported versions

choscorui is pre-1.0. Only the latest released version receives security fixes. After 1.0, the latest minor release of the current major version will receive them.

## Reporting a vulnerability

Do not open a public issue for a vulnerability.

Report it privately through the repository's **Security → Report a vulnerability** form (GitHub private vulnerability reporting). Include:

- the affected version or commit, and the macOS version and architecture
- the impact (for example memory unsafety reachable from safe Rust, a crash from untrusted input, or data exposure through the clipboard, drag and drop, or accessibility)
- a minimal reproduction, ideally a small program that uses only the `choscorui` facade

We aim to acknowledge reports within 5 working days and to agree on a disclosure date within 30 days. Fixes ship as patch releases with an advisory that credits the reporter unless they ask otherwise.

## Scope

In scope:

- **Memory safety.** Undefined behavior, use-after-free, or out-of-bounds access reachable from safe Rust through the public API. This includes the hand-written FFI in `choscorui-macos` (see [docs/FFI.md](docs/FFI.md)).
- **Untrusted input.** Panics or crashes caused by:
  - content that may come from untrusted sources, such as image and SVG decoding, rich text, Markdown, HTML, settings, and translation files
  - pasted or dropped data
- **Data exposure.** Leaks of secure-text contents (password fields) through accessibility, the clipboard, or input methods.

Out of scope:

- Bugs in macOS system frameworks.
- Denial of service through deliberately enormous but valid input. Report these as ordinary performance issues.
