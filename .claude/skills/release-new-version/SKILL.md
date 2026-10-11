---
name: release-new-version
disable-model-invocation: true
description: Release choscorui from one verified tag with a signed, notarized macOS universal archive and a GitHub Release whose text is the release notes.
---

# Release choscorui

Work from the repository root. Read [the release procedure](../../../docs/releasing.md),
[the versioning policy](../../../docs/versioning.md), and
[the contribution checks](../../../CONTRIBUTING.md). Follow the procedure's order;
this skill adds the decisions and safeguards around it.

## Scope and local configuration

Determine the requested version and whether the user wants preparation only, a
packaged candidate, or the full release. A request to release end to end
authorizes the release commit, tag push, and GitHub release; carry it through
without repeatedly requesting the same permission. Preparation alone does not
authorize tags or a GitHub release. If scope is missing, prepare the notes and
verified archive before asking for the publication decision.

Use the checked-out repository and its configured Git remote; never embed a
maintainer's checkout path, username, email, team ID, signing identity, or
notary profile in tracked files or notes. Resolve tools through PATH and existing
local authentication. Credentials stay in Keychain (`notarytool` profile) or
environment variables, outside Git.

## Prepare the version

Require `X.Y.Z` with no leading zeros, following versioning.md: before 1.0 a
breaking change bumps `Y`. If no version was given and none is unambiguous, ask.
Inspect Git status and the remote branch; fetch and check for divergence before
release mutations. Do not reset, stash, discard, or commit unrelated work.

Inspect local and remote `vX.Y.Z` tags and any existing GitHub release. Treat
network or authentication failures as errors, not as absence. An existing tag is
reusable only when its peeled commit equals the selected source commit. Never
force-push or move a conflicting tag.

Bump `version` in every `Cargo.toml`, including `samples/notes`, then run
`cargo update -w` and `(cd samples/notes && cargo update -w)`. Stage only the
intended manifests and lockfiles with explicit paths.

## Draft the release notes

There is no changelog file; the GitHub Release text is the record. Generate the
change list with `scripts/release-notes.sh <previous-tag> HEAD` and draft notes
in an ignored file under `dist/` (for example `dist/release-notes-X.Y.Z.md`).
Begin with `# X.Y.Z`, then `Released: YYYY-MM-DD` with the intended date.

Write `## Highlights` by hand: concise user-facing bullets, verified against the
diff, with related changes merged. Then add, only when substantiated:

- `## Breaking changes`: every break, including behavior changes shown by
  `git diff <previous-tag> -- crates/choscorui/tests/golden`.
- `## Deprecated`: deprecated items and their replacements.
- `## Security`: verified security fixes ([SECURITY.md](../../../SECURITY.md)).
- `## Known limitations`: current limits and workarounds, consistent with
  [platform-macos.md](../../../docs/platform-macos.md) and
  [controls.md](../../../docs/controls.md).
- `## Upgrading`: migrations, raised MSRV, or required user actions.

Then paste the generated change list. Finish with `## Download` naming the
archive, its SHA-256 once verified, and the macOS 15+ requirement. Do not paste
raw commit bodies, private issues, local paths, or placeholder bullets. Show the
draft to the user while the checks run and incorporate their edits.

## Verify and package

Run every check in the "Automated checks" section of releasing.md on the release machine,
including the native smoke tests one at a time and the bench budget check on the
reference machine. Resolve failures before packaging; do not weaken checks, add
retries, or raise timeouts as the only fix. For an authorized full release,
commit the version bump as `chore(release): X.Y.Z` so packaging captures a clean
commit.

Build, sign, notarize, staple, and check the archive as in the "Signed and
notarized archive" section of releasing.md, with `CHOSCORUI_UNIVERSAL=1`, the
Developer ID identity, and the notary profile from the local environment. Never bypass signing or notarization. Run
`scripts/macos/install-check.sh` on the result.

The public attachments are exactly `choscorui-X.Y.Z-macos-universal.zip` and its
`.sha256`. Do not upload `dist/` recursively or with a wildcard. Inspect the
archive and notes for credentials, personal paths, and copied maintainer
configuration; if one is found, fix the source and rebuild rather than editing
the signed archive.

## Tag and publish

Check local and remote tags again immediately before mutation. Create a signed,
annotated tag on the exact release commit only if absent, and push only that tag:

```sh
git tag -s "v$VERSION" "$RELEASE_COMMIT" -m "choscorui $VERSION"
git push "$REMOTE" "refs/tags/v$VERSION:refs/tags/v$VERSION"
```

Verify the remote tag's peeled commit, then create the GitHub release with `gh`
from the reviewed notes file and the two explicit attachment paths. Recheck the
notes' version and date first. Never use `--clobber` or replace published
assets; a faulty shipped version is fixed with a higher version.

## Report

Report the version and exact source commit, automated checks and their outcomes,
the archive SHA-256, the tag push result, and the GitHub release link or draft
status. Distinguish completed steps from pending ones. Never report real input
methods, VoiceOver, a notarized clean-machine install, Intel hardware, or budgets
on other hardware as verified from automated evidence.
