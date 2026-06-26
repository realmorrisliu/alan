# Existing Alan Agent Runtime Migration Map

This map classifies current Alan Agent concepts against the Plan 9-style Agent
Process target model.

| Current concept | Current implementation | Target classification | Target shape | Migration note |
| --- | --- | --- | --- | --- |
| Session metadata | `crates/runtime/src/session.rs`, daemon session store | Compatibility transport | `/agent/<pid>/status` plus compatibility projection | Preserve existing session APIs until AgentFS attach reaches parity. |
| Conversation transcript | Tape/history reducers, TUI history | Agent IO | `/agent/<pid>/io/input`, `/agent/<pid>/io/output`, `/agent/<pid>/io/events` | Default user/app surface is IO, not tape. |
| Tape | `crates/runtime/src/tape.rs` | Agent Machine | `/agent/<pid>/machine/tape` | Preserve Turing-machine abstraction, but keep it runtime schema. |
| Rollout/event log | `rollout.rs`, event envelopes | Agent Machine / Agent IO streams | `/agent/<pid>/machine/events`, `/agent/<pid>/io/events` | Separate external events from transition/debug events. |
| Checkpoint/recovery | compaction, snapshots, reconnect | Agent Machine | `/agent/<pid>/machine/checkpoints` | Session reconnect becomes open/watch/checkpoint recovery. |
| Turn loop/execution engine | `crates/runtime/src/runtime/*` | Agent Runtime Service behavior | Agent Process execution backend | Reuse as future `alan-agent-engine`. |
| Provider clients/retry | `crates/llm`, runtime LLM client | Agent Runtime Service implementation | Not Kernel | Keep providers outside Kernel. |
| Built-in tools | `crates/tools`, runtime tool registry | Tool executables | `/bin/<tool>`, `/man/1/<tool>`, `/lib/exec/<tool>/manifest` | Runtime registry becomes compatibility cache/discovery, not authority. |
| Virtual tools | `request_confirmation`, `request_user_input`, `update_plan`, delegation | AgentFS request/action files or child Agent Process | `/requests`, `/actions`, child Agent Process | Replace private callbacks with file trees. |
| Skills | `crates/runtime/src/skills`, `runtime/skills` | Skill packages | `/lib/skill/<name>`, `/man/skill/<name>` descriptors | Canonical access is descriptor passing. |
| Policy/governance | `policy.rs`, `tool_policy.rs`, approval types | Policy descriptors + Agent Action Governance | `/lib/policy/*`, `/mnt/policy/*`, `/agent/<pid>/policy`, `/actions/<id>/approval` | OS access checks remain separate from agent action governance. |
| Sandbox/path guard | `tools/sandbox.rs`, policy backend | Agent Execution Guard | action execution guard metadata | Preserve as guard used when actions run. |
| Memory recall/promotion/flush | runtime memory modules | Memory Store descriptors | `/mnt/mem/*` descriptors, projected under context | No global agent memory registry. |
| Child agents/delegation | `child_runs.rs`, delegated skill handoff | Child Agent Process | parent/child Agent Process tree | "Subagent" becomes child Agent Process terminology. |
| Approval/yield/resume | protocol Yield, submit Resume | Agent Request files | `/agent/<pid>/requests/<id>` | Answer by writing response file. |
| Tool call traces | ToolCall events | Agent Action files | `/agent/<pid>/actions/<id>` | Link to `/proc/<tool-pid>` when a process exists. |
| Daemon HTTP/WS routes | `crates/alan/src/daemon/*` | Legacy transport compatibility | adapter over files/processes | Retire as architecture term, preserve while clients migrate. |
| TUI daemon client | `crates/tui/src/daemon_client.rs` | Current Alan Shell compatibility path | open/watch files in target model | Current path remains during migration. |
| Alan Agent UI | current TUI/future app module | Optional Agent Workspace | richer workspace over `/agent`, `/proc`, `/lib/skill`, `/man`, `/mnt/mem`, `/mnt/policy` | Built in, not required to run agents. |

## Follow-Up Spec Updates

- `define-plan9-kernel-substrate`: define a single `Process` category, the aP
  protocol, `/proc`, `/srv`, and namespace anchors (ADR-0024 D3). Supersedes the
  removed `add-agent-process-kernel-types`.
- `define-agent-file-layout-contract`: own the agent file-layout convention
  (agent-ness over ordinary Process), `/agent` as a view over `/proc`.
- `introduce-alan-kernel-runtime`: own the compatibility projection (engine →
  agent files). Absorbs the removed `add-agent-runtime-service-filesystem`.
- `migrate-alan-agent-to-agent-workspace`: keep Alan Agent built in but optional,
  as a file-client over agent files; Alan Shell remains the primary OS surface.
