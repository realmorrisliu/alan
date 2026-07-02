> **Delivery in three PR slices (re-planned 2026-07-02).** The finish line for
> this change is structural, not a checkbox count: **the
> `RuntimeEnvironment::Legacy` variant in `agent_loop.rs` can be deleted.** The
> progress bar is the production call-site count of the legacy accessors — at
> re-plan time: `.llm_client()`/`.llm_client_mut()` ×21 (turn_executor 15,
> compaction 2, memory_flush 2, agent_loop 1, tool_orchestrator 1) and
> `.tools()`/`.tools_mut()` ×18 (child_agents 6, tool_orchestrator 5,
> response_guardrails 3, virtual_tools 2, turn_executor 1,
> submission_handlers 1). Both counts must reach zero (§8.3 greps for this).
>
> - **Slice A — generation + state native** (§2, §3, §4, §4a): all
>   `.llm_client*()` call sites migrate to namespace generation;
>   `namespace_native.rs` dissolves into the loop modules (it is scaffolding,
>   not a second engine); `RuntimeEnvironment::Legacy` loses its `llm_client`.
> - **Slice B — tools as executables** (§6): all `.tools*()` call sites migrate
>   to `/bin` + `/proc/clone`; `ToolRegistry` and the `Legacy` variant are
>   deleted.
> - **Slice C — capability assembly + overlay** (§7): spawn-time namespace
>   assembly, the `/agent` overlay and ctl routing (absorbed from
>   `introduce-alan-kernel-runtime` §7–§8).
>
> M2 (§5) already has a walking skeleton on the scaffolding path; it becomes
> load-bearing when Slice A lands. TUI file-client migration
> (`introduce-alan-kernel-runtime` §9) is deliberately **not** owned here — it
> is a follow-on change once A–C land.

## 1. Supersession and PR disposition

- [x] 1.1 Mark `introduce-alan-kernel-runtime` as superseded by this change for
  the engine-native rewrite; move its remaining deferred tasks (live wiring,
  io/input resume, overlay) under this change's ownership. Disposition
  2026-07-02: ctl routing + `/agent` overlay absorbed into §7.3/§7.4 (Slice C),
  `requests/<id>/response` resume delivery into §5.4; TUI migration (its §9)
  deferred to a follow-on change once A–C land.
- [x] 1.2 Note in PR #579 (`alan-llmfs-client`) that `FileLlmProvider` is retired
  by this change; its clone/write/read-events logic moves into the engine.
- [x] 1.3 Note in PR #576 (`alan-agentfs`) that it is re-aimed from event
  projector to file backing of engine-written state.

## 2. Namespace handle for the engine (D1)

- [x] 2.1 Present the kernel namespace as one aP `FileServer` (a thin `MountFs`
  over `alan-kernel::Namespace`) so `/mnt/llm`, `/proc`, `/agent/<pid>` resolve
  uniformly through one root handle. Done in `alan-kernel::MountFs`: per-fid it is
  either a synthetic namespace dir (lists child mount points) or a backing node
  forwarded through `Resolved::call` (mount access enforced). Unblocks the shell's
  path-resolution P1s (#577).
- [x] 2.2 Add the namespace-handle environment to `RuntimeLoopState`, replacing
  the `provider` and `tools` fields. TDD against an in-memory root (MemFs +
  mounted servers).
- [x] 2.3 Add a `dependency_boundary`-style test that the engine reaches LLM,
  tools, and its own state only through the namespace handle (no `LlmProvider` /
  `ToolRegistry` field remains on the live path).
- [ ] 2.4 Dissolve `runtime/namespace_native.rs` into the loop modules
  (`turn_executor` / `agent_loop`) by the end of Slice A. It is the M2 proving
  scaffold, not a second engine; per the Architecture Progression Principle it
  must not survive as a parallel path once the real call sites are migrated.

## 3. Generation as a file operation (D2)

- [ ] 3.1 Replace the engine's generation call site with file ops on
  `/mnt/llm/connections/<conn>` (clone-via-open → write `data` → read `events`),
  reusing the client logic currently in `alan-llmfs-client`.
- [ ] 3.2 Delete the `LlmProvider`/`generate_stream` call path from the engine.
- [x] 3.3 Retire `crates/llmfs-client` (`FileLlmProvider`); close/repurpose PR
  #579.
- [x] 3.4 TDD generation end-to-end against `alan-llmfs` (mock-backed): a turn
  produces output by reading `events`, with no provider object involved.
- [ ] 3.5 Move provider projection behind the llmfs wire DTO: the engine's
  `detect_provider(...)`, `llm_client().capabilities()`, and
  `llm_client().project_messages(...)` call sites are replaced by writing one
  neutral request document; provider-local wire mapping lives in `alan-llmfs`
  (ADR-0024 D2). **Critical-path dependency on `add-llm-file-server` §5
  (versioned DTO) and §3.1 (capability introspection files)** — without it,
  `LlmClient` cannot leave `compaction`/`memory_flush`/`turn_executor`.
- [ ] 3.6 Migrate the non-turn generation call sites — compaction summarize
  (`compaction.rs`, 2 sites) and memory flush (`memory_flush.rs`, 2 sites) — to
  namespace generation, so auxiliary generations also go through `/mnt/llm`.

## 4. State written to agent files (D4)

- [ ] 4.1 Write assistant output to `io/output`, the tape to `machine/tape`,
  yields to `requests/<id>/`, and tool calls to `actions/<id>/` from the engine.
  **Constraint (ADR-0027 D1)**: tape records must be content-addressable-ready —
  self-contained, append-only, canonically serializable units whose identity
  does not depend on file offsets or mutable in-place state, so
  `add-content-addressed-knowledge` can later hash-address them without a
  migration rewrite. Hash storage itself is NOT built here; only the record
  shape is constrained.
- [x] 4.2 Re-aim `alan-agentfs` to serve these engine-written files (file backing)
  rather than translating `EventEnvelope`s.
- [x] 4.3 Remove the `Event`/`EventEnvelope` publication path from the engine's
  live model (keep the alphabet only as legacy compatibility transport).
- [x] 4.4 TDD: reading the agent's files reflects engine state directly, with no
  event-projection step.

- [x] 4.5 agentfs surfaces completed (per-container requests/events +
  actions/events streams, io/events scoped to IO, requests/<id>/options).
  Originally deferred from the
  agentfs rework): per-container `requests/events` + `actions/events` streams,
  `io/events` scoped to IO-only lifecycle records (vs the aggregate), and
  structured `requests/<id>/options`. These land once the engine drives the
  writes and the record kinds exist.

## 4a. Access discipline — owned by the file-layout contract

The agent-file access discipline (`ctl` roles/scoping, the `machine/tape`
generation lease, append-only tape/events, actor-keyed authority + interpose, the
external-writer protocol-layer prerequisite, self-description) is **specified in
`define-agent-file-layout-contract`, not here.** This change only has to honor it:

- [x] 4a.1 The agentfs file surface (#576) conforms to the contract: answering is
  a `requests/<id>/response` write committed on clunk (rejected if terminal),
  `machine/ctl` carries agent-runtime tape commands (`compact`/`rollback`),
  generic lifecycle control is the kernel `/proc/<pid>/ctl`, and
  `machine/status`/`requests/<id>/status` are read-only state.
- [x] 4a.2 When the engine drives tape writes (D4), honor the contract's
  GENERATING exclusive-write lease on `machine/tape` (one writer, open readers;
  append-only). The aP-layer promotion of the lease before the first external
  writer is owned by the future external-writers work, not here.

## 5. M2 — a real conversation through files (D1+D2+D4)

- [x] 5.1 Spawn an agent process via `/proc/clone` with a namespace mounting an
  llm connection and its `/agent/<pid>` tree.
- [x] 5.2 Wire `alan-shell` `io/input`/`io/output` to the spawned agent (generic
  builtins only).
- [x] 5.3 TDD M2 end-to-end: shell writes `io/input` → agent generates via
  `/mnt/llm` → response appears on `io/output` → shell tails it (mock-backed, no
  real key); add a live `#[ignore]` variant for a real connection.
- [ ] 5.4 Deliver a written `requests/<id>/response` back into the engine's
  resume path, and resume a waiting agent on new `io/input` frames (absorbed
  from `introduce-alan-kernel-runtime` §5.3; becomes real once Slice A puts the
  full loop on the namespace path).

## 6. Tools as executables (D3)

- [ ] 6.1 Convert one tool to a `/bin` executable invoked via `/proc/clone`,
  reading its output files; project into `actions/<id>/`. Prove the path.
  Progress: `ProcFs` now supports spawner-context clone views and
  `NamespaceRuntimeEnvironment::spawn_process` can submit an exec spec through
  `/proc/clone`; remaining work is executable resolution/runner output and
  projecting the real tool result into `actions/<id>/`.
- [ ] 6.2 Convert the remaining built-in tools; remove the in-process
  `ToolRegistry` from the engine's effect path.
- [ ] 6.3 TDD: a tool effect is produced by spawning `/bin/<tool>` and reading its
  files; a withheld (unmounted) tool is unreachable.
- [ ] 6.4 Delete the `RuntimeEnvironment::Legacy` variant and its panicking
  accessors (`llm_client()`, `tools()`, …) from `agent_loop.rs` — the structural
  finish line for Slices A+B. The child-agent capability-cloning call sites in
  `child_agents.rs` (6 `.tools*()` sites) become child-namespace assembly (§7.1).

## 7. Spawn-time capability assembly (D5)

- [ ] 7.1 Assemble each agent's namespace at `/proc/clone` (llm connection +
  tools + agent tree = capability set); a sub-agent with fewer mounts has a
  narrower world.
  Progress: `/proc/clone` can now record the child process with the spawner's
  parent/credentials and a child copy of a preassembled namespace; remaining
  work is wiring production agent/sub-agent spawn assembly into that path.
- [x] 7.2 Restate the convention-vs-isolation boundary (ADR-0024 R1) where the
  capability set is documented; do not claim hard isolation until the kernel
  §7.1a amplification check lands.
- [ ] 7.3 Present `/agent/<pid>` as an overlay: union the kernel `/proc/<pid>`
  generic layout with the agent surfaces (requests/actions/machine/…); resolve
  `/agent/root` to whichever pid embodies the root agent's home. No agent files
  in `/proc`; no `/agent` entry without a backing `/proc` Process. (Absorbed
  from `introduce-alan-kernel-runtime` §8.)
- [ ] 7.4 Route generic control (interrupt, cancel) through the kernel
  `/proc/<pid>/ctl` and agent-runtime control (compact, rollback) through
  `machine/ctl` — the kernel interprets no runtime semantics. (Absorbed from
  `introduce-alan-kernel-runtime` §7.1; the agentfs surface half is done per
  §4a.1 — this task is the engine honoring both routes.)

## 8. Verification

- [ ] 8.1 `just verify` (fmt + lint + test + mock smoke).
- [x] 8.2 `openspec validate refactor-engine-namespace-native --strict`.
- [ ] 8.3 Confirm no `LlmProvider`, `ToolRegistry`, or `EventEnvelope` remains on
  the engine's live path (only as legacy transport behind file servers): grep
  for `.llm_client()`/`.llm_client_mut()`/`.tools()`/`.tools_mut()` production
  call sites returns zero and `RuntimeEnvironment::Legacy` no longer exists —
  checkbox state elsewhere in this file does not override this grep.
- [ ] 8.4 Confirm the iron law on the agent files: no operation is performed as a
  side effect of a data/field write (control only via `ctl`), and the GENERATING
  tape lease holds under a concurrent-writer test.
