# Architecture Decision Records

Each record captures one decision, the context that forced it, and — most
usefully — **what was rejected and why**. When a decision is revisited, the ADR
is superseded rather than edited, so the reasoning stays recoverable.

| # | Decision | Status |
|---|---|---|
| [0001](0001-link-instead-of-generate.md) | Link instead of generate | Accepted |
| [0002](0002-canonical-layout.md) | Adopt `AGENTS.md` + `.agents/` as the canonical layout | Accepted |
| [0003](0003-link-primitives.md) | Symlinks and junctions, never hard links | Accepted |
| [0004](0004-providers-as-data.md) | Providers are declarative data, not code | Accepted |
| [0005](0005-git-posture.md) | Commit the canonical layout; materialise the rest locally | Accepted |
| [0006](0006-rust-with-npm-distribution.md) | Rust core, distributed through npm | Accepted |

## Writing one

Open an ADR for anything that would be expensive to reverse, that a future
maintainer would otherwise re-litigate, or where the obvious choice was rejected.

Use the next number and this shape:

```markdown
# ADR NNNN — Title in the imperative

- **Status:** Proposed | Accepted | Superseded by [ADR NNNN](...)
- **Date:** YYYY-MM-DD

## Context
The forces at play. Facts, not preferences.

## Decision
What we are doing.

## Consequences
What follows — including the costs. An ADR with no downsides listed is
incomplete.

## Alternatives considered
What was rejected, and the specific reason. This is the section people come back
for.
```
