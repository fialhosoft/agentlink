# ADR 0006 — Serve the agents a repository chose, and retire the ones it drops

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

v0.0.1 served every provider agentlink knew about. A team using Claude Code and
Antigravity still got `.cursor/skills`, `.github/skills` and `.opencode/skills`
materialised, listed in `.gitignore`, and reported on every `status`.

The defence was that a link is nearly free. That is true of bytes and false of
attention: a directory nobody reads is still a directory reviewers ask about, and
a `.gitignore` block naming four agents the team does not use misrepresents the
repository. `.agentlink/config.toml` already had a `providers` key that fixed
this, and nothing ever wrote it — a setting that exists only for people who read
the source is not a feature.

Deselecting also has to mean something. A provider list that only ever adds is a
list you cannot correct: narrow it and the previous run's junctions stay behind,
now maintained by nobody.

## Decision

**`agentlink init` asks.** A multi-select lists every known agent, preselecting
the ones with evidence in the repository — a path the provider owns that already
exists. The answer is written as an explicit `providers` list.

Evidence is deliberately narrow. A `native` capability reads the canonical
layout, so `AGENTS.md` existing says nothing about whether anyone here runs
Codex; preselecting on it would check every native agent for everybody. Fully
native agents are therefore never preselected, and the picker labels them
"costs nothing" so the user can see that checking one is free.

**Nobody is ever asked in a script.** The prompt requires both stdin and stdout
to be terminals. Without them — CI, a pipe, `--quiet` — the key stays unset,
which continues to mean *every agent, including ones future releases add*.
`--providers a,b` sets the list non-interactively, and `providers --select`
reopens the choice later.

**Deselecting retires.** The lattice gains `Retire`: an artefact whose provider
is no longer served, which the lock claims and which still matches what we
created, is removed. Retirement is planned, not executed on the side, so `status`
announces it before `apply` performs it.

Where the artefact has diverged — a link replaced by a real directory, a stub
someone edited — the verdict is `Skip::Unmanaged`: the content stays and
agentlink drops its claim instead. This keeps the rule that outranks everything
(never destroy content agentlink did not create) and, by releasing the claim,
stops the tool reporting a path it no longer has any say over.

## Consequences

**The repository is as wide as the team, not as wide as the registry.** This is
the visible payoff: no directory appears for an agent nobody uses.

**An explicit list freezes out future agents.** Someone who selected today will
not silently gain a provider added in a later release. That is the correct
reading of an explicit choice, and `providers --select` shows the newcomers.

**Selection is a shared decision.** `config.toml` is committed, so a teammate's
`apply` serves the same agents. The lock, which is per-machine, is not.

**A new verdict means a new way to be wrong.** `Retire` is the first outcome that
removes something, so its safety test is the same one `clean` uses, and it is
covered from both sides: the link we made is retired, the directory the user put
there survives.

## Alternatives considered

**Keep serving everything; let users run `clean`.** Rejected: `clean` is
all-or-nothing, so the only way to drop one agent was to remove every link and
re-materialise. That is not a setting anyone would use.

**Preselect every agent and let the user uncheck.** Rejected: it makes the
zero-effort answer the noisy one, which is the behaviour this ADR exists to
change. Preselecting all remains the fallback when nothing is detected, so a
fresh repository still behaves as v0.0.1 did.

**Compute retirements in the CLI rather than the planner.** Rejected: `status`
and `apply` render the same plan, and a removal that only `apply` knew about
would break the promise that nothing happens without being announced first.

**Hand-roll the prompt on `stdin().read_line()`.** Rejected. The dependency
budget is deliberately small, but arrow keys, checkboxes and terminal state
across Windows and POSIX are exactly the kind of thing to take from a library
(`dialoguer`, default features off).
