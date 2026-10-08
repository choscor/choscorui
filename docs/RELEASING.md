# Releasing

This is the checklist for publishing a choscorui release on macOS, the only supported platform. Do every step in order. A release is blocked until each automated gate passes and each manual gate is signed off in the release issue.

## 1. Preconditions

- [ ] `LICENSE` (MIT) and `NOTICE` are current, and the archive contains both (see [LICENSES.md](LICENSES.md)).
- [ ] The license audit has been re-run: `cargo tree --workspace -e normal --prefix none | sort -u`, `otool -L` on the packaged binaries, and the archive contents all match [LICENSES.md](LICENSES.md).
- [ ] CI is green on `main` for both `macos-15` (arm64) and `macos-15-intel`.

## 2. Version and notes

1. Bump `version` in every `Cargo.toml`, including `samples/notes`; all crates share one version. Then run `cargo update -w` and `(cd samples/notes && cargo update -w)`.
2. Generate the change list: `scripts/release-notes.sh <previous-tag> HEAD`.
3. In [CHANGELOG.md](../CHANGELOG.md), rename **Unreleased** to the version and date.
   - Write highlights by hand.
   - Paste the generated list.
   - Fill in **Breaking changes**, **Deprecated**, **Known limitations**, and **Upgrading** (see [VERSIONING.md](VERSIONING.md)).
4. If any golden file changed since the last tag (`git diff <previous-tag> -- crates/choscorui/tests/golden`), describe the behavior change under Breaking changes.

## 3. Automated gates (local, on the release machine)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88 check --workspace --all-targets --locked
cargo test --workspace --locked
CHOSCORUI_RUN_MACOS_SMOKE=1 cargo test -p choscorui-macos --test window_smoke --test menu_smoke --test lifetime_smoke
CHOSCORUI_RUN_MACOS_CLIPBOARD=1 cargo test -p choscorui-macos --test clipboard_smoke
(cd samples/notes && cargo test --locked && NOTES_AUTOPILOT=1 cargo run --locked)
cargo build --release -p choscorui-gallery -p choscorui-bench
target/release/choscorui-bench --check bench/budgets.txt     # on the reference machine (BUDGETS.md)
```

## 4. Signed, notarized archive

One-time setup, with a Developer ID Application certificate in the login keychain:

```sh
xcrun notarytool store-credentials choscorui-notary --apple-id <id> --team-id <team> --password <app-specific password>
```

Build, sign, notarize, staple, and verify:

```sh
CHOSCORUI_UNIVERSAL=1 \
CHOSCORUI_SIGN_IDENTITY="Developer ID Application: <name> (<team>)" \
CHOSCORUI_NOTARY_PROFILE=choscorui-notary \
scripts/macos/package.sh
scripts/macos/install-check.sh dist/choscorui-<version>-macos-universal.zip
```

`package.sh` signs with the hardened runtime and a secure timestamp. It submits the zip with `notarytool --wait`, staples each app, checks it with `spctl --assess`, and re-zips it. It also writes `dist/*.sha256`.

## 5. Manual gates

Record each result in the release issue:

- [ ] **Fresh machine.** Use a clean macOS 15 user account or VM with no developer tools. Download the archive and check its SHA-256. Unzip it, run `./install.sh`, and launch each app from Finder; Gatekeeper must not warn. Run the three scenario checks in the archive's README, then `./uninstall.sh`. Confirm that `~/Applications` and `~/Library/{Application Support,Preferences,Caches,Saved Application State}` contain nothing for `com.choscor.*`.
- [ ] **Intel hardware.** Do the same on an Intel Mac. This is required until an Intel run is recorded in [FFI.md](FFI.md).
- [ ] **Input methods.** Type with Japanese (Kotoeri), Vietnamese Telex, and Pinyin in the gallery's text field and document: marked text, candidate window position, commit, and cancel.
- [ ] **VoiceOver.** Walk through the gallery, the stress gallery's list and canvas, and two Notes windows: [ACCESSIBILITY.md](ACCESSIBILITY.md#manual-voiceover-checklist-gallery).
- [ ] **Notifications.** In the signed Stress bundle, test permission, delivery, and that clicking a notification opens the app ([DESKTOP.md](DESKTOP.md)).
- [ ] **Budgets on other hardware.** Run `choscorui-bench --check` on each additional Mac model before claiming it ([BUDGETS.md](BUDGETS.md#process)).

## 6. Publish

1. Commit the version bump and changelog: `chore(release): <version>`.
2. Tag the commit: `git tag -s v<version> -m "choscorui <version>"`.
3. Push the tag.
4. Create the release and attach `choscorui-<version>-macos-universal.zip` and its `.sha256`. The changelog section for this version is the release text.
5. Add a new **Unreleased** heading to `CHANGELOG.md`.

## Security fixes

Security fixes follow [SECURITY.md](../SECURITY.md): fix privately, release a patch for each supported version, then publish an advisory that credits the reporter.
