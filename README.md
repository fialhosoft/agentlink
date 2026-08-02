<div align="center">

# agentlink

**One brain for every AI coding agent — with zero file duplication.**

[![CI](https://github.com/fialhosoft/agentlink/actions/workflows/ci.yml/badge.svg)](https://github.com/fialhosoft/agentlink/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/agentlink-cli.svg)](https://crates.io/crates/agentlink-cli)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Write your rules and skills once. Claude Code, Cursor, Codex, Copilot,
Antigravity and OpenCode all read the same files — the *same inode*, not a copy.

</div>

---

## The problem everyone has

You write a skill for Claude Code. Now you want it in Cursor. And Copilot. And
Codex. So you copy it. Then you fix a typo in one of them, and the others drift.
Rename a folder and you have an orphan. Add a seventh agent and you copy again.

Existing tools solve this by **generating copies** from a source directory. That
trades one duplication problem for another: a `generate` step you must remember
to run, files that go stale the moment anyone edits the wrong one, and renames
that leave orphans behind.

## The insight

In 2026 the ecosystem already converged. For the two resources that matter most,
**the format is identical and only the location differs**:

- **Instructions** → `AGENTS.md`, an open spec donated to the Linux Foundation in
  December 2025, read by 20+ tools.
- **Skills** → Agent Skills / `SKILL.md`, an open specification read by 30+ tools.

| | Claude Code | Cursor | Copilot | Antigravity | Codex |
|---|---|---|---|---|---|
| **Skills format** | `SKILL.md` | `SKILL.md` | `SKILL.md` | `SKILL.md` | `SKILL.md` |
| **Skills location** | `.claude/skills` | `.cursor/skills` | `.github/skills` | `.agents/skills` | `.agents/skills` |

Converting an identical format to an identical format is invented work. The real
problem is **placement** — and placement is what filesystems solve.

## What agentlink does instead

Every pairing of an agent with a resource gets one of four verdicts:

| Verdict | When | Action | Cost |
|---|---|---|---|
| **native** | the tool already reads the canonical path | **nothing** | zero |
| **link** | same format, different path | symlink or junction | one reparse point |
| **import** | linking impossible; the tool has an include syntax | a one-line stub | ~11 bytes |
| **blocked** | needs a human decision | nothing is written | — |

The canonical layout is `AGENTS.md` + `.agents/`, which is not an agentlink
invention — it is what Antigravity and Codex already read natively. Choosing the
layout the ecosystem converged on is what keeps most verdicts at **native**, and
every native verdict is work that never happens.

### Why this makes bidirectional sync disappear

For `native` and `link` verdicts there is exactly **one inode**. So this is not a
feature anyone implemented — it is arithmetic:

```console
$ mkdir .cursor/skills/deploy && echo "..." > .cursor/skills/deploy/SKILL.md

$ cat .claude/skills/deploy/SKILL.md    # instantly there
$ cat .github/skills/deploy/SKILL.md    # instantly there
$ mv .github/skills/deploy .github/skills/release   # renamed for everyone
$ rm -rf .opencode/skills/release                   # gone for everyone
```

No daemon. No watcher. No `generate` step. No drift, because there is nothing to
drift *from*.

## Quick start

```console
$ agentlink init      # create the canonical layout and link every agent
$ agentlink adopt     # or: absorb the .claude/ you already have
$ agentlink status    # see exactly who reads what
```

Onboarding an existing repository that has only ever used Claude Code:

```console
$ agentlink adopt

  AGENTS.md (instructions)
    antigravity      native  reads AGENTS.md directly — nothing to do
    claude-code      ok      CLAUDE.md (import stub)
    codex            native  reads AGENTS.md directly — nothing to do
    cursor           native  reads AGENTS.md directly — nothing to do
    github-copilot   native  reads AGENTS.md directly — nothing to do
    opencode         native  reads AGENTS.md directly — nothing to do

  .agents/skills (skills)
    antigravity      native  reads .agents/skills directly — nothing to do
    claude-code      ok      .claude/skills (junction)
    codex            native  reads .agents/skills directly — nothing to do
    cursor           ok      .cursor/skills (junction)
    github-copilot   ok      .github/skills (junction)
    opencode         ok      .opencode/skills (junction)

  12 capabilities · 12 need nothing · 0 files copied

  3 created · 2 adopted
```

Seven of those twelve capabilities cost nothing at all, because those tools
already read the canonical paths. **`0 files copied` is structural, not a
coincidence.**

## Installation

```console
# npm — ships a prebuilt binary per platform, no Rust needed
npm install -g @fialhosoft/agentlink

# Homebrew
brew install fialhosoft/tap/agentlink

# Cargo
cargo install agentlink-cli
```

## Platform support

agentlink works on Windows **without administrator rights or Developer Mode**,
which is the case most tools in this space get wrong.

| | Directories (skills) | Files (`CLAUDE.md`) |
|---|---|---|
| **Linux / macOS** | symlink | symlink |
| **Windows, Developer Mode on** | symlink | symlink |
| **Windows, out of the box** | **junction** — never needed elevation | `@AGENTS.md` import stub |

Link support is *probed at runtime*, not inferred from the platform: two machines
running the same Windows build can differ, so agentlink tries and adapts.

## Adding a new agent

A provider is **data, not code**. Adding one is a single file:

```toml
# crates/agentlink-domain/providers/my-agent.toml
schema = 1
id = "my-agent"
name = "My Agent"

[[capability]]
resource = "skills"
strategy = "link"
path = ".my-agent/skills"
```

That is the entire change. The build embeds every manifest in
`crates/agentlink-domain/providers/` automatically — no list to update, no Rust
to write, no core to touch.

Manifests are validated at load time: a `native` claim is checked against the
canonical path, unknown keys are rejected, and paths that could escape the
workspace are refused. To try one before sending a pull request, drop it in
`.agentlink/providers/` and it takes effect immediately.

## Commands

| Command | Purpose |
|---|---|
| `agentlink init` | Create the canonical layout and materialise every capability |
| `agentlink adopt` | Move an agent's existing content into the canonical layout, then link it back |
| `agentlink apply` | Materialise anything missing or out of date (idempotent) |
| `agentlink status --check` | Exit `2` if anything is pending — for CI |
| `agentlink doctor` | Diagnose privileges, drift and stale links |
| `agentlink providers` | Show the capability matrix |
| `agentlink clean` | Remove everything agentlink created, keeping the canonical layout |

## How it treats your repository

agentlink commits **only the canonical layout** and keeps every agent-specific
path local, in a managed `.gitignore` block:

```gitignore
# >>> agentlink >>>
/.claude/skills
/.cursor/skills
/.github/skills
/CLAUDE.md
# <<< agentlink <<<
```

Committed symlinks silently degrade into plain text files for teammates on
Windows without `core.symlinks`, and junctions cannot be represented in git at
all. So each developer materialises their own — an idempotent command that takes
about 20 ms, ideal for a `post-checkout` hook. See
[ADR 0005](docs/adr/0005-git-posture.md).

## Safety

The rule the whole design enforces: **agentlink never destroys content it did not
create.**

- A lock file records everything it materialised. Nothing else is ever removed.
- No removal is recursive — it unlinks reparse points, never directory trees.
- A path holding real content is reported as `blocked`, never overwritten.
- Every blocked outcome prints the exact command that resolves it.
- `status` and `apply` render the *same* plan, so nothing happens unannounced.

## What this release does not do yet

Being explicit about the boundary, because trust matters more than surface area:

- **v0.0.1 covers instructions and skills** — the two resources where the format
  already converged, so they are fully solved by linking.
- **MCP, subagents, commands, hooks and settings are not covered yet.** These
  genuinely differ in *format* (`mcpServers` vs `servers`, `env` vs
  `environment`, JSON vs TOML), so they need transformation and a reconciliation
  model rather than a link. That is the [v0.1 milestone](ROADMAP.md).
- **Project scope only.** User-level (`~/.claude`, `~/.codex`) comes next.

## Documentation

- [Architecture](docs/architecture.md) — how the pieces fit
- [Architecture Decision Records](docs/adr/) — why, and what was rejected
- [Contributing](CONTRIBUTING.md) — including how to add an agent
- [Roadmap](ROADMAP.md)

## License

[Apache-2.0](LICENSE). Chosen over MIT for its explicit patent grant, which
matters for a project that aims to be adopted as shared infrastructure by
companies building commercial agents.
