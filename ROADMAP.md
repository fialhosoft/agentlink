# Roadmap

Direction, not dates. Sequencing follows one rule: **prove the link engine on the
resources where format already converged, before taking on the ones where it has
not.**

## v0.0.1 — shipped

The full engine on the two converged resources.

- Capability lattice: `native` / `link` / `import` / `blocked`
- Symlinks, Windows junctions, runtime privilege probing
- Adoption of existing agent directories, converging in one pass
- Lock-based ownership, drift detection, managed `.gitignore`
- Six providers: Claude Code, Antigravity, Codex, Cursor, GitHub Copilot, OpenCode
- Providers as declarative manifests — adding an agent needs no code

## v0.1 — MCP, and the `render` verdict

The first resource whose **format** genuinely differs, so the first that needs
transformation rather than placement.

Every tool invented its own schema for the same protocol:

| Tool | File | Root key | Transport |
|---|---|---|---|
| Claude Code | `.mcp.json` | `mcpServers` | `type: "stdio"` |
| VS Code / Copilot | `.vscode/mcp.json` | `servers` | `type: "stdio"` |
| Cursor | `.cursor/mcp.json` | `mcpServers` | — |
| Codex | `.codex/config.toml` | `mcp_servers` | — |
| OpenCode | `opencode.json` | `mcp` | `type: "local"`, `environment` |

This requires machinery the linking verdicts did not:

- A canonical `.agents/mcp.toml` and an intermediate representation.
- A **declarative** transform vocabulary — key renaming, wrapping, enum mapping —
  keeping providers as data ([ADR 0004](docs/adr/0004-providers-as-data.md)).
- **Content-addressed provenance.** A rendered file's hash is recorded, so the
  next run can tell "unchanged since we wrote it" (safe to re-render) from "a
  human edited this" (a conflict, never silently overwritten).
- `agentlink adopt` extended to parse a rendered file back into the canonical
  representation.

Merging a rendered block into a file that also holds unrelated user settings
(`opencode.json`, `.codex/config.toml`) is the hard part, and the reason this is
not in v0.0.1.

## v0.2 — user scope

Everything today is project-scoped. Personal configuration is at least as
duplicated: `~/.claude/skills`, `~/.codex/skills`, `~/.gemini/config/skills`,
`~/.cursor/skills`.

- A canonical `~/.agents/` with the same four verdicts
- Precedence rules where project and user scope overlap
- `agentlink apply --user`

## v0.3 — the remaining resources

In descending order of how much the format has converged:

- **Subagents** — Claude Code `.claude/agents/*.md`, OpenCode `.opencode/agents/`.
  Markdown with frontmatter in both; the frontmatter keys differ.
- **Commands / prompts** — Claude Code `.claude/commands/`, Cursor
  `.cursor/commands/`, Copilot `.github/prompts/*.prompt.md`, Gemini CLI
  `.gemini/commands/*.toml`.
- **Hooks and permissions** — the least converged and most security-sensitive,
  since these describe code execution. Deliberately last.

## Beyond

- **More providers.** Windsurf, Cline, Roo Code, Zed, Amp, Goose, Junie, Kiro,
  Amazon Q, Aider, Continue, Jules. Each is one file, and each is a good first
  contribution.
- **`agentlink watch`.** Only meaningful for `render` targets; linked and native
  ones need no watching, ever.
- **A `--json` plan.** Machine-readable output so other tools and agents can
  consume a plan. `Step` is already fully self-describing for this.
- **An agentlink MCP server.** Let an agent query and modify the shared brain
  through a tool call rather than through file paths.
- **Editor integrations.** Deliberately thin: convenience over the CLI, never a
  second source of truth.
- **A conformance suite.** Fixtures asserting where each agent actually reads
  from, so provider claims are testable by anyone.

## Explicit non-goals

- **Becoming a configuration format.** The canonical layout is the community
  standard. agentlink adds no schema of its own to a space that just finished
  converging.
- **Content opinions.** agentlink places files; it does not lint, rewrite or
  grade your instructions.
- **A daemon.** For linked resources the filesystem already does this, correctly,
  with nothing running.
- **Being the only way in.** `agentlink clean` must always return a repository
  that works without it.
