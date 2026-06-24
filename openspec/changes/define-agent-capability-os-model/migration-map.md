# Alan Agent Capability Migration Map

This map classifies current Alan Agent capabilities before they are migrated
into the Alan OS / Alan OS model. It is a planning artifact for preserving
useful existing implementation work while keeping Alan Kernel semantic and
moving execution, storage, provider, sandbox, and supervision concerns behind
Host Service APIs.

## Classification Legend

- **OS Primitive:** durable Alan Kernel semantics such as typed identity,
  descriptors, commands, tasks, permissions, evidence, audit, and projection
  shapes.
- **Host Service Capability:** service behavior provided through Host Service
  APIs or concrete Host Service Implementations, including provider execution,
  runtime supervision, memory storage, sandboxing, event transport, and
  scheduling.
- **Alan Agent App Feature:** product behavior owned by the built-in Alan Agent
  Agent Workspace, including conversation UX, session organization, steering,
  and user-visible agent work management.
- **Compatibility-only behavior:** current daemon, protocol, or TUI pathways
  that should remain stable while semantic parity is built, but should not
  become the durable OS contract.
- **Rewrite candidate:** behavior that conflicts with System Agent Supervisor,
  bounded Agent Runs, Context Grants, Result Contracts, Agent Capability
  Service, or Command Governance and should be reshaped instead of lifted
  directly.

## Capability Map

| Existing capability | Current source | Migration class | Target boundary | Reuse plan | Notes |
| --- | --- | --- | --- | --- | --- |
| Agent root and resolved definition overlays | `crates/runtime/src/agent_root.rs`, `crates/runtime/src/agent_definition.rs` | Host Service Capability; compatibility source | Host Service API for agent/app definition resolution | Adapt | Keep overlay resolution useful, but avoid making agent-root inheritance the general app OS model. |
| Agent-facing runtime config | `crates/runtime/src/config.rs`, `crates/runtime/src/runtime/mod.rs` | Host Service Capability | Agent Capability Service implementation config | Adapt | Split semantic Agent Run settings from provider/runtime/supervision knobs. |
| Connection profiles, auth, and model catalog | `crates/runtime/src/connections.rs`, `crates/runtime/src/models.rs`, `crates/alan/src/daemon/connection_*` | Host Service Capability | Credentials/model Host Service APIs | Reuse/adapt | These remain service concerns outside Alan Kernel. |
| Session identity and lifecycle | `crates/runtime/src/session.rs`, `crates/alan/src/daemon/session_store.rs`, `crates/alan/src/daemon/runtime_manager.rs` | Alan Agent App Feature; Host Service Capability; compatibility-only path | Bounded Agent Run plus Alan Agent Session projection | Adapt | Existing sessions stay compatibility authority while Agent Runs become the OS execution unit. |
| Tape, messages, content parts, and summaries | `crates/runtime/src/tape.rs` | Alan Agent App Feature; OS Primitive source | Agent Run transcript/result projection | Adapt | Tape remains valuable for Alan Agent, but Alan Kernel should not inherit tape as its universal state model. |
| Turn loop and execution engine | `crates/runtime/src/runtime/*`, especially `agent_loop.rs`, `turn_driver.rs`, `turn_executor.rs` | Host Service Capability | Agent Capability Service implementation over Agent Runs | Reuse/adapt | This is the current Agent Execution Engine. It should be wrapped before any rename to `alan-agent-engine`. |
| Provider clients and LLM retry behavior | `crates/llm`, `crates/runtime/src/llm.rs`, `crates/runtime/src/retry.rs` | Host Service Capability | Provider/runtime execution behind Agent Capability Service | Reuse | Kernel may model requested capability and result contract, not provider calls. |
| Operation and event protocol | `crates/protocol/src/op.rs`, `crates/protocol/src/event.rs` | Compatibility-only behavior; OS Primitive source | Compatibility adapter into commands, tasks, yields, evidence, and audit | Adapt | Preserve HTTP/WS client behavior while deriving semantic Alan OS events. |
| Spawn and child run protocol | `crates/protocol/src/spawn.rs`, `crates/runtime/src/runtime/child_runs.rs` | OS Primitive source; Host Service Capability | Agent Run hierarchy, delegate capability, task tree | Adapt | Child runs should become bounded delegated Agent Runs/tasks, not implicit nested root sessions. |
| Tool registry and builtin tools | `crates/runtime/src/tools`, `crates/tools` | Host Service Capability; Command Governance source | Command registry/execution backend | Adapt | Tools become one backend for commands and Agent Capability work, not the whole command model. |
| Virtual tools | `crates/runtime/src/runtime/virtual_tools.rs` | OS Primitive source; Alan Agent App Feature | Yield, plan, delegation, and task-control commands | Adapt | `request_confirmation`, `request_user_input`, and `update_plan` inform OS yield/task primitives; `invoke_delegated_skill` informs delegate descriptors. |
| Policy engine and governance profile | `crates/runtime/src/policy.rs`, `crates/protocol/src/op.rs` | Host Service Capability; Command Governance source | OS Command Governance Host Service API | Adapt/generalize | Preserve allow/deny/escalate, red-line, and audit ideas while widening beyond shell/tool calls. |
| Tool policy and approval checkpoints | `crates/runtime/src/runtime/tool_policy.rs`, `crates/runtime/src/approval.rs` | Host Service Capability; OS Primitive source | Command Governance plus yielded approvals | Adapt | Approval is an OS yield primitive; shell-specific policy matching stays service implementation detail. |
| Sandbox and execution guard selection | `crates/runtime/src/tools/sandbox_backend.rs`, `crates/runtime/src/tools/sandbox.rs` | Host Service Capability | Execution Guard Host Service API/implementation | Reuse/adapt | Kernel can record guard strength and effect class; concrete containment stays outside Kernel. |
| Skills and capability packages | `crates/runtime/src/skills`, `crates/runtime/skills` | Host Service Capability; Alan Agent App Feature; OS Primitive source | App/capability registry plus Agent Capability descriptors | Adapt | Package loading is a service/app concern; exported capabilities should map into descriptors and grants. |
| Dynamic tools and client capabilities | `crates/runtime/src/session.rs`, `crates/runtime/src/skills/capability_view.rs` | Host Service Capability; compatibility path | Host capability negotiation and command/result contracts | Adapt | Keep useful negotiation, but replace ad hoc tool payloads with typed Context Grants and Result Contracts where possible. |
| Memory recall, promotion, flush, and surfaces | `crates/runtime/src/runtime/memory_*`, `openspec/specs/runtime-memory-*` | Host Service Capability; rewrite candidate for ownership | User Memory, System Memory, and App Memory services | Adapt/rewrite | Reuse proven memory flows, but split ownership and app grants before platformizing. |
| Context compaction | `crates/runtime/src/runtime/compaction.rs`, `crates/runtime/src/session.rs` | Host Service Capability | Agent Run context lifecycle service | Adapt | Compaction remains execution-context management, not Kernel object storage. |
| Rollout JSONL, effects, checkpoints, evidence | `crates/runtime/src/rollout.rs` | OS Primitive source; Host Service Capability | Activity ledger, audit, evidence, compatibility persistence | Adapt | Preserve append-only evidence/audit discipline; replay must not re-execute side effects. |
| Runtime event stream and reconnect state | `crates/alan/src/daemon/*websocket*`, `session_store.rs`, `crates/tui/src/daemon_client.rs` | Host Service Capability; compatibility-only path | Event transport/reconnect Host Service API | Reuse/adapt | Keep stable for clients while semantic projections are introduced. |
| Task store, scheduling, sleep, and resume | `crates/alan/src/daemon/task_store.rs`, daemon session routes | Host Service Capability; OS Primitive source | OS task scheduling and Agent Run wake/resume service | Adapt | Task lifecycle is semantic; concrete timers and daemon storage remain service implementation. |
| TUI history reducer and transcript cells | `crates/tui/src/history.rs` | Alan Agent App Feature; compatibility-only path | Alan Agent conversation projection and terminal host rendering | Preserve/adapt | Useful parity baseline for semantic conversation snapshots; should not become Kernel state. |
| Prompt/persona assembly | `crates/runtime/src/prompts`, `crates/runtime/prompts` | Alan Agent App Feature; Host Service Capability | Alan Agent app definition and Agent Capability execution prompt assembly | Adapt | Prompt assembly should consume Context Grants and Result Contracts rather than raw app dumps. |
| Loop guard, response guardrails, repeat limits | `crates/runtime/src/runtime/loop_guard.rs`, `response_guardrails.rs`, `tool_orchestrator.rs` | Host Service Capability | Agent Run execution guardrails | Reuse/adapt | Execution safety remains service behavior surfaced through audit metadata. |
| Current daemon session API shape | `crates/alan/src/daemon/routes.rs`, `api_contract.rs` | Compatibility-only behavior; Host Service Implementation | Compatibility adapter to Host Service APIs | Preserve/adapt | Existing clients keep working while OS APIs are extracted. |
| Alan Agent product workspace UX | `crates/tui`, future `alan-agent` app module | Alan Agent App Feature | Built-in Agent Workspace | Adapt | Alan Agent is the place to inspect, steer, and organize sessions, Agent Runs, memory, evidence, and supervisor-raised tasks. |

## Cross-Reference Targets

When these areas are next touched, their specs should link back to
`agent-capability-os-model` and use the migration classes above:

- agent/session/runtime specs: distinguish Agent Session compatibility from
  bounded Agent Runs and Alan Agent App projections;
- governance/tooling specs: migrate tool policy into Command Governance and
  Execution Guard language;
- memory specs: split current memory behavior into User Memory, System Memory,
  and App Memory ownership;
- sandbox specs: treat sandboxing as Execution Guard implementation strength,
  not Kernel behavior;
- child-run and delegated-result specs: classify delegation as Agent Capability
  descriptors over bounded Agent Runs;
- TUI and macOS host specs: consume semantic projections and Host Service APIs
  rather than owning Agent Capability execution.

