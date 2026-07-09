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
>
> **Metric update (2026-07-02, structural finish line reached):** the legacy
> runtime branch is gone. The structural grep for `RuntimeEnvironment::Legacy`,
> legacy generation/tool helpers, `.llm_client*()` accessors, `.tools_mut()`,
> `InProcessCatalog`, the provider-stream turn helper, and the deleted retry
> module returns zero under `crates/agent-engine/src`. `just verify` and
> `openspec validate refactor-engine-namespace-native --strict` pass. The TUI
> file-client migration remains the explicitly deferred follow-on change.

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
- [x] 2.4 Dissolve `runtime/namespace_native.rs` into the loop modules
  (`turn_executor` / `agent_loop`) by the end of Slice A. It is the M2 proving
  scaffold, not a second engine; per the Architecture Progression Principle it
  must not survive as a parallel path once the real call sites are migrated.
  Progress: the standalone top-level `runtime/namespace_native.rs` module is
  gone; its namespace environment, llmfs file client, AgentFS state writers,
  `/proc/clone` tool execution helpers, and M2 runtime tests now live under the
  `agent_loop` module as `agent_loop/namespace_environment.rs`. The dependency
  boundary test now reads that loop-owned module instead of a parallel runtime
  module.

## 3. Generation as a file operation (D2)

- [x] 3.1 Replace the engine's generation call site with file ops on
  `/mnt/llm/connections/<conn>` (clone-via-open → write `data` → read `events`),
  reusing the client logic currently in `alan-llmfs-client`.
  Done 2026-07-02: the production runtime bootstrap wraps the resolved
  `LlmClient` behind `alan-llmfs`, assembles a root Agent Process namespace, and
  runs the loop through the namespace-only `RuntimeEnvironment`. Turn generation
  opens the llmfs connection clone, writes the neutral request DTO to `data`,
  tails `events` for live deltas, and reconstructs the final
  `GenerationResponse` from those records.
- [x] 3.2 Delete the `LlmProvider`/`generate_stream` call path from the engine.
  Done 2026-07-02: `RuntimeLoopState::generate_stream_for_turn` and the
  provider-stream turn branch are deleted, `streaming_mode` no longer calls
  `LlmProvider::generate_stream`, and the `RuntimeEnvironment::Legacy`
  generation path is gone. Production root and child runtimes reach providers
  only as llmfs backends, never as the engine's live transition-function
  surface.
- [x] 3.3 Retire `crates/llmfs-client` (`FileLlmProvider`); close/repurpose PR
  #579.
- [x] 3.4 TDD generation end-to-end against `alan-llmfs` (mock-backed): a turn
  produces output by reading `events`, with no provider object involved.
- [x] 3.5 Move provider projection behind the llmfs wire DTO: the engine's
  `detect_provider(...)`, `llm_client().capabilities()`, and
  `llm_client().project_messages(...)` call sites are replaced by writing one
  neutral request document; provider-local wire mapping lives in `alan-llmfs`
  (ADR-0024 D2). **Critical-path dependency on `add-llm-file-server` §5
  (versioned DTO) and §3.1 (capability introspection files)** — without it,
  `LlmClient` cannot leave `compaction`/`memory_flush`/`turn_executor`.
  Done 2026-07-02: `alan-llmfs` now accepts a versioned v1 neutral request DTO and
  emits versioned stream-event records; namespace generation writes the full
  `GenerationRequest` as that DTO. Compaction, memory flush, turn execution,
  turn-end memory promotion, and guardian reviewer escalation no longer call
  `.llm_client*()` directly and have namespace-backed llmfs coverage where the
  path is load-bearing. Namespace turn generation now uses a neutral capability
  matrix rather than pretending to be an OpenAI-compatible provider, and
  regression coverage asserts the llmfs request has no provider-local
  `extra_params`. `alan-llmfs` now exposes provider/connection capability files
  and the namespace turn loop reads `/mnt/llm/connections/<conn>/capabilities`
  at turn start; the engine neutralizes provider wire-projection fields so
  `responses_input_items` / `chat_completions_messages` /
  `anthropic_messages` stay behind llmfs. Turn response selection routes
  namespace generation through llmfs `events`; the engine tails those records
  directly for live text deltas instead of calling a provider stream branch.
  `detect_provider` and the legacy generation/capability helpers are deleted.
  Mock llmfs connections now report Responses-compatible capabilities at the
  file-server boundary, preserving test semantics without reintroducing an
  engine-side provider projection.
- [x] 3.6 Migrate the non-turn generation call sites — compaction summarize
  (`compaction.rs`, 2 sites) and memory flush (`memory_flush.rs`, 2 sites) — to
  namespace generation, so auxiliary generations also go through `/mnt/llm`.
- [x] 3.7 Restore incremental (token-level) output on the namespace path
  before the legacy branch dies. The namespace turn currently uses
  request/response generation with streaming explicitly confined to the legacy
  branch (§3.5 progress note). Tail the llmfs Generation `events` stream during
  the turn (and/or append `io/output` incrementally) so clients see deltas, not
  only turn-final output — the `events` file is already a retained,
  offset-resumable stream, so this is client-side work in the engine, not a new
  llmfs surface. **Blocks §6.4**: deleting `RuntimeEnvironment::Legacy` without
  this is a user-visible streaming regression on the very day the finish line
  is crossed.
  Done 2026-07-02: `NamespaceRuntimeEnvironment` now has a live generation
  reader that tails `/mnt/llm/connections/<conn>/<generation>/events`, forwards
  each text record as an engine `TextDelta`, and still returns the accumulated
  `GenerationResponse` for guardrails, tape writes, and tool-call handling. The
  namespace turn executor uses this live reader by default and suppresses the
  old final-response re-chunking when llmfs text events were already emitted.
  Focused coverage proves a mock llmfs generation with three token records
  produces the same three `TextDelta` chunks before `TurnCompleted`, while
  `io/output`, `machine/tape`, and the neutral request DTO behavior remain
  unchanged.

## 4. State written to agent files (D4)

- [x] 4.1 Write assistant output to `io/output`, the tape to `machine/tape`,
  yields to `requests/<id>/`, and tool calls to `actions/<id>/` from the engine.
  **Constraint (ADR-0027 D1)**: tape records must be content-addressable-ready —
  self-contained, append-only, canonically serializable units whose identity
  does not depend on file offsets or mutable in-place state, so
  `add-content-addressed-knowledge` can later hash-address them without a
  migration rewrite. Hash storage itself is NOT built here; only the record
  shape is constrained.
  Done 2026-07-02: assistant output/tape and tool actions already write through
  `NamespaceRuntimeEnvironment`; yield-producing runtime paths now allocate
  AgentFS `requests/<id>/` records before pausing, use that file id as the
  pending/resume key, and preserve the original tool/checkpoint id in request
  options for tape/tool-response correlation. Covered by namespace request-file
  tests for `request_confirmation`, `request_user_input`, and dynamic tools,
  plus existing escalation/effect-replay resume coverage. ADR-0027 D1 is now
  locked by a byte-level tape-record test: `machine/tape` records are versioned,
  typed, self-contained newline-delimited JSON units (`version`, `kind`, `role`,
  `content`) so later content-addressed storage can hash records without
  depending on offsets or mutable tape state.
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
- [x] 5.4 Deliver a written `requests/<id>/response` back into the engine's
  resume path, and resume a waiting agent on new `io/input` frames (absorbed
  from `introduce-alan-kernel-runtime` §5.3; becomes real once Slice A puts the
  full loop on the namespace path). Progress: `NamespaceRuntimeEnvironment` can
  now read an answered `/agent/<pid>/requests/<id>/response` and convert it into
  an `Op::Resume` submission, preserving structured JSON answers as structured
  resume content and falling back to text for non-JSON responses. Focused
  coverage proves an AgentFS `response` write committed on clunk becomes a
  resume submission that the existing engine resume handler consumes to clear a
  pending structured-input yield. The live turn driver now polls pending
  namespace request responses while waiting for interaction and feeds answered
  requests into the same resume path as brokered submissions. `io/input` frames
  are read as `Op::Input(FollowUp)` submissions by the runtime main loop: idle
  runtimes start a new turn from file input, and active turns route file input
  through the existing in-band broker. Focused coverage proves both a delayed
  AgentFS response unblocks the pending wait and a namespace runtime completes a
  turn after only writing `/agent/<pid>/io/input`, with no API submission.

## 6. Tools as executables (D3)

- [x] 6.1 Convert one tool to a `/bin` executable invoked via `/proc/clone`,
  reading its output files; project into `actions/<id>/`. Prove the path.
  Progress: `ProcFs` now supports spawner-context clone views and
  `NamespaceRuntimeEnvironment::spawn_process` can submit an exec spec through
  `/proc/clone`; `ProcFs` can run a registered user-space executable runner
  after clone commit, write `/proc/<pid>/io/output`, and set the process exit
  code; `NamespaceRuntimeEnvironment::run_tool_action` spawns a `/bin/...`
  executable, reads `/proc/<pid>/io/output` + `exit`, and writes the same result
  into `actions/<id>/`. `tool_orchestrator` now routes namespace-environment
  `read_file` calls to `/bin/read_file` with the JSON arguments document as
  argv[0], reads the tool process output, normalizes it into the tool payload,
  and records the same action under AgentFS. Focused regression coverage proves
  the production branch with a mounted `/bin/read_file` executable; no
  in-process `ToolRegistry::execute` call is on that effect path.
- [x] 6.2 Convert the remaining built-in tools; remove the in-process
  `ToolRegistry` from the engine's effect path. Progress: namespace
  environments now execute every built-in tool name (`read_file`, `write_file`,
  `edit_file`, `bash`, `grep`, `glob`, `list_dir`) by spawning `/bin/<tool>` via
  `/proc/clone`; focused coverage asserts each one creates a process, reads the
  process output, and records `actions/<id>/` through AgentFS. The in-process
  `ToolRegistry::execute` path remains only as the legacy-environment fallback,
  not on the namespace effect path.
- [x] 6.3 TDD: a tool effect is produced by spawning `/bin/<tool>` and reading
  its files; a withheld (unmounted) tool is unreachable. Covered by namespace
  `tool_orchestrator` regression tests: mounted `/bin/read_file` exits 0,
  produces the tool payload via `/proc/<pid>/io/output`, and records
  `actions/<id>/`; omitting the `/bin` mount makes the same call exit 127 with a
  failed action and no successful tool effect.
- [x] 6.4 Delete the `RuntimeEnvironment::Legacy` variant and its panicking
  accessors (`llm_client()`, `tools()`, …) from `agent_loop.rs` — the structural
  finish line for Slices A+B. The child-agent capability-cloning call sites in
  `child_agents.rs` (6 `.tools*()` sites) become child-namespace assembly (§7.1).
  **Precondition satisfied: §3.7** — namespace streaming is restored, so the
  remaining blocker is deleting the legacy provider/tool fallback itself.
  Done 2026-07-02: `RuntimeEnvironment` has only the namespace variant; the
  legacy constructor, provider/capability helpers, retry module, in-process tool
  catalog branch, and legacy accessors are deleted. `RuntimeLoopState` keeps a
  host `tool_catalog` only for prompt metadata, response guardrails, virtual
  tools, workspace defaults, and child namespace materialization. Production
  root and child tool effects require a namespace environment and spawn
  `/bin/<tool>` through `/proc/clone`; `ToolRegistry::execute` remains behind the
  process runner/materializer, not on the engine effect path.

## 7. Spawn-time capability assembly (D5)

- [x] 7.1 Assemble each agent's namespace at `/proc/clone` (llm connection +
  tools + agent tree = capability set); a sub-agent with fewer mounts has a
  narrower world.
  Progress: `/proc/clone` can now record the child process with the spawner's
  parent/credentials and a child copy of a preassembled namespace. Child-agent
  spawn now builds a `ChildNamespaceAssemblyPlan` from the `SpawnSpec` and the
  effective child config before materializing legacy tools: it records the child
  agent mount placeholder, `/mnt/llm/connections/<profile>`, the resolved
  workspace/cwd, and the exact `/bin/<tool>` mount set implied by
  `SpawnHandle::Workspace` plus any tool-profile allowlist. Regression coverage
  proves no Workspace handle mounts no `/bin` tools, an allowlist mounts only
  the requested tool executable, and an unbindable workspace-local parent tool is
  omitted for a different child workspace. `/proc/clone` exec specs can now
  declare a namespace manifest, and the kernel rejects commit-on-clunk when that
  manifest does not match the pending child namespace, preserving the
  no-pid-leak behavior for rejected spawns. The child namespace plan can now
  render a clone-ready exec document for a concrete child pid with `/agent/<pid>`
  rw, `/mnt/llm/connections/<profile>` rw, and allowed `/bin/<tool>` ro mounts,
  using the kernel's public `ExecSpec`/namespace-manifest DTO rather than a
  private mirror. `/proc/clone` now expands `<child-pid>` mount placeholders in
  the spawner namespace before manifest validation, so the spawner can assemble
  `/agent/<child-pid>` before the fid-private pid is known. Regression coverage
  proves a child plan with an allowed `/bin/alpha` mount writes that `ExecSpec`
  through real clone-open-write-clunk and the committed `/proc/<pid>/namespace`
  exposes `/agent/<pid>` rw, `/mnt/llm/connections/default` rw, and
  `/bin/alpha` ro. The runtime startup path now has an internal
  `spawn_with_namespace_environment` seam: the main loop can reach ready with a
  `RuntimeEnvironment::Namespace`, explicit startup provider capabilities, and a
  host tool catalog for prompt metadata, without constructing an injected
  `LlmClient`/`ToolRegistry` as its live environment. Done 2026-07-02:
  production child-agent launch now assembles mountable AgentFS, child-scoped
  LlmFS, and `/bin/<tool>` handles, commits the child through real
  `/proc/clone`, and starts the child runtime with
  `spawn_with_namespace_environment` bound to the allocated `/agent/<pid>` and
  child namespace. Regression coverage proves the child runtime still honors
  spawn filtering and that a tool spawned from the child namespace creates a
  `/proc/<pid>` process, writes the AgentFS `actions/<id>/` record, and returns
  the runner output. The remaining `ToolRegistry` use is the temporary
  executable runner/materializer to be deleted with the `RuntimeEnvironment`
  legacy fallback in §6.4/§8.3, not a blocker for spawn-time namespace assembly.
- [x] 7.2 Restate the convention-vs-isolation boundary (ADR-0024 R1) where the
  capability set is documented; do not claim hard isolation until the kernel
  §7.1a amplification check lands.
- [x] 7.3 Present `/agent/<pid>` as an overlay: union the kernel `/proc/<pid>`
  generic layout with the agent surfaces (requests/actions/machine/…); resolve
  `/agent/root` to whichever pid embodies the root agent's home. No agent files
  in `/proc`; no `/agent` entry without a backing `/proc` Process. (Absorbed
  from `introduce-alan-kernel-runtime` §8.)
  Done 2026-07-02: `alan-agentfs` now exposes `AgentRootFs`, a thin `/agent`
  view that reads `/proc` over aP, lists only registered agent surfaces whose pid
  exists in `/proc`, resolves `/agent/root` as an alias to the configured Root
  Agent Process pid, and forwards `/agent/<pid>/...` to that pid's `AgentFs`.
  The production child-agent namespace now mounts `/agent` instead of
  `/agent/<pid>`, binds the committed child pid into `AgentRootFs` after
  `/proc/clone` succeeds, and still runs the engine against `/agent/<pid>`.
  Coverage proves orphan agent surfaces are hidden, `/agent/root` and
  `/agent/<pid>` share the same state tree, agent files are absent from
  `/proc/<pid>`, and child-spawned tool processes inherit the `/agent` overlay.
- [x] 7.4 Route generic control (interrupt, cancel) through the kernel
  `/proc/<pid>/ctl` and agent-runtime control (compact, rollback) through
  `machine/ctl` — the kernel interprets no runtime semantics. (Absorbed from
  `introduce-alan-kernel-runtime` §7.1; the agentfs surface half is done per
  §4a.1 — this task is the engine honoring both routes.)
  Done 2026-07-02: namespace runtimes can write generic process control commands
  to `/proc/<pid>/ctl`, and `Op::Interrupt` now does that before clearing the
  engine's turn state. Regression coverage proves interrupting a namespace
  runtime exits `/proc/1` through the kernel ctl file and does not write a
  `ctl:` record into `/agent/1/events`; `machine/ctl` remains the
  agent-runtime compact/rollback surface covered by §4a.1.

## 8. Verification

- [x] 8.1 `just verify` (fmt + lint + test + mock smoke).
  Evidence 2026-07-02: `just verify` passed, including `cargo fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test --workspace`, doctests, and the mock smoke suite.
- [x] 8.2 `openspec validate refactor-engine-namespace-native --strict`.
- [x] 8.3 Confirm no `LlmProvider`, `ToolRegistry`, or `EventEnvelope` remains on
  the engine's live path (only as legacy transport behind file servers): grep
  for `.llm_client()`/`.llm_client_mut()`/`.tools()`/`.tools_mut()` production
  call sites returns zero and `RuntimeEnvironment::Legacy` no longer exists —
  checkbox state elsewhere in this file does not override this grep.
  Evidence 2026-07-02: structural grep for
  `RuntimeEnvironment::Legacy`, `RuntimeEnvironment::legacy`,
  `legacy_generation_`, `legacy_tool_registry_clone`, `InProcessCatalog`,
  `.llm_client()`, `.llm_client_mut()`, `.tools_mut()`,
  `generate_stream_for_turn`, and `mod retry` returns zero under
  `crates/agent-engine/src`. The remaining provider/tool traits are behind
  llmfs and `/proc` executable runners or are host metadata surfaces, not the
  engine's live generation/effect environment.
- [x] 8.4 Confirm the iron law on the agent files: no operation is performed as a
  side effect of a data/field write (control only via `ctl`), and the GENERATING
  tape lease holds under a concurrent-writer test.
  Evidence: `alan-agentfs` has direct coverage that ordinary data writes to
  `io/output`, `machine/tape`, `requests/<id>/prompt`, and
  `actions/<id>/status` leave `machine/status` unchanged and produce no `ctl:`
  control record, while `machine/ctl` is the explicit compact/rollback command
  path. The same `agent_files` suite covers terminal request-response integrity,
  read-only machine status, and the exclusive `machine/tape` writer lease; the
  engine namespace test `engine_tape_writer_holds_generating_lease_and_allows_readers`
  proves the runtime-side tape writer holds the GENERATING lease while readers
  can still tail.
