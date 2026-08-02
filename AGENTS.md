# AGENTS.md

Shared instructions for every AI coding agent working on agentlink.

This file is managed by agentlink itself — `CLAUDE.md` points here, so editing it
from any agent updates all of them.

## What this project is

agentlink gives every AI coding agent in a repository the same rules and skills
**without copying files**. Where a tool already reads the canonical path we do
nothing; where only the location differs we create a filesystem link; where
linking is impossible we write a one-line include stub.

Read [docs/adr/0001-link-instead-of-generate.md](docs/adr/0001-link-instead-of-generate.md)
before proposing architectural changes. Most "obvious" alternatives were
considered and rejected there for concrete reasons.

## Commands

```console
cargo test                      # unit + integration, ~1s
cargo clippy --all-targets      # must be clean
cargo fmt                       # rustfmt.toml, 100 columns
cargo run -p agentlink-cli -- status
```

## Architecture

```text
agentlink-cli     composition root, human-facing output
      ↓
agentlink-domain    pure domain — no std::fs anywhere
      ↑
agentlink-fs      adapter: symlinks, junctions, privilege probing
```

`agentlink-domain` never touches the filesystem. It depends on the `Workspace`
trait; `agentlink-fs` implements it for real and `testing::FakeWorkspace`
implements it in memory. That is why tests can simulate Windows without symlink
privileges on a Linux runner.

Planning and execution are separate. `status` and `apply` render the *same* plan,
so nothing happens that was not announced first.

## The rule that outranks everything

**Never destroy content agentlink did not create.**

Any code path that removes, overwrites or relocates a file must consult the lock
file first, and must resolve every ambiguity to a `Blocked` outcome naming the
fix — never to a guess. If a change seems to need `--force`, add a diagnostic
instead.

## Conventions

- Paths in the domain are `RelPath`: normalised, always `/`-separated, and unable
  to escape the workspace. Never introduce a raw `PathBuf` into `agentlink-domain`.
- Adding an agent must stay a **single TOML file** in
  `crates/agentlink-domain/providers/`. If a change
  would require Rust to support a new agent, that is a design problem — see
  [ADR 0004](docs/adr/0004-providers-as-data.md).
- `#![forbid(unsafe_code)]` in every crate. Windows junctions go through the
  `junction` crate.
- Tests are named as the sentence they prove:
  `a_hand_written_file_is_never_replaced`, not `test_apply_3`.
- Conventional Commits. `feat(providers): add <agent>` for new agents.

## Where to be careful

- `crates/agentlink-domain/src/plan.rs` — the capability lattice. Every new branch
  needs a test for the case where the user's content is at risk.
- `crates/agentlink-fs/src/lib.rs` — the only place with platform-specific code.
  Windows behaviour differs by *privilege*, not just by OS.
- `crates/agentlink-domain/src/gitignore.rs` — edits a file the user owns. It must
  preserve surrounding content and existing line endings exactly.

## Do not

- Add a dependency without a clear justification; the surface is deliberately
  small (`serde`, `toml`, `thiserror`, `clap`, `anyhow`, `junction`).
- Introduce hard links. See [ADR 0003](docs/adr/0003-link-primitives.md) — they
  are undetectable after the fact and silently broken by atomic saves.
- Invent configuration formats. The canonical layout is the community standard.
