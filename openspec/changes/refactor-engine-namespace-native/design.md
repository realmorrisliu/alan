## Context

`alan-agent-engine` (formerly `alan-runtime`) implements the agent Turing-machine
loop in `runtime/agent_loop.rs` (`RuntimeLoopState`). Today it is wired to the
pre-Plan-9 worldview on three axes:

- **Transition function**: a `Box<dyn LlmProvider>` whose `generate_stream` is
  called directly.
- **Side effects**: an in-process `ToolRegistry` invoked as Rust functions.
- **State publication**: emission of the `alan-agent-protocol`
  `Event`/`EventEnvelope` alphabet, consumed by transports/UIs.

The substrate already exists and is pure: `alan-ap` (protocol + `Stream`),
`alan-kernel` (`/proc` + `/proc/clone` spawn + `/srv` + namespace engine),
`alan-llmfs` (Generation as a clone-via-open `data`→`events` directory). The
first projection slices added two adapters — `FileLlmProvider` (files →
`LlmProvider` → engine) and `agentfs` (engine events → files) — that preserve the
old abstractions instead of replacing them. This change removes that bridging and
makes the engine read and write its namespace directly.

## Goals / Non-Goals

**Goals:**

- The engine's only environment is a namespace handle (an aP client over its
  mounted root); generation, tools, and state are file operations on it.
- An agent's capability set is exactly what its spawner mounts (D6): no injected
  `Box<dyn>` capabilities.
- Generation reads `/mnt/llm/connections/<conn>` (clone → `data` → `events`).
- Tools are `/bin` executables spawned via `/proc/clone`; results are read from
  the tool process's files.
- Agent state is written to `/agent/<pid>` files (`io/output`, `machine/tape`,
  `requests/`, `actions/`); these files are the source of truth.
- M2 falls out: shell writes `io/input`; the agent reads it, generates via
  `/mnt/llm`, writes `io/output`; the shell tails it.

**Non-Goals:**

- Keeping `LlmProvider` or `EventEnvelope` on the engine's live path (they remain
  only as legacy compatibility transport behind file servers, ADR-0025 D4).
- Rewriting provider wire adapters in `alan-llm` (the HTTP drivers stay; they are
  reached *through* `alan-llmfs`, not by the engine).
- A cross-process aP wire transport (still a later slice; v1 in-process).
- The full `add-llm-file-server` surface (provider introspection, metering) — the
  minimal Generation slice in `alan-llmfs` is sufficient.

## Decisions

### D1. The engine's environment is one namespace handle

`RuntimeLoopState` stops holding `provider` and `tools`. It holds an aP client
over its mounted root (e.g. `alan_ap::InProcessTransport` to the assembled
namespace, or an `alan-kernel` namespace). Everything the agent can do is reached
by walking paths in that root. *Alternative considered*: keep `provider`/`tools`
as fields but back them with file clients — rejected, that is the adapter we are
deleting; it leaves the old interfaces as the engine's mental model.

### D2. Generation is a file operation, not a trait call

The generation step opens `/mnt/llm/connections/<conn>/clone`, writes the request
document to `data`, and reads token records from `events` (the exact protocol
`alan-llmfs` already serves and `FileLlmProvider` already implements as a
client). That client logic moves *into* the engine's transition step; the
`LlmProvider` trait disappears from the call path and `alan-llmfs-client` is
retired. *Alternative*: keep `FileLlmProvider` — rejected (it is the bridge).

### D3. Tools are executables spawned via `/proc/clone`

A tool call becomes: spawn `/bin/<tool>` via `/proc/clone` (exec spec carries
args + the child's namespace), then read the tool process's output/result files
and project them into `actions/<id>/`. The in-process `ToolRegistry` call path is
removed. *Alternative*: keep `ToolRegistry` behind a file facade — rejected, same
bridge anti-pattern. This is the heaviest step (registry is deeply embedded).

### D4. State is written to `/agent/<pid>` files; agentfs is their backing

The engine writes `io/output`, `machine/tape`, `requests/<id>/`, `actions/<id>/`
as the source of truth. `alan-agentfs` is re-aimed from translating
`EventEnvelope`s to serving these engine-written files. The `EventEnvelope`
emission path is removed from the live model. *Alternative*: keep event emission
and let agentfs translate — rejected, the alphabet is the demoted legacy
transport, not the publication mechanism.

### D5. An agent is a `Process` with a spawner-assembled namespace

Spawning an agent is `/proc/clone` with an exec spec whose namespace mounts the
permitted llm connection and tools. The mounted set *is* the capability set; a
withheld model is simply not mounted (D6). This unifies "what can this agent do"
into one inspectable place — its namespace.

### D6. Stage as pure replacements, not parallel bridges

Each step deletes the old path as it lands rather than running both behind a flag.
Ordering: D1 (environment) → D2 (generation, unlocks M2 spine) → D4 (state files)
→ D3 (tools, heaviest) → D5 (spawn/namespace assembly). M2 is reachable after
D1+D2+D4. *Alternative*: feature-flag old vs new — rejected for the live model
(a flagged bridge is still a bridge); test doubles (in-memory aP servers) provide
the safety net instead.

### Access discipline is owned by the agent file-layout contract

The namespace's access discipline — `ctl` role/scoping, the `machine/tape`
generation lease, append-only tape/events, actor-keyed write authority, the
interpose extension seam, the external-writer protocol-layer prerequisite, and
self-description — is **not** specified here. It is consolidated in
`define-agent-file-layout-contract` (the single authoritative file-surface
contract). This change implements the engine *against* that contract; it does not
redefine the file surface.

Earlier drafts of this change carried a conflicting D7 shape (answer via
`requests/<id>/ctl`, lifecycle verbs on `machine/ctl`). That shape is superseded:
answering is a `requests/<id>/response` write committed on clunk, generic
lifecycle control is the kernel `/proc/<pid>/ctl`, and `machine/ctl` carries
agent-runtime tape/checkpoint commands (`compact`/`rollback`).

## Risks / Trade-offs

- **Largest existing crate rewrite; deep test coupling** → Stage per D6; keep
  each step behind TDD against in-memory aP servers (the `alan-ap` reference
  `MemFs`, `alan-llmfs` mock-backed) so behavior is pinned before deleting old
  code. Land as its own PR stack.
- **Tool rewrite (D3) is the deepest** → Sequence it after M2 (D1+D2+D4) is
  green, so a talking agent exists before the registry is dismantled; convert one
  tool end-to-end first to prove the `/bin` + `/proc/clone` path.
- **Losing event-stream consumers (current TUI/daemon)** → those become file
  readers (already the direction in `alan-shell` and the agentfs rework);
  `EventEnvelope` stays available as legacy transport during migration, but off
  the engine's live path.
- **Capability assembly correctness (D5/D6)** → ties into the kernel's deferred
  §7.1a amplification check; until that lands, the boundary is convention-
  enforced (ADR-0024 R1), which this change must restate, not claim hard
  isolation.
- **Access discipline could balloon the rewrite** → the discipline now lives in
  `define-agent-file-layout-contract`, not here. Of it, only the GENERATING
  `machine/tape` exclusive-write lease is load-bearing for M2 (the moment a live
  writer exists) and must be honored by the engine's writes; the human-edit and
  interpose surfaces are deferred follow-ons and do not block the talking-agent
  spine.

## Migration Plan

1. Land this change's spec deltas; mark `alan-llmfs-client` (#579) for retirement
   and `alan-agentfs` (#576) for rework in their PRs.
2. D1: introduce the namespace-handle environment on `RuntimeLoopState` (no
   behavior change yet beyond the seam), TDD against an in-memory root.
3. D2: replace the generation call site with file ops on `/mnt/llm`; delete the
   `LlmProvider` call path; retire `alan-llmfs-client`.
4. D4: write `/agent/<pid>` state files from the engine; re-aim agentfs; remove
   the `EventEnvelope` publication path.
5. Reach M2: wire `alan-shell` `io/input`/`io/output` to a spawned agent process;
   prove a real conversation through files end-to-end.
6. D3: convert tools to `/bin` executables + `/proc/clone`; remove `ToolRegistry`
   from the call path.
7. D5: assemble agent namespaces at spawn; tie capabilities to mounts.

## Delivery Plan (re-planned 2026-07-02)

### The finish line is structural, and the progress bar is a grep

Done means **`RuntimeEnvironment::Legacy` is deleted from `agent_loop.rs`**.
Until then the engine has two worlds, and everything that makes it the deepest
crate (compaction, memory flush, guardrails, virtual tools, child agents) runs
only in the Legacy one. Progress is therefore measured by the production
call-site count of the legacy accessors, not by checkbox completion:

- `.llm_client()` / `.llm_client_mut()` — 21 sites at re-plan time
  (turn_executor 15, compaction 2, memory_flush 2, agent_loop 1,
  tool_orchestrator 1) → 0.
- `.tools()` / `.tools_mut()` — 18 sites (child_agents 6, tool_orchestrator 5,
  response_guardrails 3, virtual_tools 2, turn_executor 1,
  submission_handlers 1) → 0.

### Three PR slices

Per the stacked-PR review-cost lesson, the change lands as three independently
reviewable slices, each of which deletes something:

- **Slice A — generation + state native** (D1+D2+D4 completed for the full
  loop, not just the M2 spine): all `.llm_client*()` sites migrate; auxiliary
  generations (compaction, memory flush) go through `/mnt/llm`; the engine
  writes `/agent/<pid>` state directly; `Legacy` loses `llm_client`. Invisible
  to users; pure foundation.
- **Slice B — tools as executables** (D3): all `.tools*()` sites migrate;
  `ToolRegistry` and the `Legacy` variant are deleted. The child-agent
  capability-cloning sites become child-namespace assembly (bridging into C).
- **Slice C — capability assembly + overlay** (D5, plus the ctl-routing and
  `/agent` overlay work absorbed from `introduce-alan-kernel-runtime` §7–§8).
  This is the first slice where the north star is *visible*: an agent's world
  narrows and widens with its mounts.

### `namespace_native.rs` is scaffolding with a demolition date

The M2 spine was proven in a standalone module (`runtime/namespace_native.rs`)
sitting beside `agent_loop.rs`. That was the right way to prove the path, but
this change's own proposal condemns exactly this shape as a permanent
structure: a parallel second engine is the compatibility bridge the
Architecture Progression Principle forbids. The module therefore **dissolves
into `turn_executor`/`agent_loop` by the end of Slice A** — migration means
moving the existing call sites onto namespace operations, not growing the side
module until it rivals the loop.

### Hidden dependency: provider projection must move behind the llmfs DTO

The engine does not only call `generate_stream`. It calls
`detect_provider(...)`, `llm_client().capabilities()`, and
`llm_client().project_messages(...)` — provider-specific request shaping in
engine code. ADR-0024 D2 places that mapping in the provider file server: the
engine writes one neutral request document; `alan-llmfs` maps it to the
provider-local wire format. This puts `add-llm-file-server` §5 (versioned wire
DTO) and §3.1 (capability introspection files) on Slice A's critical path —
they are no longer optional polish on the llmfs surface.

### Ownership absorbed / deferred

- Absorbed from `introduce-alan-kernel-runtime` (now archived as historical
  ADR-0024 migration context): kernel-vs-machine ctl routing (§7.1 → tasks
  §7.4), the `/agent` overlay and `/agent/root` resolution (§8 → tasks §7.3),
  and `requests/<id>/response` resume delivery (§5.3 → tasks §5.4).
- Deliberately deferred to a follow-on change: TUI file-client migration (its
  §9) — out of blast radius here.
- Parked until Slice B settles the tool-execution seam:
  `refactor-sandbox-spec-input` (P1 of the namespace-driven sandbox track)
  touches `tool_orchestrator`, which A/B actively rewrite.

## Open Questions

- Does the engine hold one aP root handle (a federating namespace server) or
  individual mounted handles? Leaning federating root so paths like `/mnt/llm`
  and `/agent/<pid>` resolve uniformly — needs the kernel namespace presented as
  one `FileServer` (a thin `MountFs` over `alan-kernel::Namespace`).
- How much of the current `Tape`/compaction logic stays in-engine vs becomes
  views over `machine/tape`? Compaction-as-view is the target; scope the first
  pass to writing the tape, keep compaction in-engine initially.
