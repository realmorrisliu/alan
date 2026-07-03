# North Star Capability Map (Five Rings)

Status: Accepted. Consolidates [ADR-0024](0024-plan9-kernel-model.md) (kernel
model), [ADR-0025](0025-target-crate-architecture.md) (crate architecture),
[ADR-0026](0026-plan9-application-ideas-for-agents.md) (application ideas), the
product constitution (`programmable-environment-product`, archived 2026-06-26),
and the agent file-layout contract into one end-state capability map. It
introduces one new decision (D1: tape records stay content-addressable-ready);
everything else is a synthesis with explicit sequencing warnings.

## Context

The north-star vision is spread across three ADRs, an archived product
constitution, a file-layout contract, and several in-flight OpenSpec changes.
Each records its own slice; none shows the whole. That makes two failure modes
easy: treating far-ring vision as near-term runway, and making near-term
implementation choices that foreclose far-ring capabilities. This record fixes
the full map and the three places where the sequencing bites.

## The thesis

**Alan OS turns a live agent's cognition into a mountable file tree, and then
collapses using it, inspecting it, governing it, extending it, and distributing
it into file operations.**

The genuinely new claim (vs Emacs, Acme, or any agent framework): **one
namespace = runtime = UI = plugin API**, where the malleable object is not text
but a running agent's cognition (tape / memory / model stream / tools as
files). The iron law that makes the claim falsifiable: every behavior change or
extension MUST be expressible as a file operation or a mount/interpose on the
namespace — never a side-channel API. If a feature needs a non-file API, the
thesis is leaking.

## The five rings

```
                  ┌──────────────────────────────────────────────┐
                  │  Ring 5 · Ecosystem: Alan Apps are real       │
                  │  products (Groove Master, UPDF): bounded      │
                  │  descriptors + spawn; no embedded chatbot,    │
                  │  no RPC agent API                             │
               ┌──┴──────────────────────────────────────────┐   │
               │  Ring 4 · Interaction: the Acme layer (M4+,   │   │
               │  deliberately deferred): text is executable;  │   │
               │  the UI is itself a file server; humans and   │   │
               │  agents are symmetric editors                 │   │
            ┌──┴──────────────────────────────────────────┐  │   │
            │  Ring 3 · Composition: the four classic       │  │   │
            │  ideas (ADR-0026): Venti → content-addressed  │  │   │
            │  knowledge · Plumber → routefs · 9P network   │  │   │
            │  transparency → distributed agents            │  │   │
         ┌──┴──────────────────────────────────────────┐  │  │   │
         │  Ring 2 · Cognition as files (current work):  │  │  │   │
         │  generation = reading a stream · tools =      │  │  │   │
         │  /bin · state = /agent/<pid> · capability =   │  │  │   │
         │  the mount set · governance = file access     │  │  │   │
         │  + ctl                                        │  │  │   │
      ┌──┴──────────────────────────────────────────┐  │  │  │   │
      │  Ring 1 · Substrate (built): aP protocol ·    │  │  │  │   │
      │  namespace engine · /proc /srv · MountFs      │  │  │  │   │
      └───────────────────────────────────────────────┘  │  │  │   │
         └───────────────────────────────────────────────┘  │  │   │
            └───────────────────────────────────────────────┘  │   │
               └───────────────────────────────────────────────┘   │
                  └─────────────────────────────────────────────────┘
```

- **Ring 1 — Substrate.** Shipped: `alan-ap`, `alan-kernel` (namespace,
  process table, `/proc`, `/srv`), `MountFs` as the single root handle.
- **Ring 2 — Cognition as files.** In flight:
  `refactor-engine-namespace-native` (slices A/B/C; finish line = the
  `RuntimeEnvironment::Legacy` variant is deleted). This ring is the whole
  current bet; every other ring assumes it.
- **Ring 3 — Composition.** Chartered but barely started:
  `add-content-addressed-knowledge` (Venti idea), `add-message-routing`
  (Plumber idea), aP wire transport (9P network transparency).
- **Ring 4 — Interaction.** Owned by
  `define-editable-buffer-interaction`: an editable-buffer layer above `io/`
  where any text can be executed and the UI surface is itself a file server.
  Absorb the idea, not Acme's literal UI.
- **Ring 5 — Ecosystem.** Chartered in the product constitution: apps get AI
  by opening bounded descriptors and spawning agent processes.

## End-state capabilities

For agents:

- **Capability = the mount set.** What an agent can do is exactly what its
  spawner mounted. A withheld tool is not "denied by policy" — it is
  structurally absent from the agent's world. Auditing an agent's authority is
  `ls` on its namespace.
- **Inspectable cognition.** `cat machine/tape` for the thinking history,
  `tail io/output` for what it is saying, `ls requests/` for what it is
  waiting on. No state hides behind in-process private objects.
- **Cheap forking of thought** (Venti idea). Tape states are content-addressed
  snapshots; a checkpoint is a root hash; forking is nearly free — speculative
  and branching execution (tree search over agent states) becomes a first-class
  operation. This is the map's most beyond-convention capability.
- **Decoupled communication** (Plumber idea). An agent emits "a patch" /
  "a task to approve"; `cat`-able rule files route it to a review agent, an
  apply tool, or the human inbox. Handoff stops being a hardcoded "call agent
  X"; human-in-the-loop governance falls out of routing rules.
- **Distributed agents** (9P idea). Import another host's tool tree or model
  Connection into the local namespace; cross-machine collaboration without an
  RPC mesh.

For the professional user:

- **Using = reprogramming** (the iron law). Intercept every tool call by
  interposing a file server on `/bin`; swap models by remounting `/mnt/llm`;
  extend in any language via Rust-native or WASM file servers.
- **Every client is just a file reader.** The TUI, the macOS shell, a shell
  script, and another agent are symmetric; `echo ... > io/input` is a
  conversation.
- **Governance as file semantics.** Interrupt via `/proc/<pid>/ctl`;
  compact/rollback via `machine/ctl`; answer via `requests/<id>/response`
  committed on clunk; the GENERATING exclusive-write lease makes a Yield a safe
  human-edit window (all per `define-agent-file-layout-contract`).
- **Tools are the same for humans and agents.** `/bin` executables with
  `--help`, `/man` pages, and machine-readable manifests.
- **Tamper-evident history.** Content addressing gives the audit trail
  built-in integrity.

Product level: data stays where it lives (mounts, not imports); local-first;
Rust core + WASM Component Model extensions; apps are real products, not
demos.

## Decisions

### D1. Tape records stay content-addressable-ready from the first engine write

ADR-0026 ranks the Venti idea first *because it reshapes the D7 home/
persistence model and must land before the home model sets*. The engine-native
rewrite (Ring 2, Slice A) is about to make the engine write `machine/tape`
directly — which is precisely the moment the tape's storage shape sets.

Therefore: the Slice A tape write format MUST NOT foreclose content
addressing. Concretely, tape records are self-contained, append-only,
canonically serializable units whose identity does not depend on file offsets
or mutable in-place state — so that `add-content-addressed-knowledge` can later
address them by hash without a migration rewrite. Hash-addressed storage
itself is NOT built in Slice A; only the record shape is constrained.

### D2. The rings are a map, not a runway

ADR-0026 is explicit: *the north star does not need any of the four classic
ideas*. M0–M2 need only `io/` + `ctl`. Ring 3–5 work is sequenced by leverage
after Ring 2's finish line (delete `RuntimeEnvironment::Legacy`), and no Ring
3–5 change may be used to justify delaying that finish line. Everything-is-a-
file-server makes far-ring capabilities cheap to start and therefore tempting
to over-build; the priority order stands: content-addressed knowledge →
message routing → network transparency → Acme layer.

### D3. State the isolation boundary honestly, everywhere

"Capability = the mount set" is convention-enforced until the kernel §7.1a
amplification check lands (ADR-0024 R1) — an architectural discipline, not a
security property. Additionally, native subprocesses (e.g. `bash`) cannot see
the namespace at all; OS-level confinement (the `SandboxSpec` projection track,
`define-namespace-driven-sandbox`) remains the second enforcement mechanism
permanently, not transitionally. Any user-facing or spec text describing the
capability model MUST carry this qualification until the amplification check
exists.

## Non-goals (deliberate, part of the north star)

- **No mass-market UI.** Pro users only for now; expose the raw namespace;
  accept Emacs-style deep-and-narrow.
- **No RPC platform.** A non-file agent API is compatibility transport at
  most, never the canonical boundary.
- **No proprietary object store.** Files, mounts, and projections; source
  data is not forced into Alan.
- **No literal Acme UI.** Mouse chords and no-syntax-highlighting are taste,
  not the idea; the interaction soul is keyboard-first because the flagship is
  a TUI.
- **No "never delete".** Content addressing comes with reachability GC and
  retention policy (ADR-0026 D3 caveat), not Venti's immortality.

## Risks / Trade-offs

- **Ring 2 is a single point of failure.** Every capability above assumes the
  engine is namespace-native. Mitigation: the three-slice plan with a
  structural finish line (see `refactor-engine-namespace-native` design.md,
  Delivery Plan).
- **D1 is a bet on record granularity.** Constraining tape records now costs
  little, but if content addressing later wants a different unit (e.g.
  chunk-level dedup), the record shape may still need rework. Accepted: the
  cheap hedge beats re-pouring the foundation.
- **Routing and interposition can hide control flow.** Carried forward from
  ADR-0026 D2's caveat: routed messages log to observable streams; rules are
  `cat`-able; interposition is inspectable in the namespace.

## References

- ADR-0024 (kernel model), ADR-0025 (crate architecture), ADR-0026
  (application ideas and their priority order).
- Product constitution: `openspec/changes/archive/2026-06-26-define-
  programmable-environment-product/specs/programmable-environment-product/`.
- File surface + access discipline: `define-agent-file-layout-contract`.
- Ring 2 delivery plan: `openspec/changes/refactor-engine-namespace-native/
  design.md` ("Delivery Plan", 2026-07-02).
