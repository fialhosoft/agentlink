# ADR 0006 — Rust core, distributed through npm

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

agentlink's audience overwhelmingly has Node installed — the AI coding tool
ecosystem is largely JavaScript-adjacent, and `npx` is how such tools are
expected to be tried. That argues for TypeScript.

Against that: agentlink's core competency is **reparse points, link semantics and
platform privileges**, and it is expected to run in a git hook, where process
startup is a visible cost.

Crucially, [ADR 0004](0004-providers-as-data.md) makes providers *data*. So the
usual decisive argument for a mainstream language — "contributors must be able to
add support for their agent" — does not apply: adding an agent is a TOML file
regardless of the core's language.

## Decision

Write the core in **Rust** and distribute it through **npm** (per-platform
optional dependencies carrying a prebuilt binary), alongside Homebrew and
`cargo install`.

## Consequences

**Correctness where it matters most.** Windows link handling is the part most
likely to harbour subtle bugs, and Rust's explicit error handling and
`#[cfg(windows)]` boundaries keep the platform-specific surface small, visible
and confined to one crate.

**Fast enough for a git hook.** A full `agentlink status` over twelve
capabilities measures ~18 ms end to end on Windows, of which process startup is a
small fraction; an equivalent Node process spends roughly 80 ms before running
any of its own code. Since the recommended posture
([ADR 0005](0005-git-posture.md)) is to run `apply` on `post-checkout`, that
difference is paid on every branch switch.

**No runtime dependency.** agentlink works in a repository with no Node, no
`node_modules`, and in minimal containers.

**Reach is preserved.** The npm wrapper means `npx agentlink init` still works,
which is the discovery path this audience actually uses. This is the established
pattern for Rust CLI tools with JavaScript-ecosystem audiences — esbuild, swc,
Biome and Rolldown all ship this way.

**`#![forbid(unsafe_code)]` in every crate.** Junction creation, the one
operation requiring raw Windows APIs, is delegated to the `junction` crate rather
than hand-rolled, keeping the entire workspace free of `unsafe`.

**The core contributor pool is smaller.** This is the real cost. It is mitigated
by providers being data, by an architecture where the domain has no I/O and is
testable without a filesystem, and by keeping the crate count and dependency list
small (`serde`, `toml`, `thiserror`, `clap`, `anyhow`, `junction`).

**Release engineering is heavier.** Cross-compiling six targets and publishing to
three registries is more CI than `npm publish`. It is automated once and then
amortised.

## Alternatives considered

**TypeScript + Node.** The strongest alternative, and it would have reached v0.1
sooner. Notably, Node *can* create Windows junctions via
`fs.symlinkSync(target, path, 'junction')`, which weakens the strongest technical
argument for Rust. It was rejected on startup cost in a git hook, the runtime
dependency, and weaker ergonomics for inspecting reparse points — but it was a
close decision, not an obvious one.

**Go.** A reasonable middle ground: single binary, fast startup, larger
contributor pool than Rust. Rejected because `os.Symlink` does not create
junctions, so the single most important Windows capability would require
hand-written `DeviceIoControl` / `FSCTL_SET_REPARSE_POINT` syscalls — precisely
the code most likely to be subtly wrong, in the language offering the least help
in getting it right.
