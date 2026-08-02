# ADR 0004 — Providers are declarative data, not code

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

New AI coding agents appear constantly, and existing ones change their paths. The
rate at which agentlink can absorb that change *is* the product. If adding an
agent means writing Rust, the contributor pool is limited to people who know
Rust — a small fraction of the people who know that a given agent reads a given
directory.

This also bounds the language choice ([ADR 0006](0006-rust-with-npm-distribution.md)):
if providers required code, picking Rust would be a serious adoption cost.

## Decision

A provider is a TOML manifest in `crates/agentlink-core/providers/`. The build
script enumerates that directory and embeds every manifest via `include_str!`,
so there is no list to update anywhere.

The directory lives inside the `agentlink-core` crate rather than at the
workspace root — a constraint discovered when the first `cargo publish` failed:
`cargo` packages only files under a crate's own root, so a workspace-sibling
`providers/` built fine from a git checkout but produced a broken tarball on
crates.io. See the comment in `build.rs` for the detail.

```toml
schema = 1
id = "claude-code"
name = "Claude Code"

[[capability]]
resource = "instructions"
strategy = "link"
path = "CLAUDE.md"

[capability.fallback]
strategy = "import"
template = "@{canonical}\n"
```

Adding an agent is **one file**. No Rust, no registration, no core change.

Manifests may also be dropped into a workspace's `.agentlink/providers/`, where
they override built-ins by id.

## Consequences

**The contribution funnel is as wide as it can be.** The barrier to contributing
is knowing where an agent stores its skills — which is exactly the knowledge that
is distributed across the community and scarce in any single maintainer.

**The core stays closed to modification and open to extension.** Adding providers
touches no logic, so provider growth cannot destabilise the planner.

**Validation must be strict, because manifests are untrusted community data.**
Enforced at load time:

- `deny_unknown_fields` — a typo like `pathh` fails loudly instead of silently
  doing nothing, which would leave a user believing they were covered.
- `native` paths must equal the canonical path, so the claim cannot be wrong.
- Paths are parsed into `RelPath`, which rejects absolute paths, drive letters,
  UNC prefixes and `..`. A manifest **cannot** address anything outside the
  workspace. This is a security property, not a convenience.
- An `import` template must reference `{canonical}`, or the stub would be inert.
- Provider ids must be lowercase kebab-case, so they are stable in lock files.

Every shipped manifest is parsed and asserted in the registry test suite, so an
invalid contribution fails CI rather than a user's machine.

**Users are never blocked on a release.** When an agent changes its paths
upstream, a workspace-local manifest fixes it the same day — and that file is
precisely what becomes the pull request.

**Expressiveness is bounded on purpose.** A declarative manifest cannot express
an arbitrary format transformation. That is acceptable while every capability is
`native`, `link` or `import`. The `render` verdict planned for v0.1 (MCP) will
need a declarative transform vocabulary — key renaming, wrapping, enum mapping —
with a plugin escape hatch only if real cases demand it. Reaching for a general
scripting escape hatch first would trade away the property this ADR exists to
protect.

## Alternatives considered

**A trait implemented per provider in Rust.** Rejected: maximally expressive and
minimally contributable. It would put the project's growth rate behind a Rust
review queue.

**Fetching manifests from a registry at runtime.** Rejected for v0.0.1. It would
add a network dependency, a trust boundary and a supply-chain surface to a tool
that otherwise only touches local files. Embedding keeps the binary hermetic and
auditable. A signed, opt-in manifest channel is a plausible later addition once
provider churn actually outpaces releases — the workspace-local override covers
the urgent case in the meantime.

**JSON or YAML instead of TOML.** TOML chosen for comment support (manifests
carry citations to the upstream documentation that justifies each path), a
readable array-of-tables syntax for capabilities, and consistency with the Rust
ecosystem the project already lives in.
