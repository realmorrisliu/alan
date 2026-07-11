# Contract Removal Review Evidence

Snapshot date: 2026-07-11.

## Inventory boundary

The audit covers:

- all 67 canonical capability specs under `openspec/specs/`;
- every non-archived change reported by `openspec list --json`;
- `README.md`, `AGENTS.md`, `CONTEXT.md`, current `docs/`, and ADRs;
- `alan --help` plus every current direct command family;
- Rust and Apple production source, tests, fixtures, scripts, Just recipes,
  Cargo manifests, and Xcode source membership;
- generated stable/dev runtime path helpers and inspected local state.

`openspec/changes/archive/` is immutable history and is excluded from current
authority. The two predecessor changes were archived with `--skip-specs` after
their accepted positive requirements were folded here:

- `2026-07-11-define-alan-app-service-integration`;
- `2026-07-11-define-remote-access-service`.

No other active change names either predecessor as its authority after the
reference update.

## Term classification

| Class | Examples | Treatment |
| --- | --- | --- |
| retired Alan architecture | host server modules, public control command, Agent Session identity, client routes, network renderer transport, remote-control state, scheduled runtime extensions | delete or replace with the narrower surviving owner |
| legitimate terminal/platform term | PTY session, terminal find session, Mac login session, Apple LaunchDaemon, Swift scheduling helper | retain and explicitly allowlist |
| legitimate provider/auth/third-party term | ChatGPT auth session, OpenRouter `session_id`, HTTP provider clients, Docker daemon | retain under the owning adapter/tool |
| immutable history | files below `openspec/changes/archive/` | preserve unchanged and exclude from current guards |
| false positive | `ResolvedAgentDefinition` containing the byte sequence `AgentD` | ignore; guards use semantic/word-boundary matching |

The inventory contains no replacement Thread, Conversation, globally
addressable Run, execution manager, or Alan for macOS attachment decision.
Child Run Registration remains Process-local delegation metadata; live child
lifecycle is authoritative in `/proc`.

## `daemon-api-contract` ownership matrix

| Requirement | Outcome |
| --- | --- |
| Canonical Endpoint Registry | obsolete; no product endpoint owner remains |
| Shared URL Construction | obsolete |
| Remote Access Scope Metadata | remote principal/lease policy belongs to `remote-access-service`, not a URL schema |
| Relay Policy Metadata | obsolete |
| Generated Client Endpoint Helpers | obsolete |
| Protocol And Payload Drift Checks | file/protocol conformance moves to AgentFS, aP, and renderer tests |
| Public Route Compatibility | obsolete; no compatibility promise |
| Session route semantics match runtime protocol mapping | obsolete |
| Raw Route String Guardrail | replaced by the semantic current-tree absence guard |
| Rust TUI client preserves session API compatibility | replaced by `rust-inline-tui` mounted-file requirements |

The canonical capability directory is deleted when this change is synchronized.

## `remote-control-contract` ownership matrix

| Requirement | Outcome |
| --- | --- |
| Remote control contracts live in OpenSpec | current remote entry requirements belong to `remote-access-service` |
| Remote governance cannot bypass local policy | retained by Process credentials, namespace authority, and governance contracts |
| Remote control topology preserves node authority | retained as destination-host Process authority |
| Direct and relay transports expose explicit MVP surfaces | obsolete |
| Relay node discovery and sticky binding are deterministic | obsolete |
| Reconnect snapshots preserve remote continuity without re-execution | lease reattachment plus stream offsets belongs to `remote-access-service` |
| Remote notification signals are informational | ordinary file/stream observation replaces this transport-specific rule |
| Remote reconnect and multi-client consistency use node-authored cursors | service-owned stream offsets and file semantics own consistency |
| Remote metadata extends protocol without changing runtime semantics | lineage-local remote context files own provenance |
| Remote auth scopes and daemon configuration are explicit | remote principal, entry ticket, lease, and lineage policy own authority |
| Relay credentials and runtime configuration are scoped and revocable | transport-specific configuration is obsolete; principal/lease revocation survives |
| Remote security preserves replay integrity and audit trails | Process, namespace, file-operation, and lease evidence own audit |
| Local daemon defaults are channel-scoped | obsolete |

The canonical capability directory is deleted when this change is synchronized.

## `runtime-core-contract` ownership matrix

| Requirement | Outcome |
| --- | --- |
| Runtime core contracts live in OpenSpec | surviving requirements move to their concrete capabilities |
| Runtime object boundaries remain explicit | Process, Agent Machine, AgentFS, rollout, checkpoint, and Memory Store capabilities |
| Runtime durability and recovery stay auditable | rollout/checkpoint evidence |
| App-server protocol objects remain stable | obsolete |
| Compatibility session APIs map to protocol operations | obsolete |
| Input modes have first-class semantics | Event/Op execution alphabet and AgentFS input files |
| Events use cursor-based recovery | AgentFS offset-readable streams and retained-data gap semantics |
| Session lifecycle distinguishes liveness from existence | Process lifecycle and Agent Machine evidence |
| Rollback and compaction expose durability limits | Agent Machine plus rollout/checkpoint evidence |
| Reconnect snapshots preserve mobile and TUI recovery state | obsolete; renderer recovery uses file snapshots and offsets |
| Errors, backpressure, and governance are protocol-visible | AgentFS requests/notices, Tool audit, and Process status files |
| Remote and relay routing preserve protocol authority | remote entry Process and aP file semantics; old routing owner obsolete |
| App-server protocol changes remain backward-compatible | obsolete |
| Runtime confirmation resumes persist checkpoint records | request answer transition plus checkpoint evidence |
| Runtime confirmation checkpoints link to current tape roots when available | checkpoint/evidence capabilities |

The canonical capability directory is deleted when this change is synchronized.

## Current authority result

- lifecycle: Process and `/proc`;
- machine state: Agent Machine;
- agent IO and control: AgentFS;
- execution evidence: rollout and checkpoint files;
- continuity: Working Memory, Episodic Memory, handoff, and other Memory Stores;
- rendering: mounted snapshots, streams, and control writes;
- connection management: direct CLI plus owning connection/auth files;
- macOS terminal activity: terminal-observed Process/invocation signals.

Alan for macOS attachment transport, Process topology, lifecycle ownership, and
client API remain deliberately unspecified. ADR-0029 is the only current
decision for that boundary.

## Current guide and legacy-path audit

- Every tracked file under `docs/spec/` is deleted; immutable OpenSpec archives
  remain unchanged.
- Canonical capabilities no longer contain scenarios that require opening or
  preserving a named `docs/spec/` bridge. Remaining generic mentions only ban
  new parallel contracts or define documentation-governance checks.
- `docs/testing_strategy.md` treats Event/Op only as the Agent Execution Engine
  and AgentFS execution alphabet and names no server, route, or daemon client.
- `docs/live_runtime_smoke.md` describes an Agent Execution Engine turn and
  Process-to-Process continuity, not Session event streaming.
