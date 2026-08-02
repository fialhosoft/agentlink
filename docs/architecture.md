# Architecture

## The shape of the problem

agentlink's job is not "convert format A to format B". For the resources it
covers, the format is already identical across tools — what differs is *where*
each tool looks. So the job is **placement**, and the filesystem is already very
good at placement.

That reframing is what produces an architecture with almost no moving parts.

## Layers

```text
┌──────────────────────────────────────────────┐
│ agentlink-cli                                │
│   composition root · argument parsing        │
│   human-facing rendering · exit codes        │
└───────────────────┬──────────────────────────┘
                    │ depends on
┌───────────────────▼──────────────────────────┐
│ agentlink-domain            (no std::fs)       │
│   model · path · layout · provider           │
│   registry · plan · apply · lock · gitignore │
│                                              │
│            trait Workspace  ◄────────┐       │
└──────────────────────────────────────┼───────┘
                                       │ implements
┌──────────────────────────────────────┴───────┐
│ agentlink-fs                                 │
│   symlinks · Windows junctions               │
│   runtime privilege probing                  │
│   reparse-point interpretation               │
└──────────────────────────────────────────────┘
```

Dependencies point inward. `agentlink-domain` defines the `Workspace` port and
never names a concrete filesystem; `agentlink-fs` is one adapter and
`testing::FakeWorkspace` is another.

The practical payoff is not architectural purity — it is that a Linux CI runner
can exercise "Windows without symlink privileges", a host it cannot otherwise
reproduce, and that planning decisions are unit-testable without touching disk.

## The capability lattice

Planning is a pure function of five inputs: the canonical layout, the provider
manifests, what the filesystem currently holds, what the lock says agentlink
owns, and which link primitives the host permits.

For each provider × resource pairing:

```text
                    strategy == native ?
                            │
              ┌─────────────┴─────────────┐
             yes                          no
              │                            │
    canonical exists?            pick a mechanism
       │        │                (symlink → junction
      yes      no                 → import fallback)
       │        │                         │
    Native   Skip              ┌──────────┴──────────┐
                        canonical missing?    canonical exists
                               │                     │
                     target has content?      target state?
                        │           │          │    │      │
                       yes         no       absent  link  content
                        │           │          │    │      │
              Adopt / NeedsAdopt   Skip    Create  ...   ...
```

Two properties of the target state matter most:

- **A link we own** → repoint it (`Relink`). **A link we do not own** → refuse
  (`ForeignLink`). Ownership comes from the lock file, never from a heuristic.
- **Real content at the target** → adopt if the canonical side is free,
  otherwise refuse (`TargetOccupied`). Never overwrite.

Every refusal carries the command that resolves it.

## Why execution is boring

`apply` makes no decisions. It walks the plan and performs it. That separation is
what lets `status` promise exactly what `apply` will do — they render the same
`Plan` value.

`Step` is fully self-describing, including the exact bytes of any import stub, so
the executor needs nothing but the plan. That also makes a `--json` plan a small
change rather than a redesign.

### Converging to a fixed point

`apply` re-plans after each pass until nothing more would change. This exists for
one reason: adopting content *creates* a canonical resource that did not exist
before, which unblocks every other provider that was skipped for want of anything
to share. Without the loop, onboarding would take two commands and the second
would be non-obvious. The pass limit is a bug detector, not a design parameter.

## Safety model

The invariant: **agentlink never destroys content it did not create.**

It is enforced in four independent places, so no single mistake defeats it:

1. **The planner** resolves ambiguity to `Blocked`, never to a guess.
2. **The lock file** is the sole source of ownership. `apply` only removes or
   repoints paths recorded there.
3. **The filesystem adapter** refuses `remove_link` on a path that is not a link,
   and no removal is ever recursive.
4. **`RelPath`** makes it impossible to name a path outside the workspace —
   absolute paths, drive letters, UNC prefixes and `..` are rejected at parse
   time, so community-contributed manifests are contained by construction.

## Why there is no daemon

For `native` and `link` verdicts there is exactly one inode. Propagation is not
implemented; it is what having one inode *means*. A watcher would add a process,
startup ordering, event-API differences across three platforms and a class of
race conditions — to approximate what a reparse point already does exactly.

A watcher becomes meaningful only for the `render` verdict planned for v0.1,
where a derived file genuinely can drift from its source.

## Extension points

| To add | Change | Rust needed |
|---|---|---|
| An agent | one file in `crates/agentlink-domain/providers/` | none |
| A resource kind | a `ResourceKind` variant + canonical path in `layout.rs` | small |
| A link mechanism | a `Via` variant + adapter arm | small, adapter-local |
| A materialisation strategy (e.g. `render`) | a `Strategy` variant + planner arm | the real work |
| An output format | a renderer over `Plan` | CLI-local |

The asymmetry is deliberate: the change users need most often is the one that
costs least.
