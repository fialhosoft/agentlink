# Security Policy

## Reporting a vulnerability

Please report privately through
[GitHub Security Advisories](https://github.com/agentlink-dev/agentlink/security/advisories/new).
This opens a private discussion with maintainers before anything is public.

Please do not open a public issue for a vulnerability.

We aim to acknowledge within 3 working days and to ship a fix or a mitigation
plan within 30 days. We will credit you in the advisory unless you prefer
otherwise.

## Supported versions

While the project is pre-1.0, only the latest release receives security fixes.

## Threat model

agentlink reads and writes files inside a workspace, and provider manifests are
community-contributed data. The properties below are what the design actively
defends, and they are enforced in code rather than by convention.

### Provider manifests cannot escape the workspace

Every path in a manifest is parsed into `RelPath`, which rejects absolute paths,
Windows drive letters, UNC prefixes and any `..` segment. A malicious or mistaken
manifest therefore **cannot** address `~/.ssh`, `C:\Windows`, or anything outside
the workspace root. This is validated at load time, before any planning, and is
covered by tests in `crates/agentlink-core/src/path.rs`.

### agentlink does not delete content it did not create

- A lock file records everything materialised; nothing else is ever removed.
- Removal is never recursive: the filesystem adapter refuses to remove a path
  that is not a link, so a directory tree cannot be destroyed.
- A path holding real content is reported as `blocked`, never overwritten.

### No network access

agentlink makes no network requests. Provider manifests are embedded in the
binary at build time. There is no update check, no telemetry, and no remote
configuration.

### No code execution

agentlink does not execute hooks, scripts or anything else from a workspace or a
manifest. Manifests are pure declarative data with no executable fields.

### `unsafe` is forbidden

Every crate declares `#![forbid(unsafe_code)]`. Windows junction creation, the
one operation needing raw platform APIs, is delegated to the audited `junction`
crate rather than hand-written.

## What is explicitly out of scope

**Content trust.** agentlink shares instruction and skill files between agents; it
does not evaluate them. A malicious `SKILL.md` in your repository is a threat to
every agent that reads it, with or without agentlink — treat agent configuration
with the same care as executable code, particularly in pull requests from
untrusted contributors.

**Link targets outside the workspace created by others.** agentlink detects and
reports these (`agentlink doctor`) but does not follow or repair them
automatically.
