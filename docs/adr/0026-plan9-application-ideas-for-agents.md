# Plan 9 Application Ideas for Agents (Acme / Plumber / 9P / Venti)

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) (the kernel
model) and [ADR-0025](0025-target-crate-architecture.md) (crate architecture).
This record decides which *ideas* — not commands — from the Plan 9 classics Alan
OS adopts, and to answer which agent question.

## Context

"Plan 9 for agents" should absorb the design ideas of Plan 9's classic
applications, not reimplement their commands. Four classics answer four agent
questions:

- **9P** → how to unify resources
- **Plumber** → how agents/tools/apps communicate
- **Venti** → how to persist knowledge
- **Acme** → how humans and agents interact

Key observation: in Plan 9 all four are *file servers* (Acme exports the editor
as a 9P tree; Plumber is `/mnt/plumb`; Venti is a content-addressed block store).
So adopting them in Alan OS means adding aP file servers plus one interaction
layer — no new mechanism beyond ADR-0024. This validates the architecture: these
classics are all expressible as aP (`alan-ap`) file servers.

The agent-side stack mirrors Plan 9's layering:

```
Acme-like editable buffer surface (acmefs)   how to interact     [deferred, M4+]
Plumber typed routing (plumbfs)              how to communicate  [adopt: add-plumber-message-routing]
aP / per-process namespace                   unify resources     [done; + network transparency]
content-addressed immutable knowledge        persist knowledge   [adopt: add-content-addressed-knowledge]
```

## Decisions

### D1. 9P → adopt the rest: network transparency (import/export)

aP already is our 9P (ADR-0024/0025). The unexploited idea is 9P's network
transparency: importing/exporting file trees across machines. For agents this is
the basis of *distributed agents*: an agent on one host imports another host's
tool tree or model Connection into its namespace, rather than calling an RPC
mesh. This is the wire-transport slice (ADR-0024 D5) given a concrete goal, and
is recorded as a requirement on `define-plan9-kernel-substrate`.

### D2. Plumber → adopt typed, rule-routed, decoupled communication

Add a plumber file server (`plumbfs`): a sender writes a typed message to a
`send` file; rule files route it by content/type to a destination port (a stream
a receiver tails). This answers how agents/tools/apps communicate without
point-to-point coupling: an agent emits "a patch" / "a citation" / "a task to
approve" and rules dispatch it (to a review agent, an apply-patch tool, or the
human inbox). Handoff stops being a hardcoded "call agent X". Governance routing
(results needing human judgment plumb to a human port) falls out naturally, and
the rules are inspectable text files.

Caveat: rule-based routing hides control flow and can hurt auditability. So plumb
messages MUST be logged to observable streams, rules MUST be `cat`-able files,
and plumbing is a *composition* mechanism, not the primary control path. Adopted
by `add-plumber-message-routing`.

### D3. Venti → adopt content-addressed, immutable knowledge (with GC)

Make agent knowledge — `machine/tape`, memory, context — content-addressed and
immutable, so:

- every tape state is a snapshot (a root hash); checkpoints are root hashes and
  forks are cheap;
- identical context / memory / documents are stored once (dedup) across agents;
- history is tamper-evident (content addressing = built-in integrity), giving a
  verifiable audit trail.

Out-of-framework payoff: cheap forking at any checkpoint enables speculative /
branching agent execution (tree search over agent states) almost for free.

Caveat: do NOT adopt Venti's "never delete". Agent token volume makes pure
immortality impractical; adopt git-style content addressing *with* reachability
GC and retention policy. Adopted by `add-content-addressed-knowledge`; this
reshapes the durable home/persistence model of ADR-0024 D7.

### D4. Acme → adopt the idea, defer the layer

Absorb two Acme ideas: (a) text is the programmable surface — any text can be
"executed", dissolving the line between reading output and issuing commands; and
(b) the interaction surface is itself a file server (`body`/`tag`/`ctl`/`addr`/
`event`), so the UI is scriptable and an agent can drive it, making humans and
agents symmetric editors of a shared text surface.

This is an editable-buffer layer above the append-only `io/` streams we have now.
It is deferred to a later interaction milestone (M4+); the north star (M0–M2)
needs only `io/` + `ctl`. Do not copy Acme's literal UI (mouse chords, no syntax
highlighting) — that is taste, not the idea. No change is created yet; this
decision is the record.

## Priority

1. **Venti / content-addressed knowledge** — highest leverage; reshapes the D7
   home/persistence foundation, so land it early before the home model sets.
2. **Plumber / routing** — cleanest additive file server; answers decoupled
   communication and human-in-the-loop governance.
3. **9P network transparency** — the distributed-agents slice; rides the wire
   transport.
4. **Acme interaction layer** — experience upgrade; M4+.

## Risks / Trade-offs

- Everything-is-a-file-server makes all four cheap to add but tempting to
  over-build. Sequence by leverage; the north star does not need any of them.
- Content addressing without GC explodes under agent volume (mitigated in D3).
- Plumber routing can hide agent behavior (mitigated in D2).
