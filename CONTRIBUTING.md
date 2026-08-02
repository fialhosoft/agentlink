# Contributing to agentlink

Thank you for helping. The single most valuable contribution to this project is
**adding or correcting an agent**, and it needs no Rust at all.

## Adding support for an agent

This is one file. Really.

Create `crates/agentlink-domain/providers/<agent-id>.toml`:

```toml
# My Agent — https://example.com/my-agent
#
# Cite the documentation that establishes each path. Paths change, and the next
# person needs to know where the claim came from.

schema = 1
id = "my-agent"
name = "My Agent"
homepage = "https://example.com/my-agent"
docs = "https://example.com/my-agent/docs/skills"

[[capability]]
resource = "skills"
strategy = "link"
path = ".my-agent/skills"
note = "Agent Skills open spec; identical SKILL.md format, different directory."
```

Then run `cargo test -p agentlink-domain` — the registry test suite parses and
validates every shipped manifest, so a mistake fails immediately.

### Choosing a strategy

| Strategy | Use when | Must satisfy |
|---|---|---|
| `native` | the tool reads the canonical path directly | `path` **must equal** the canonical path (`AGENTS.md` or `.agents/skills`) |
| `link` | same file format, different location | — |
| `import` | the tool cannot be linked but has an include syntax | `template` must contain `{canonical}` |

Add a `[capability.fallback]` with `strategy = "import"` when the resource is a
**file** and the tool has an include directive. Files cannot be junctioned, so on
Windows without Developer Mode this fallback is what keeps the capability
working:

```toml
[[capability]]
resource = "instructions"
strategy = "link"
path = "MYAGENT.md"

[capability.fallback]
strategy = "import"
template = "@{canonical}\n"
```

### Verifying a claim

Please check the behaviour rather than the blog post. The most useful thing you
can attach to a pull request is evidence:

1. Put a skill in the canonical location: `.agents/skills/hello/SKILL.md`.
2. Run `agentlink apply`.
3. Start the agent and confirm it actually discovers the skill.
4. Say so in the pull request, with the agent version you tested.

A `native` claim is checked in code against the canonical path, but no code can
check whether a tool *really* reads a directory. That part is on us as reviewers,
so evidence matters more than confidence.

### Trying it before opening a pull request

Drop the same file into your own `.agentlink/providers/`. It overrides built-ins
by id and takes effect immediately — no rebuild, no release.

## Working on the core

```console
cargo test              # unit + integration, ~1s
cargo clippy --all-targets
cargo fmt
```

### Architecture in one minute

```text
agentlink-cli     composition root, human-facing output
      ↓
agentlink-domain    pure domain — no std::fs anywhere
      ↑
agentlink-fs      adapter: symlinks, junctions, privilege probing
```

The domain never touches the filesystem. It talks to the `Workspace` trait, which
`agentlink-fs` implements for real and `testing::FakeWorkspace` implements in
memory. That is why the test suite can simulate **Windows without symlink
privileges** on a Linux CI runner, and why planning is a pure function you can
reason about.

Planning and execution are separate on purpose: `status` and `apply` render the
*same* plan, so nothing can happen that was not announced.

See [docs/architecture.md](docs/architecture.md) and the
[ADRs](docs/adr/) for the reasoning, including what was rejected and why.

### The rule that outranks everything

**agentlink never destroys content it did not create.**

Any change that could remove, overwrite or relocate a path must:

- consult the lock file to confirm agentlink created it, and
- resolve every ambiguity to a `Blocked` outcome that names the fix — never to a
  guess.

If you find yourself adding a `--force` path, that is a signal to add a
diagnostic instead. Pull requests that weaken this get a request for changes, and
it is nothing personal.

### Tests we expect

- **Domain logic** → a unit test with `FakeWorkspace`. Fast, deterministic,
  cross-platform, and able to simulate hosts your machine is not.
- **Link behaviour** → an integration test in `crates/agentlink-fs/tests/`.
- **User-visible behaviour** → an end-to-end test in
  `crates/agentlink-cli/tests/`, driving the real binary.

Tests are named as the sentence they prove — `a_hand_written_file_is_never_replaced`,
not `test_apply_3`. A failing test name should tell a stranger what broke.

## Commits and pull requests

We use [Conventional Commits](https://www.conventionalcommits.org/) — commit
subjects are for people reading the history. Provider additions are
`feat(providers): add <agent>`.

Release notes come from change files rather than commit subjects, so the prefix
carries no versioning weight. That is the next section.

Small pull requests, please. Each one should leave the project working.

## Documenting a change

**Every pull request that changes what a user sees adds a change file**, and CI
fails without one. `CHANGELOG.md` is never edited by hand; the release compiles
it from these files.

```console
knope document-change
```

That prompts for a type and a summary and writes `.changeset/<name>.md`. Writing
the file yourself is equally fine:

```md
---
default: minor
---

# `agentlink status` explains why a path is blocked

Previously a blocked path reported only that it was blocked. It now names the
file it collided with and the command that resolves it.
```

The `#` heading is what a reader sees in the release notes; the body is the
detail, and a heading with no body renders as a plain bullet. The type sets both
the changelog section and the version bump:

| Type | Section | Use for |
|---|---|---|
| `major` | Breaking changes | An existing workspace stops working, or needs a migration |
| `minor` | Added | A new command, flag, provider or capability |
| `changed` | Changed | Different behaviour that breaks nothing |
| `deprecated` | Deprecated | Still works, will be removed |
| `removed` | Removed | Gone |
| `patch` | Fixed | A bug fix |
| `security` | Security | A vulnerability or a hardening change |

We are **pre-1.0**, where semver treats the whole crate as unstable: `minor` and
below all land as a patch bump (0.0.2 → 0.0.3), and `major` moves the minor
(0.0.2 → 0.1.0). Describe the change honestly and let the version follow —
inflating the type to force a version helps nobody.

`knope prepare-release --dry-run` prints the resulting version and changelog
section without changing anything. CI runs the same command, so malformed front
matter fails on the pull request rather than at release.

Work that changes nothing observable — refactors, tests, CI, documentation —
takes the **`no changelog`** label on the pull request instead. Use it when
nobody reading the release notes would care, not when the note is inconvenient.

## How a release happens

Nothing here is manual, and no tag is ever pushed by hand:

1. Merging to `main` opens (or refreshes) a **release pull request**. It bumps
   the workspace version, compiles the pending change files into `CHANGELOG.md`
   and empties `.changeset/`.
2. Merging *that* pull request tags the release and, in the same run, builds
   every target and publishes the GitHub release, crates.io and npm.

So a release is reviewable as a diff before it happens, and a maintainer merges
exactly twice: your pull request, then the release pull request.

## Reporting a bug

`agentlink doctor` output is the most useful thing you can include: it reports
link privileges, drift and stale links, which covers most of what we would ask.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licensing

Contributions are licensed under [Apache-2.0](LICENSE), matching the project.
