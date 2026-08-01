# ADR 0002 — Adopt `AGENTS.md` + `.agents/` as the canonical layout

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

agentlink needs one location to be the source of truth. Three options existed:

1. Invent a namespaced directory, e.g. `.agentlink/rules/`, `.agentlink/skills/`.
2. Elect one existing tool's layout, e.g. `.claude/`.
3. Adopt the layout the ecosystem is converging on.

The relevant facts:

- `AGENTS.md` is a Linux Foundation specification read natively by Codex, Cursor,
  Copilot, OpenCode, Antigravity and many others.
- `.agents/skills/` is read natively by Antigravity (IDE, CLI and AGY) and is in
  Codex's skill resolution path (`$REPO_ROOT/.agents/skills/`).
- Claude Code is the notable exception: it reads `CLAUDE.md`, not `AGENTS.md`
  ([anthropics/claude-code#6235](https://github.com/anthropics/claude-code/issues/6235),
  5,200+ reactions).

## Decision

Use `AGENTS.md` and `.agents/skills/` as the canonical layout.

Enforce this in code: a manifest declaring `strategy = "native"` at a path that
is not the canonical one is **rejected at load time**. A `native` verdict is a
factual claim that a tool reads the canonical path; if that claim were wrong,
agentlink would write nothing while the user believed they were covered.

## Consequences

**Every native verdict is free work.** Choosing the layout most tools already
read directly maximises the number of pairings that need no action at all. This
is the single highest-leverage decision in the project: it converts an
integration problem into a no-op for the majority of cases.

**Zero lock-in.** The canonical layout is a public standard, not an agentlink
format. A user who removes agentlink keeps a repository that Codex, Cursor,
Copilot, OpenCode and Antigravity all still read natively. `agentlink clean`
removes only the materialised paths and leaves the canonical layout untouched.
A tool that is easy to leave is easier to adopt.

**agentlink gets better as the ecosystem converges further.** Every tool that
adds `AGENTS.md` support turns a `link` into a `native` — less work, not more.
If Claude Code closes #6235, its instructions capability becomes `native` by
changing one word in one manifest.

**The layout is a shared dependency.** If the standards shift, agentlink must
follow. The canonical paths are centralised in `layout.rs` precisely so that this
is a small, well-tested change rather than a diffuse one.

## Alternatives considered

**Invent `.agentlink/`.** Rejected. It would make *every* pairing a `link` or
`render`, throwing away the free `native` verdicts. It would also create genuine
lock-in and add one more competing convention to a space that just finished
converging — the exact behaviour that made this problem exist.

**Elect `.claude/`.** Rejected. It is the *only* major layout that no other tool
reads natively, so it would maximise work rather than minimise it, and it would
bind a vendor-neutral tool to one vendor.

**Make the layout user-configurable.** Deferred. The `Layout` type already
carries the paths as data rather than constants, so this is a small change if a
real need appears. Shipping it now would mean supporting arbitrary layouts before
learning which ones anyone actually wants, and would weaken the strong default
that makes the tool comprehensible.
