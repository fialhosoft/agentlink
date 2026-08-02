---
name: document-change
description: Write the change file that every agentlink pull request must include. Use before opening a pull request, when CI fails the "Change documented" check, or when asked what version a change will release as.
---

# Documenting a change

agentlink's changelog is never written by hand. Each pull request leaves a
**change file** in `.changeset/`, and the release compiles every pending file
into `CHANGELOG.md`, derives the version from them, and deletes them.

CI fails a pull request that adds none. That check is the only thing standing
between a behaviour change and a release that cannot explain itself, so treat
"add the change file" as part of writing the code, not paperwork after it.

## Write the file

`knope document-change` prompts for the type and summary. Writing the file
directly is equally fine — it is `.changeset/<short-kebab-name>.md`:

```md
---
default: minor
---

# `agentlink status` explains why a path is blocked

Previously a blocked path reported only that it was blocked. It now names the
file it collided with and the command that resolves it.
```

The `#` heading is the entry a reader sees in the release notes; everything
below it is the detail. A heading with no body is rendered as a plain bullet,
which is the right shape for a one-line fix.

## Choosing the type

The front-matter key is the type. It picks both the changelog section and the
version bump:

| Type | Section | Use for |
|---|---|---|
| `major` | Breaking changes | An existing workspace stops working, or needs a migration |
| `minor` | Added | A new command, flag, provider or capability |
| `changed` | Changed | Different behaviour that breaks nothing |
| `deprecated` | Deprecated | Still works, will be removed |
| `removed` | Removed | Gone |
| `patch` | Fixed | A bug fix |
| `security` | Security | A vulnerability or a hardening change |

**agentlink is pre-1.0**, where semver treats the whole crate as unstable:
`minor` and below all land as a patch bump (0.0.2 → 0.0.3) and `major` moves the
minor (0.0.2 → 0.1.0). Pick the type that describes the change honestly and let
the version follow; do not inflate the type to force a version.

A pull request may add several change files, and should when it makes more than
one change worth announcing. The release takes the largest bump among them.

## When not to write one

Refactors, tests, CI and documentation change nothing a user of the released
binary would notice. Those carry the **`no changelog`** label on the pull
request, which is what the CI check looks for. Reach for the label when the
answer to *"would someone reading the release notes care?"* is genuinely no —
not when writing the note is inconvenient.

`feat(providers): add <agent>` always needs one: a new provider is exactly the
kind of thing people upgrade to get.

## Verifying

`knope prepare-release --dry-run` prints the version and the changelog section
the pending files produce, without touching anything. CI runs the same command,
so malformed front matter fails on the pull request rather than at release.

## What happens next

Nothing else is manual. Merging to `main` opens a release pull request that
bumps the version, compiles `CHANGELOG.md` and empties `.changeset/`; merging
*that* tags the release and publishes binaries, the GitHub release, crates.io
and npm in one run. Never edit `CHANGELOG.md` or a version in `Cargo.toml` by
hand — the release pull request owns both, and a manual edit is overwritten.
