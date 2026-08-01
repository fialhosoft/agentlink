# Support

## Before anything else

```console
agentlink doctor
```

It reports link privileges, missing canonical paths, drift, stale links and
pending work — which covers most questions, and is the first thing we would ask
for anyway.

## Common situations

**"My agent does not see the skills."**
Run `agentlink status`. If the agent shows `native`, agentlink writes nothing by
design because that tool reads `.agents/skills` directly — the problem is
elsewhere. If it shows `blocked`, the message names the exact command to run.

**"On Windows, `CLAUDE.md` is a stub instead of a link."**
Expected without Developer Mode: files cannot be junctioned and symlinks need a
privilege. The `@AGENTS.md` stub is Claude Code's own supported import syntax and
works identically. Enabling Developer Mode and re-running `agentlink apply` is
optional, not required.

**"A teammate cloned the repo and nothing is linked."**
By design — see [ADR 0005](docs/adr/0005-git-posture.md). They run
`agentlink apply` once, or you add it to a `post-checkout` hook. Most agents read
a fresh clone natively regardless, since the canonical layout is the standard.

**"I moved the repository and links broke."**
Windows junctions store an absolute path. `agentlink doctor` flags these as stale
and `agentlink apply` rebuilds them.

**"My agent is not supported."**
Adding one is a single TOML file — see [CONTRIBUTING.md](CONTRIBUTING.md). You
can use it immediately by dropping it in `.agentlink/providers/`.

## Asking a question

- **Questions and ideas** → [GitHub Discussions](https://github.com/agentlink-dev/agentlink/discussions)
- **Bugs and agent support requests** → [GitHub Issues](https://github.com/agentlink-dev/agentlink/issues)
- **Security** → see [SECURITY.md](SECURITY.md); please do not use public issues

This is a volunteer-maintained project. Issues are usually triaged within a week.
