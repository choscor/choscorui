---
name: pr-description
description: Draft or revise a choscorui pull request title and body from the actual changes and verification evidence. Use when preparing a PR for review or updating its description after the diff changes.
---

# Pull request description

Write for an open source reviewer who has not followed the implementation. Follow [CONTRIBUTING.md](../../../CONTRIBUTING.md) and the repository's [PR template](../../../.github/pull_request_template.md).

1. Inspect the PR's actual base and diff, changed tests and docs, and any relevant issue or design discussion. Do not describe unrelated working-tree changes or infer results from code alone. If the base is unknown, identify the intended base or say what range was inspected.
2. Give the title a specific outcome in the project's Conventional Commit style: `<type>(<scope>): <imperative summary>`. Choose a scope a contributor can recognize, such as `core`, `controls`, `compose`, `macos`, `facade`, `gallery`, `docs`, or `ci`; do not use a generic title such as `fix bug`. The PR title is one line.
3. In the body, state the problem and why it matters, then summarize the meaningful behavior or design changes. Highlight reviewer-relevant tradeoffs, public API or golden-file changes, compatibility or migration effects, and where to focus review when applicable. Call out new or changed `unsafe` FFI and its `docs/ffi.md` entry. Avoid a file-by-file changelog or claims broader than the diff.
4. Report exact verification commands and outcomes that are known. Distinguish local results from CI, and name unrun or blocked relevant checks with the reason. Unit, smoke, and autopilot runs are not IME, VoiceOver, notarized-install, or other-hardware evidence; say what was checked by hand, if anything, and in which themes. Include screenshots or recordings only when they help review a visible change and are actually available.
5. Link related issues or design context when available. Use `Closes #N` only when merging this PR fully resolves that issue; otherwise use `Refs #N` or a normal link. Remove empty sections and template instructions from the final body. Recheck the description when the diff changes during review.

Return a ready-to-paste title and Markdown body when asked to draft. Creating or editing a remote PR is a separate action and requires that the user has asked for it.
