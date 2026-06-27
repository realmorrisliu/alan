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

### Access discipline (the namespace's own law)

D1–D6 decide *what becomes a file*. D7–D10 decide *the discipline those files
obey*, so that "the agent's state is files" does not quietly re-introduce the
side-channels the Plan 9 model exists to remove. The governing constraint (the
*iron law*) is: **every behavior change or extension SHALL be expressible as a file
operation or a mount/interpose on the namespace — never an out-of-band API.** An
audit of the current `agentfs` surface found three deviations this change must not
cement: control smuggled into a data write (writing `requests/<id>/response` flips
status to `answered`), `machine/status` as free-text doubling as lifecycle control,
and no control plane for interrupt/pause/resume at all.

### D7. Control, state, and notification are three separate file roles

State is read from `data`/field files; an operation is a write to a `ctl` file; a
notification is an append to an `events` stream. A write to a state file SHALL NOT
perform an operation as a side effect. Concretely: answering a yield becomes
`requests/<id>/ctl <- answer` (not a side effect of writing `response`), and
lifecycle transitions become verbs on `machine/ctl` over a fixed vocabulary
(`interrupt`/`pause`/`resume`), with `machine/status` demoted to read-only state.
*Why now*: this change writes the engine→file paths for the first time; baking in
the data/ctl split here avoids cementing the side-channel shape the iron law
forbids. *Alternative*: keep status/response overloaded — rejected, a control
side-channel disguised as a field.

### D8. Access is gated by the agent's run-state, not statically by node

`is_writable(node)` becomes `access(node, run_state, actor)`. While the agent is
GENERATING, `machine/tape` is held under an *exclusive-write* lease by the generator
(one writer, open to readers — not Plan 9 `DMEXCL`, which would also exclude the
consumers tailing the tape) so no second writer can splice the tape mid-stream;
`machine/tape` and `events` are append-only by policy, not merely by absent
capability. The lease's enforcement layer is decided in D12. The safe
window for a human/extension to amend tape or context is the YIELDED/paused state —
which is exactly the existing recoverable-Yield point where control returns to a
consumer. *Scope note*: the run-state→access matrix and the GENERATING tape lease
are load-bearing for M2 (the moment a live writer exists) and are in-scope; the
human-edit-on-yield *surfacing* is stated as the contract but staged to a follow-on.

### D9. The namespace is self-describing, because the agent is a consumer

Because the agent (an LLM) itself reads and writes these files to think, each node
SHALL expose its byte contract in-band — a `man`/`ctl`-help the engine can read as
prose — rather than the contract living only in Rust doc-comments and specs. This
is the one place the Plan 9 "files + out-of-band man pages" model improves on its
ancestor: the consumer reads prose, so the manual ships inside the namespace.
*Scope note*: principle adopted now; minimal form is a documented record vocabulary
per stream, expanded incrementally.

### D10. Permission carries an actor dimension; the iron law needs the seam

Write authority is keyed to the actor (agent / parent / human / interposing
extension) and the agent's mounted capabilities, not a global property of the node.
This is the seam the iron law requires: an extension changes behavior by
interposing a file server on the namespace, governed by who-may-{read,write,mount,
interpose}, never by a private API. Cross-actor enforcement leans on the kernel's
deferred §7.1a amplification check (ADR-0024 R1); until it lands the boundary is
convention-enforced, which this change restates, not claims to isolate.

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
- **Access discipline (D7–D10) could balloon the rewrite** → only the data/ctl
  split (D7) and the GENERATING tape lease (D8) are load-bearing for M2 and stay
  in-scope; self-description (D9) is incremental and cross-actor permission (D10)
  rides the deferred kernel check. If D7–D10 threaten M2, split the
  human-edit/interpose *surfacing* into a follow-on change rather than blocking the
  talking-agent spine.

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

## Open Questions

- Does the engine hold one aP root handle (a federating namespace server) or
  individual mounted handles? Leaning federating root so paths like `/mnt/llm`
  and `/agent/<pid>` resolve uniformly — needs the kernel namespace presented as
  one `FileServer` (a thin `MountFs` over `alan-kernel::Namespace`).
- How much of the current `Tape`/compaction logic stays in-engine vs becomes
  views over `machine/tape`? Compaction-as-view is the target; scope the first
  pass to writing the tape, keep compaction in-engine initially.
- Does the `ctl` vocabulary live per-node (each writable file has its own verbs) or
  split as `machine/ctl` for lifecycle plus per-container ctl for requests/actions?
  Leaning: lifecycle on `machine/ctl`, answering on `requests/<id>/ctl`.
- Is the GENERATING tape lease modeled as a Plan 9 `DMEXCL` exclusive open at the
  aP layer, or as a run-state check inside agentfs? Leaning aP-layer exclusive open
  so the guarantee is structural, not a per-server convention.
