## 1. Supersession and PR disposition

- [ ] 1.1 Mark `introduce-alan-kernel-runtime` as superseded by this change for
  the engine-native rewrite; move its remaining deferred tasks (live wiring,
  io/input resume, overlay) under this change's ownership.
- [x] 1.2 Note in PR #579 (`alan-llmfs-client`) that `FileLlmProvider` is retired
  by this change; its clone/write/read-events logic moves into the engine.
- [x] 1.3 Note in PR #576 (`alan-agentfs`) that it is re-aimed from event
  projector to file backing of engine-written state.

## 2. Namespace handle for the engine (D1)

- [ ] 2.1 Present the kernel namespace as one aP `FileServer` (a thin `MountFs`
  over `alan-kernel::Namespace`) so `/mnt/llm`, `/proc`, `/agent/<pid>` resolve
  uniformly through one root handle.
- [ ] 2.2 Add the namespace-handle environment to `RuntimeLoopState`, replacing
  the `provider` and `tools` fields. TDD against an in-memory root (MemFs +
  mounted servers).
- [ ] 2.3 Add a `dependency_boundary`-style test that the engine reaches LLM,
  tools, and its own state only through the namespace handle (no `LlmProvider` /
  `ToolRegistry` field remains on the live path).

## 3. Generation as a file operation (D2)

- [ ] 3.1 Replace the engine's generation call site with file ops on
  `/mnt/llm/connections/<conn>` (clone-via-open → write `data` → read `events`),
  reusing the client logic currently in `alan-llmfs-client`.
- [ ] 3.2 Delete the `LlmProvider`/`generate_stream` call path from the engine.
- [ ] 3.3 Retire `crates/llmfs-client` (`FileLlmProvider`); close/repurpose PR
  #579.
- [ ] 3.4 TDD generation end-to-end against `alan-llmfs` (mock-backed): a turn
  produces output by reading `events`, with no provider object involved.

## 4. State written to agent files (D4)

- [ ] 4.1 Write assistant output to `io/output`, the tape to `machine/tape`,
  yields to `requests/<id>/`, and tool calls to `actions/<id>/` from the engine.
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
- [ ] 4a.2 When the engine drives tape writes (D4), honor the contract's
  GENERATING exclusive-write lease on `machine/tape` (one writer, open readers;
  append-only). The aP-layer promotion of the lease before the first external
  writer is owned by the future external-writers work, not here.

## 5. M2 — a real conversation through files (D1+D2+D4)

- [ ] 5.1 Spawn an agent process via `/proc/clone` with a namespace mounting an
  llm connection and its `/agent/<pid>` tree.
- [ ] 5.2 Wire `alan-shell` `io/input`/`io/output` to the spawned agent (generic
  builtins only).
- [ ] 5.3 TDD M2 end-to-end: shell writes `io/input` → agent generates via
  `/mnt/llm` → response appears on `io/output` → shell tails it (mock-backed, no
  real key); add a live `#[ignore]` variant for a real connection.

## 6. Tools as executables (D3)

- [ ] 6.1 Convert one tool to a `/bin` executable invoked via `/proc/clone`,
  reading its output files; project into `actions/<id>/`. Prove the path.
- [ ] 6.2 Convert the remaining built-in tools; remove the in-process
  `ToolRegistry` from the engine's effect path.
- [ ] 6.3 TDD: a tool effect is produced by spawning `/bin/<tool>` and reading its
  files; a withheld (unmounted) tool is unreachable.

## 7. Spawn-time capability assembly (D5)

- [ ] 7.1 Assemble each agent's namespace at `/proc/clone` (llm connection +
  tools + agent tree = capability set); a sub-agent with fewer mounts has a
  narrower world.
- [ ] 7.2 Restate the convention-vs-isolation boundary (ADR-0024 R1) where the
  capability set is documented; do not claim hard isolation until the kernel
  §7.1a amplification check lands.

## 8. Verification

- [ ] 8.1 `just verify` (fmt + lint + test + mock smoke).
- [ ] 8.2 `openspec validate refactor-engine-namespace-native --strict`.
- [ ] 8.3 Confirm no `LlmProvider`, `ToolRegistry`, or `EventEnvelope` remains on
  the engine's live path (only as legacy transport behind file servers).
- [ ] 8.4 Confirm the iron law on the agent files: no operation is performed as a
  side effect of a data/field write (control only via `ctl`), and the GENERATING
  tape lease holds under a concurrent-writer test.
