---
name: add-provider
description: Add support for a new AI coding agent to agentlink, or correct an existing agent's paths. Use when someone asks to support a tool like Windsurf, Cline, Zed, Roo Code, Amp or Goose, or reports that an agent's directories changed.
---

# Adding a provider

Adding an agent to agentlink is **one TOML file and zero lines of Rust**. The
build script enumerates `providers/*.toml` and embeds each one, so there is no
list to register in.

## 1. Find out where the agent actually reads from

Check the agent's own documentation, then **verify the behaviour** — published
docs are frequently aspirational or stale. The two questions:

- Does it read `AGENTS.md`, or its own instructions file?
- Where does it load Agent Skills (`SKILL.md` folders) from?

## 2. Write the manifest

Create `providers/<agent-id>.toml`. The id is lowercase kebab-case.

```toml
# Agent Name — https://example.com
#
# Cite the documentation establishing each path. Paths change, and the next
# person needs to know where the claim came from.

schema = 1
id = "agent-name"
name = "Agent Name"
homepage = "https://example.com"
docs = "https://example.com/docs/skills"

[[capability]]
resource = "instructions"
strategy = "native"          # this agent reads AGENTS.md directly
path = "AGENTS.md"

[[capability]]
resource = "skills"
strategy = "link"
path = ".agent-name/skills"
note = "Agent Skills open spec; identical SKILL.md format, different directory."
```

## 3. Pick the right strategy

| Strategy | Use when | Constraint enforced at load time |
|---|---|---|
| `native` | the tool reads the canonical path directly | `path` **must equal** `AGENTS.md` or `.agents/skills`, or the manifest is rejected |
| `link` | identical format, different location | — |
| `import` | cannot be linked, but the tool has an include directive | `template` must contain `{canonical}` |

Prefer `native` whenever it is true: it means agentlink writes nothing at all.

### Files need a fallback

A **file** capability cannot be served by a junction, and Windows symlinks
require Developer Mode or elevation. So if the resource is a file and the agent
has an include syntax, declare it:

```toml
[[capability]]
resource = "instructions"
strategy = "link"
path = "AGENTNAME.md"

[capability.fallback]
strategy = "import"
template = "@{canonical}\n"     # whatever include syntax the agent supports
```

Without this, unprivileged Windows users get `blocked` instead of a working
setup. Directories (skills) never need a fallback — junctions always work.

## 4. Verify

```console
cargo test -p agentlink-core registry
```

The registry test suite parses every shipped manifest and asserts its
invariants, so mistakes fail immediately rather than on a user's machine.

Then test it for real:

```console
cargo run -p agentlink-cli -- providers    # the new agent should appear
cargo run -p agentlink-cli -- status
```

Best of all: put a skill in `.agents/skills/hello/SKILL.md`, run
`agentlink apply`, start the actual agent, and confirm it discovers the skill.
Attach that result to the pull request — it is the part no automated check can
do.

## 5. Try it without rebuilding

Dropping the same file in a workspace's `.agentlink/providers/` overrides
built-ins by id and takes effect immediately. That is how users unblock
themselves when an agent changes paths upstream, and the file is exactly what
becomes the pull request.

## Commit

```text
feat(providers): add agent-name
```
