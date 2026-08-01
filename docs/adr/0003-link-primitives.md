# ADR 0003 — Symlinks and junctions, never hard links

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

To place one inode at several paths, a filesystem offers three primitives, and
they are not interchangeable:

| Primitive | Links dirs | Links files | Windows privilege | Target |
|---|---|---|---|---|
| Symlink | yes | yes | **elevation or Developer Mode** | relative or absolute |
| Junction | yes | **no** | **none, ever** | absolute only |
| Hard link | no | yes | none | n/a (same inode) |

Two Windows facts drive everything:

- Creating a **junction** (`mklink /J`) has never required a privilege.
- Creating a **symlink** requires `SeCreateSymbolicLinkPrivilege`, granted only
  to elevated processes or when Developer Mode is enabled.

The highest-value resource — skills — is a **directory**. So on stock Windows,
junctions cover the case that matters most, with no setup at all.

## Decision

Support **symlinks** and **junctions**. Do not use hard links.

Preference order, chosen per node kind after *probing the host at runtime*:

- Directory → symlink if permitted, else junction.
- File → symlink if permitted, else the provider's declared `import` fallback.

Support is probed by attempting a throwaway symlink, not inferred from the target
triple: whether a Windows process may create symlinks depends on Developer Mode
and elevation, so two machines running the same build genuinely differ.

## Consequences

**agentlink works on stock Windows with no setup.** This is the differentiator
that makes the whole approach viable rather than a Unix-only trick.

**Symlinks are preferred because they store a relative target.**
`.claude/skills → ../.agents/skills` survives the workspace being moved, copied
or cloned to a different path. Junctions cannot express this; they record an
absolute path.

**Stale junctions are a real failure mode, so they are detected.** Moving or
copying a workspace leaves a junction resolving to the *original* location —
silently, with the old content still readable, which is worse than an error.
`RootedWorkspace::stale_junctions` finds these and `agentlink doctor` reports
them, with `agentlink apply` rebuilding them in place.

**Mechanism churn is deliberately avoided.** If Developer Mode is enabled later,
existing junctions are *not* rewritten into symlinks. A link pointing at the
right place is correct, and rewriting working links produces diffs and risk for
no benefit. The planner therefore treats "points at the canonical path" as
up-to-date regardless of primitive.

**Files on unprivileged Windows need the `import` fallback.** This is not a
compromise: for Claude Code the stub is `@AGENTS.md`, which uses Claude Code's own
officially supported import syntax. It is ~11 bytes, never changes, survives
atomic saves, and works identically in git.

## Alternatives considered

**Hard links for files, to avoid needing symlink privileges.** Rejected, and this
is the most important rejection in this ADR. Hard links are:

- **Undetectable after the fact.** A hard link is indistinguishable from a
  regular file, so agentlink could never tell its own work from the user's — and
  the safety model depends on exactly that distinction.
- **Silently broken by ordinary editors.** Most editors save atomically:
  write a temporary file, then rename it over the target. That replaces the
  inode. The two paths then diverge with no error and no way to notice — the
  worst possible failure for a tool whose entire promise is that they cannot
  diverge.

A `render`-style copy would at least be honest about being a copy. A hard link
looks like a link and behaves like one until it quietly stops.

**Requiring Developer Mode on Windows.** Rejected: it is a per-machine setting
many corporate environments forbid, and demanding it before first run would
disqualify agentlink for a large share of its audience — for a capability
junctions already provide.

**Copying as the fallback instead of `import`.** Rejected where a tool offers an
include syntax, since a stub keeps a single source of truth while a copy
reintroduces drift. Copying remains the honest last resort for tools with no
include mechanism, and is tracked as `render` for v0.1.
