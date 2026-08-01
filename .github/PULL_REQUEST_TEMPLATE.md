<!--
Thank you. Please use a Conventional Commit title, e.g.
  feat(providers): add windsurf
  fix(fs): rebuild stale junctions after a workspace move
-->

## What this changes

<!-- One or two sentences. -->

## Why

<!-- The problem being solved. Link an issue if there is one. -->

---

### If this adds or changes a provider

- [ ] The manifest cites the upstream documentation for each path
- [ ] Any `native` capability points at the canonical path (`AGENTS.md` / `.agents/skills`)
- [ ] File capabilities that could not be linked declare an `import` fallback
- [ ] **Verified against the real agent** — put a skill in `.agents/skills/`, ran
      `agentlink apply`, and confirmed the agent discovers it

Agent and version tested:

### If this touches the core

- [ ] `cargo test` and `cargo clippy --all-targets` pass
- [ ] Domain changes are covered by a `FakeWorkspace` unit test
- [ ] Link behaviour changes are covered by a real-filesystem integration test
- [ ] Nothing can remove, overwrite or relocate a path unless the lock file says
      agentlink created it
- [ ] New ambiguity resolves to a `Blocked` outcome that names the fix, not a guess
