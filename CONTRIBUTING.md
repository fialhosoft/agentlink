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

We use [Conventional Commits](https://www.conventionalcommits.org/). release-plz
derives the version bump from the prefix and opens the release PR, so it is
load-bearing. **We are pre-1.0**, where semver treats the whole crate as
unstable, so `feat` and `fix` both land as a patch bump and only a breaking
change moves the minor version:

- `feat:`, `fix:` → patch bump (e.g. 0.1.2 → 0.1.3)
- `feat!:` or a `BREAKING CHANGE:` footer → minor bump (e.g. 0.1.2 → 0.2.0)
- `docs:`, `chore:`, `refactor:`, `test:`, `ci:` → no release

Once agentlink reaches 1.0, this reverts to ordinary semver: `feat` → minor,
`fix` → patch, `feat!`/`BREAKING CHANGE` → major.

Provider additions are `feat(providers): add <agent>`.

CHANGELOG.md itself is still hand-written, under `## [Unreleased]`, as part of
the same pull request as the change — release-plz never touches it. Cutting a
release means renaming that heading to `## [x.y.z] — date` inside the release
PR before merging it.

Small pull requests, please. Each one should leave the project working.

## Reporting a bug

`agentlink doctor` output is the most useful thing you can include: it reports
link privileges, drift and stale links, which covers most of what we would ask.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licensing

Contributions are licensed under [Apache-2.0](LICENSE), matching the project.
