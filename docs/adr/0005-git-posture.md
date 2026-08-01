# ADR 0005 — Commit the canonical layout; materialise the rest locally

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

agentlink creates links inside a repository, so it must take a position on what
gets committed. Each option fails somewhere:

**Committing symlinks.** Git stores a symlink as a blob with mode `120000`. On
macOS and Linux this works perfectly — clone and everything resolves. On Windows,
git disables symlink support by default; without `core.symlinks=true` (which
itself needs the symlink privilege) the teammate receives a **plain text file
containing the target path**. Their agent then reads `../.agents/skills` as if it
were instructions. It fails silently and confusingly.

**Committing junctions.** Not possible. Git does not model reparse points; it
would recurse into the junction and commit a second full copy of the content —
reintroducing exactly the duplication agentlink exists to remove.

**Committing generated copies.** Works for everyone at clone time, and is what
copy-based tools do. But it reintroduces duplication and drift as a permanent
property of the repository.

## Decision

Commit **only the canonical layout** (`AGENTS.md`, `.agents/`). Every
agent-specific path is ignored via a managed block in `.gitignore` and
materialised locally by `agentlink apply`.

```gitignore
# >>> agentlink >>>
/.claude/skills
/.cursor/skills
/.github/skills
/CLAUDE.md
# <<< agentlink <<<
```

## Consequences

**The repository is clean and portable.** One copy of every byte, no
platform-specific artefacts, no diff noise. A reviewer sees skill changes in
`.agents/skills/` and nowhere else.

**Cross-platform teams cannot be silently broken.** Nobody receives a text file
pretending to be a directory.

**Materialisation is a per-developer step.** This is the cost. It is mitigated by
`apply` being idempotent and taking a few milliseconds, which makes it suitable
for a `post-checkout` / `post-merge` hook, a `postinstall` script, or a
devcontainer step. `agentlink status --check` exits `2` when anything is pending,
so CI can enforce it.

**A clone without agentlink still works for most agents.** Because the canonical
layout *is* the standard, Codex, Cursor, Copilot, OpenCode and Antigravity read a
fresh clone natively with agentlink never having run. Only Claude Code's paths
need materialising. The failure mode is graceful, not total.

**`.gitignore` editing is treated as delicate.** It is a file the user also owns,
so the managed block is a pure, exhaustively tested string transformation:
content outside the markers is never touched, the block is rewritten in place so
stale entries disappear, removing it restores the file byte-for-byte, and the
file's dominant line ending is preserved — silently converting a CRLF
`.gitignore` to LF would produce a whole-file diff for every Windows contributor.

## Alternatives considered

**Commit symlinks, document the Windows caveat.** Rejected: it moves a silent,
confusing failure onto the least-equipped teammate, and documentation does not
prevent it.

**Detect the platform and choose per machine.** Rejected: what gets committed
must not depend on who ran the command last. That would make the repository's
contents non-deterministic across a team.

**Make it configurable.** Partially adopted: `gitignore.manage = false` disables
the block for users who manage ignores centrally. The commit posture itself is
not configurable in v0.0.1, because a strong, well-argued default is more
valuable than an option nobody can evaluate without having hit the failure first.
