# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Entries below v0.1.0 describe a pre-release API that may change.

## [Unreleased]

### Added

- **`agentlink init` asks which agents the repository uses.** A multi-select
  preselects the agents with content already in the repository, and the answer is
  saved as an explicit `providers` list — so a repository using two agents no
  longer grows directories for six. `agentlink providers --select` reopens the
  choice. See [ADR 0006](docs/adr/0006-provider-selection.md).
- **`init --providers claude-code,antigravity`** sets the list without a prompt.
  Nothing is ever asked when stdin or stdout is not a terminal, so scripts and CI
  keep serving every agent exactly as before.
- **`retire` verdict.** Dropping an agent from the list removes what agentlink
  created for it, announced by `status` before `apply` performs it. A path that
  no longer matches what agentlink created is kept and simply disowned.

## [0.0.1] — 2026-08-01

First release. Shares **instructions** and **skills** across six AI coding agents
without copying a single file.

### Added

- **Capability lattice.** Each provider/resource pairing resolves to `native`
  (the tool already reads the canonical path — nothing is written), `link` (a
  symlink or junction), `import` (a one-line include stub), or `blocked` (needs a
  human decision).
- **Canonical layout** of `AGENTS.md` and `.agents/skills`, matching the
  community standards rather than inventing one, so most pairings cost nothing.
- **Windows support without elevation.** Directory junctions need no privilege;
  symlink availability is probed at runtime rather than assumed, and files
  degrade to an import stub where linking is impossible.
- **`agentlink adopt`.** Moves an existing agent directory into the canonical
  layout and links it back, converging in a single pass so every other agent is
  served immediately.
- **Lock-based ownership.** agentlink removes or repoints only what it created;
  everything else is reported, never touched.
- **Managed `.gitignore` block** that preserves surrounding content and the
  file's existing line endings.
- **Providers as declarative TOML manifests**, embedded at build time. Adding an
  agent is one file and no code. Workspace-local manifests in
  `.agentlink/providers/` override the built-ins.
- **Commands:** `init`, `apply`, `status`, `adopt`, `doctor`, `providers`,
  `clean`. `status --check` exits `2` when work is pending, for CI.
- **Providers:** Claude Code, Google Antigravity, OpenAI Codex CLI, Cursor,
  GitHub Copilot, OpenCode.

### Security

- Manifest paths are parsed into a type that rejects absolute paths, drive
  letters, UNC prefixes and `..`, so a manifest cannot address anything outside
  the workspace.
- `#![forbid(unsafe_code)]` across every crate.
- No network access, no code execution, no telemetry.

[Unreleased]: https://github.com/fialhosoft/agentlink/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/fialhosoft/agentlink/releases/tag/v0.0.1
