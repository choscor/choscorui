# Releasing

This document describes how to publish a ChoscorUI release for macOS. Do the steps in order. Use one source commit for the tag, the archive, and the release notes.

```mermaid
flowchart LR
    A["License audit"] --> B["Version bump"]
    B --> C["Release notes"]
    C --> D["Automated checks"]
    D --> E["Signed and notarized archive"]
    E --> F["Tag and publish"]
```

## License audit

1. Run `cargo tree --workspace -e normal --prefix none | sort -u`.
2. Compare the output with the crate table in [licenses.md](licenses.md).
3. Run `otool -L` on the packaged binaries and compare the result with the library list in licenses.md.
4. Make sure that `scripts/macos/third-party-notices.txt` names each statically linked third-party component.
5. Update the "Last checked" date in licenses.md.
6. Make sure that CI is green on `main` for `macos-15` (arm64) and `macos-15-intel` (x86-64).

## Version bump

1. Set the same `version` in each `Cargo.toml`, including `gallery/Cargo.toml` and `tools/bench/Cargo.toml`.
2. Run `cargo update -w`.

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

## Signed and notarized archive

You need a Developer ID Application certificate in the login keychain. Do this configuration one time:

```sh
xcrun notarytool store-credentials choscorui-notary --apple-id <id> --team-id <team> --password <app-specific password>
```

To build, sign, notarize, and check the archive, do these steps:

1. Run `package.sh` with the signing identity and the notary profile:

   ```sh
   CHOSCORUI_UNIVERSAL=1 \
   CHOSCORUI_SIGN_IDENTITY="Developer ID Application: <name> (<team>)" \
   CHOSCORUI_NOTARY_PROFILE=choscorui-notary \
   scripts/macos/package.sh
   ```

2. Run `scripts/macos/install-check.sh dist/choscorui-<version>-macos-universal.zip`.

`package.sh` signs the gallery app with the hardened runtime and a secure timestamp. It sends the zip to `notarytool --wait`, staples the app, and checks the app with `spctl --assess`. Then it makes the zip again and writes `dist/*.sha256`.

The archive contains the gallery app, `install.sh`, `uninstall.sh`, and `README.md` from `scripts/macos/archive-readme.md`. It also contains `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD-PARTY-NOTICES.txt` from `scripts/macos/third-party-notices.txt`. `install-check.sh` makes sure that these files are in the archive.

## Tag and publish

> **Caution:** Do not move or force-push a published tag. Do not replace a published asset. To correct a release, publish a new version.

1. Commit the version bump with the message `chore(release): <version>`.
2. Run `git tag -s v<version> -m "choscorui <version>"` on that commit.
3. Push the tag.
4. Create the GitHub Release with the release notes as its text.
5. Attach `choscorui-<version>-macos-universal.zip` and its `.sha256` file.

## Security fixes

[SECURITY.md](../SECURITY.md) describes the policy for security fixes. Do these steps:

1. Fix the problem privately.
2. Release a patch for each supported version.
3. Publish an advisory that gives credit to the reporter.
