---
name: release-new-version
disable-model-invocation: true
description: Release ChoscorUI from one verified tag by publishing the crates to crates.io and creating a GitHub Release whose text is the release notes.
---

# Release ChoscorUI

Work from the repository root. Read [the release procedure](../../../docs/releasing.md),
[the versioning policy](../../../docs/versioning.md), and
[the contribution checks](../../../CONTRIBUTING.md). Follow the procedure's order;
this skill adds the decisions and safeguards around it.

## Scope and local configuration

Determine the requested version and whether the user wants preparation only or
the full release. A request to release end to end authorizes the release commit,
tag push, crates.io publish, and GitHub release; carry it through without
repeatedly requesting the same permission. Preparation alone does not authorize
tags, a crates.io publish, or a GitHub release. If scope is missing, prepare the
notes and pass the checks, including the publish dry run, before asking for the
publication decision.

Use the checked-out repository and its configured Git remote; never embed a
maintainer's checkout path, username, email, or registry token in tracked files
or notes. Resolve tools through PATH and existing local authentication. The
crates.io token stays in Cargo's credential store (`cargo login`) or the
environment, outside Git; never read or print it.

## Prepare the version

Require `X.Y.Z` with no leading zeros, following versioning.md: before 1.0 a
breaking change bumps `Y`. If no version was given and none is unambiguous, ask.
Inspect Git status and the remote branch; fetch and check for divergence before
release mutations. Do not reset, stash, discard, or commit unrelated work.

Inspect local and remote `vX.Y.Z` tags, any existing GitHub release, and whether
crates.io already has `X.Y.Z` of each published crate. Treat network or
authentication failures as errors, not as absence. An existing tag is
reusable only when its peeled commit equals the selected source commit. Never
force-push or move a conflicting tag.

Bump `version` in every `Cargo.toml` and in each dependency on a workspace
crate, then run `cargo update -w`. Stage only
the intended manifests and the lockfile with explicit paths.

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

Then paste the generated change list. Finish with `## Install`: the
`cargo add choscorui@X.Y.Z` command, the macOS 15+ requirement, and the MSRV. Do not paste
raw commit bodies, private issues, local paths, or placeholder bullets. Show the
draft to the user while the checks run and incorporate their edits.

## Verify

Run every check in the "Automated checks" section of releasing.md on the release machine,
including the native smoke tests one at a time, the publish dry run, and the
bench budget check on the reference machine. Resolve failures before publishing;
do not weaken checks, add retries, or raise timeouts as the only fix. For an
authorized full release, commit the version bump as `chore(release): X.Y.Z` and
run the dry run again on that clean commit.

Inspect `cargo package --workspace --list` and the notes for credentials,
personal paths, and copied maintainer configuration; if one is found, fix the
source before publishing.

## Tag and publish

Check local and remote tags again immediately before mutation. Create a signed,
annotated tag on the exact release commit only if absent, and push only that tag:

```sh
git tag -s "v$VERSION" "$RELEASE_COMMIT" -m "choscorui $VERSION"
git push "$REMOTE" "refs/tags/v$VERSION:refs/tags/v$VERSION"
```

Verify the remote tag's peeled commit, then run `cargo publish --workspace --locked`
from the clean release commit. A crates.io version is permanent: it can be
yanked but never replaced, so a faulty shipped version is fixed with a higher
version. If the publish stops partway, publish only the missing crates from the
same commit; do not bump or retag.

Then create the GitHub release with `gh` from the reviewed notes file, with no
attachments. Recheck the notes' version and date first.

## Report

Report the version and exact source commit, automated checks and their outcomes,
the tag push result, the published crates.io versions, and the GitHub release
link or draft status. Distinguish completed steps from pending ones. Never report real input
methods, VoiceOver, a notarized clean-machine install, Intel hardware, or budgets
on other hardware as verified from automated evidence.
