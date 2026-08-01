# ADR 0001 — Link instead of generate

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

Several tools already solve "share configuration across AI coding agents":
`rulesync` (~1.3k stars, 40+ tools), `agent_sync`, `ai-rules-sync`. All of them
work the same way: read a source directory, **write a copy** into each tool's
location.

That architecture was correct in 2025, when every tool had a genuinely different
format. It is no longer correct, because the ecosystem converged:

- **AGENTS.md** became an open specification (OpenAI, Google, Cursor and Factory,
  August 2025), donated to the Linux Foundation's Agentic AI Foundation in
  December 2025. Read by 20+ tools, adopted by 60k+ repositories.
- **Agent Skills / `SKILL.md`** was published as an open specification in
  December 2025. Microsoft and OpenAI shipped support within 48 hours; by March
  2026 more than 30 tools read the identical file format.

So for the two highest-value resources, the *format* is already the same across
tools. Only the *directory* differs — `.claude/skills`, `.cursor/skills`,
`.github/skills`, `.agents/skills`.

Copy-generation in that world has four costs, all of which users hit:

1. **Duplication.** N copies of every byte.
2. **A step to remember.** Nothing is shared until `generate` runs.
3. **Drift.** Editing the generated copy is the natural thing to do, and it is
   silently discarded on the next run.
4. **Orphans.** Renaming or deleting on the source side leaves the copy behind.

## Decision

Do not copy. Put the *same inode* everywhere the tool already knows how to look.

Each pairing of a provider with a resource resolves to one of four verdicts:

| Verdict | Meaning | Action |
|---|---|---|
| `native` | the tool already reads the canonical path | nothing |
| `link` | same format, different path | symlink or junction |
| `import` | link impossible, but the tool has an include syntax | one-line stub |
| `blocked` | ambiguous — needs a human | nothing is written |

## Consequences

**The bidirectionality problem dissolves.** It is not implemented; it is a
property of having one inode. Creating, editing, renaming, moving and deleting
through any agent's path is immediately visible through every other, with no
process running. This is strictly stronger than any copy-based tool can offer,
and it is *free*.

**No daemon, no watcher, no file-system events.** The filesystem is the daemon.
The CLI is a fast, idempotent reconciler, which is what makes it safe to put in a
git hook.

**"The best sync is no sync."** Because the canonical layout was chosen to be
what most tools already read (see [ADR 0002](0002-canonical-layout.md)), the most
common verdict is `native` — literally zero work. Of the twelve capabilities
shipped in v0.0.1, seven are `native`.

**Transformation is still needed eventually.** MCP configuration genuinely
differs in format between tools (`mcpServers` vs `servers`, `env` vs
`environment`, JSON vs TOML). A `render` verdict with content-addressed
provenance and explicit drift reconciliation is planned for v0.1. The point of
this ADR is that it should be the *exception*, applied only where format actually
diverges — not the default applied to everything.

**Some things cannot be linked.** Files on Windows without symlink privileges are
the main case. That is what `import` exists for; see
[ADR 0003](0003-link-primitives.md).

## Alternatives considered

**Copy generation, like the existing tools.** Rejected: it is the status quo, it
cannot deliver bidirectionality, and it duplicates content that is already
byte-identical across tools. It would also mean competing with an established
project on its own terms rather than on a better architecture.

**A file-watching daemon that mirrors edits between copies.** Rejected: it
inherits every drift and conflict problem of copying, and adds a background
process, startup ordering, race conditions and platform-specific event APIs. It
is a great deal of machinery to approximate what one reparse point gives exactly.

**A FUSE / projected filesystem.** Rejected as far too invasive for the problem:
it requires a driver or elevated privileges on every platform, is unavailable in
many CI and container environments, and would gate adoption on an installation
step far heavier than the problem justifies.
