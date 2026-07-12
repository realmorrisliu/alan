## 1. Prerequisite And Package Contract

- [x] 1.1 Start from main only after `clean-canonical-spec-debt`, `remove-residual-compatibility-shims`, and `remove-legacy-macos-persistence` are merged and their canonical deltas are synced.
- [x] 1.2 Inventory every `ToolRegistry`, `RuntimeToolProcessRunner`, `RuntimeEventEnvelope`, `event_sender`, event-forwarder/projector, runtime receiver, and single-variant `RuntimeEnvironment` live-path use before replacement.
- [x] 1.3 Complete built-in Tool package manifests under `/lib/exec/<tool>/manifest` with model definition, schema, capability, locality, timeout/execution hints, and tests that join each manifest to its `/bin` executable.
- [x] 1.4 Add composition tests proving permitted Tool packages mount executable plus manifest together and incomplete packages fail closed before Agent Process launch.

## 2. Namespace-Native Tool Discovery

- [x] 2.1 Implement a namespace Tool-package walker that enumerates visible `/bin` entries, validates `/lib/exec/<tool>/manifest`, distinguishes Tools from Agent Executables/ordinary commands, and returns immutable request metadata without constructing a registry.
- [x] 2.2 Switch request assembly, Tool definitions, capability classification, locality, timeout hints, and policy inputs from `RuntimeLoopState.tool_registry` to the namespace walker.
- [x] 2.3 Add tests proving visible complete packages are model-callable, missing executables/manifests are not, and no hidden catalog can grant an unmounted Tool.

## 3. Process-Native Tool Execution

- [x] 3.1 Implement the namespace Tool launcher that resolves `/bin/<tool>`, commits the exec spec through `/proc/clone`, passes arguments through the defined Process/file contract, and reads output/result files.
- [x] 3.2 Switch Tool orchestration and `actions/<id>` ownership from registry-backed materialization to the concrete Tool Process reference and result.
- [x] 3.3 Delete direct in-process Tool implementation calls from the Agent Execution Engine effect path and add tests proving unmounted Tools cannot execute.
- [x] 3.4 Replace child Tool registry construction with pre-spawn complete-package mounts and make child request assembly walk its own namespace.

## 4. Direct AgentFS State Ownership

- [x] 4.1 Add narrow owner-specific AgentFS writers for output/tape, request trees, action trees, and `machine/ui` activity/plan/thinking/notice snapshots plus streams.
- [x] 4.2 Move assistant streaming/output and tape/checkpoint updates to direct writes by the turn/tape owners.
- [x] 4.3 Move yield/approval/interaction state to direct request-tree writes and Tool lifecycle/results to direct action-tree writes.
- [x] 4.4 Move activity, plan, renderer-visible thinking, warnings, compaction, and memory notices to direct `machine/ui` writes without accepting a generic runtime event as input.
- [ ] 4.5 Convert engine, host, and TUI integration tests to hydrate snapshots and resume AgentFS streams by offset rather than waiting on live runtime receivers.

## 5. File-Based Child Supervision

- [x] 5.1 Implement child observation over `/proc/<pid>/status`, `io/output`, request/action streams, `machine/ui/events`, and `machine/ui/activity` freshness with resumable offsets/timestamps.
- [x] 5.2 Switch child progress, heartbeat, timeout, output collection, and terminal-state reconciliation from `RuntimeEventEnvelope` subscription to Process/file observation.
- [x] 5.3 Add tests for quiet-but-fresh child activity, stale-file timeout, Process exit winning over stale projection, supervisor reattachment, and parent/child namespace differences.

## 6. Delete Parallel Engine Authorities

- [x] 6.1 Remove `ToolRegistry` from `RuntimeLoopState`, runtime construction, child launch, Tool policy/orchestration, and tests; delete registry-only helpers and materializers.
- [ ] 6.2 Remove `RuntimeEventEnvelope`, `RuntimeHandle.event_sender`, broadcast channel setup, internal forwarding tasks, subscription helpers, host forwarding, and event-to-AgentFS/UI projectors.
- [ ] 6.3 Audit remaining semantic `Event` and `Op` uses; retain only file-record schemas or transition-local values and delete broadcast-only variants/metadata.
- [x] 6.4 Replace the single-variant `RuntimeEnvironment` wrapper with the concrete namespace handle throughout public/internal APIs and tests.
- [ ] 6.5 Add an absence guard proving the engine live path has no injected provider, Tool registry, event sink, broadcast sender/receiver, or generic event projector.

## 7. Verification And Delivery

- [ ] 7.1 Run focused Kernel, AgentFS, LLMFS, Agent Execution Engine, child lifecycle, Tool package, governance, and Rust TUI tests plus an end-to-end file-only conversation with Tool and child execution.
- [ ] 7.2 Run `cargo fmt --all --check`, workspace Clippy with all targets/features and warnings denied, `cargo test --workspace`, `just smoke`, the new absence guard, and `git diff --check`.
- [ ] 7.3 Measure streaming, Tool-call, and child-supervision hot paths and confirm file-native ownership does not introduce unbounded polling, duplicate writes, or a second cache authority.
- [ ] 7.4 Review implementation evidence against ADR-0024's convention-enforced boundary and avoid claiming hard multi-process isolation before the later Kernel transport/enforcement slice.
- [ ] 7.5 Open the engine-boundary PR and keep the current HEAD under Codex review until every thread is resolved, required CI is green, and a delayed refresh shows no new findings before merge.
- [ ] 7.6 After merge, sync all five capability deltas into canonical specs, verify the TUI and integration tests use only file observation, and mark the change archive-ready.
