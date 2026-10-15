# Releasing

This document describes how to publish a ChoscorUI release for macOS. Do the steps in order. Use one source commit for the tag and the release notes. A release publishes the crates to crates.io. The GitHub Release has no attached files.

```mermaid
flowchart LR
    A["License audit"] --> B["Version bump"]
    B --> C["Release notes"]
    C --> D["Automated checks"]
    D --> E["Tag and publish"]
```

## License audit

1. Run `cargo tree --workspace -e normal --prefix none | sort -u`.
2. Compare the output with the crate table in [licenses.md](licenses.md).
3. Run `cargo build --release -p choscorui-gallery`.
4. Run `otool -L target/release/choscorui-gallery` and compare the result with the library list in licenses.md.
5. Update the "Last checked" date in licenses.md.
6. Make sure that CI is green on `main` for `macos-15` (arm64) and `macos-15-intel` (x86-64).

## Version bump

1. Set the same `version` in each `Cargo.toml`, including `gallery/Cargo.toml` and `tools/bench/Cargo.toml`.
2. Set the same `version` in each dependency on a workspace crate.
3. Run `cargo update -w`.

## Release notes

The repository has no changelog file. The release notes become the text of the GitHub Release.

`scripts/release-notes.sh` groups the commits by type. It also writes two draft sections:

- **Breaking changes** lists each breaking commit with the text of its `BREAKING CHANGE:` paragraphs.
- **Deprecated** lists each commit whose body says "deprecated", with the sentences that say it.

1. Run `scripts/release-notes.sh <previous-tag> HEAD` to get the list of changes.
2. Write the notes in an ignored file, for example `dist/release-notes-<version>.md`.
3. Write the highlights by hand.
4. Write the **Breaking changes**, **Deprecated**, **Known limitations**, and **Upgrading**, as [versioning.md](versioning.md) describes. Start from the draft sections.
5. Paste the generated list of changes.
6. Run `git diff <previous-tag> -- crates/choscorui/tests/golden`.
7. If a golden file changed, describe the behavior change under **Breaking changes**.

## Automated checks

Do these checks on the release machine. Run the native tests one at a time, because they open foreground windows.

1. Run the fast checks and the native checks in [CONTRIBUTING.md](../CONTRIBUTING.md).
2. Run `scripts/check-docs.sh`.
3. Run `cargo build --release -p choscorui-gallery -p choscorui-bench`.
4. On the reference machine, run `target/release/choscorui-bench --check bench/budgets.txt`.

[performance.md](performance.md) describes the reference machine and the budgets.

## Tag and publish

You need a crates.io API token that can publish the crates. Run `cargo login` one time. Cargo keeps the token in its credential store, outside the repository.

The workspace publishes `choscorui`, `choscorui-core`, `choscorui-controls`, `choscorui-compose`, and `choscorui-macos`. The other packages set `publish = false`.

> **Caution:** Do not move or force-push a published tag. To correct a release, publish a new version.

1. Commit the version bump with the message `chore(release): <version>`.
2. Run `git tag -s v<version> -m "choscorui <version>"` on that commit.
3. Push the tag.

> **Caution:** crates.io keeps each published version permanently. You can yank a version, but you cannot delete it or publish it again.

4. Run `cargo publish --workspace --locked`. Cargo publishes the crates in dependency order.
5. Create the GitHub Release with the release notes as its text. Do not attach files.

## Security fixes

[SECURITY.md](../SECURITY.md) describes the policy for security fixes. Do these steps:

1. Fix the problem privately.
2. Release a patch for each supported version.
3. Publish an advisory that gives credit to the reporter.
