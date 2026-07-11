## Why

Alan's canonical contracts still preserve daemon, HTTP/WebSocket session, relay, and
session-centered runtime concepts that the accepted Alan OS model has already rejected. Those
contracts keep obsolete authority boundaries alive and make future work choose between two
incompatible architectures, so they must be removed before any Alan for macOS attachment design is
chosen.

## What Changes

- **BREAKING** Remove `daemon-api-contract`, `remote-control-contract`, and the session-centered
  `runtime-core-contract` from the canonical capability set rather than freezing them as supported
  compatibility surfaces.
- **BREAKING** Remove daemon/session/HTTP/WebSocket/relay/reconnect requirements from every
  canonical capability and active planning artifact; archive history remains unchanged and
  non-normative.
- Redistribute durable behavior to its actual owners: Process and Agent Process lifecycle,
  Agent Machine tape/checkpoints, AgentFS files, rollout/checkpoint persistence, Memory Stores,
  namespace assembly, policy, providers, Skills, and renderer file projections.
- Remove Session as a domain object without replacing it with Thread, Conversation, Run, or another
  center object. Process owns lifecycle, Agent Machine owns transition state, turns remain bounded
  transitions, and Memory Stores own continuity.
- Fold the completed target contracts from `define-alan-app-service-integration` and
  `define-remote-access-service` into the canonical spec set, then retire those superseded planning
  changes without carrying their daemon-era predecessor contracts forward.
- Rewrite current ADRs, `CONTEXT.md`, `AGENTS.md`, README/help text, and active OpenSpec changes so
  none describes the daemon or Session compatibility model as current, transitional, or required.
- Define a zero-tolerance deletion inventory for the stacked implementation change, which adds the
  durable semantic guard against Alan daemon/session API surfaces returning.
- Keep Alan for macOS attachment transport, lifecycle, process topology, and client API explicitly
  undecided.

## Capabilities

### New Capabilities

- `alan-app-service-integration`: Canonical file-server, mount, descriptor, and Agent Executable
  integration contract for Alan Apps and host-backed capabilities, folded from the superseded
  contract-only change.
- `remote-access-service`: Canonical Alan OS remote-entry contract that replaces the daemon-era
  remote-control surface, folded from the superseded contract-only change.

### Modified Capabilities

- `daemon-api-contract`: Remove the entire capability from the canonical spec set.
- `remote-control-contract`: Remove the entire capability from the canonical spec set.
- `runtime-core-contract`: Remove the session/app-server compatibility capability and re-home its
  still-valid invariants under their actual owners.
- `agent-file-layout-contract`: Own Agent Machine tape, checkpoint, control, IO, request, action,
  and persistence-facing file semantics without Session identity.
- `agent-runtime-ui-file-surfaces`: State the positive file projection contract without daemon
  hydration or session compatibility clauses.
- `runtime-memory-contract`: Replace session-local and cross-session memory semantics with
  Agent-Process-local working memory, episodic execution records, handoffs, and Memory Store
  ownership.
- `runtime-memory-surfaces`: Remove session-summary/source-session terminology and bind generated
  memory surfaces to Agent Processes, turns, rollouts, and handoffs.
- `child-run-lifecycle`: Replace parent/child session identity and session recovery with parent/child
  Agent Process identity and file-backed lifecycle truth.
- `namespace-sandbox-projection`: Replace session assembly and fixed-session scope with per-Process
  namespace construction.
- `os-sandbox-enforcement`: Replace session confinement wording with Process execution and sandbox
  projection ownership.
- `governance-tooling-contract`: Replace session-scoped policy and audit identity with Agent Process,
  Tool Process, turn, and capability-call ownership.
- `coding-steward-contract`: Replace parent session and session recovery references with parent
  Agent Process and checkpoint/rollout semantics.
- `workspace-runtime-state-hygiene`: Replace generated session paths and channel session state with
  Agent Machine, rollout/checkpoint, and Memory Store paths.
- `provider-connection-contract`: Remove daemon connection-management routes and retain CLI,
  provider/connection ownership, secret storage, and future file-server composition only.
- `provider-request-controls`: Remove daemon/session metadata mirroring and keep resolver-owned
  request controls.
- `openrouter-provider-adapter`: Remove daemon catalog consumers while retaining provider descriptor
  and connection behavior.
- `skill-system-contract`: Remove daemon catalog/override APIs and retain package, CLI, prompt, and
  namespace projection semantics.
- `agent-root-layout`: Remove session/daemon resolution wording and bind definition resolution to
  Agent Process creation and workspace context.
- `agent-root-layout-contract`: Remove daemon-provided path and daemon mutation scenarios.
- `rust-inline-tui`: Express the file-backed renderer positively without daemon/session migration
  language.
- `alan-renderer-host-contract`: Express direct file-client ownership without a transitional
  compatibility transport allowance.
- `shell-workspace-core-contract`: Remove daemon comparison wording while preserving host-shell
  domain independence.
- `macos-app-architecture-maintainability`: Remove daemon API client/reducer and legacy Console
  ownership requirements; do not define the future Alan OS attachment.
- `macos-shell-terminal-lifecycle`: Remove future daemon-owned PTY continuity as a permitted target.
- `macos-shell-workspace-persistence`: Replace Alan Session activity with terminal-observed CLI
  agent activity while retaining legitimate terminal transcript continuity semantics.
- `macos-terminal-activity-semantics`: Replace Alan Session identity with conservative,
  pane-scoped CLI process and structured agent activity.
- `macos-shell-build-test-contract`: Remove daemon endpoint/channel verification and cover deletion
  of the legacy consumers instead.
- `macos-shell-ui-ux-conformance`: Remove daemon/session Settings and legacy Console requirements
  without inventing replacement Alan OS UI.
- `runtime-harness-contract`: Remove daemon bridge-controller ownership and session-shaped harness
  concepts.
- `sandbox-autonomy-invariants`: Replace daemon event-buffer recovery language with authoritative
  AgentFS/Process file state.
- `tool-result-presentation`: Remove daemon-authored presentation wording and keep runtime/file
  projection ownership.
- `rust-test-placement-contract`: Remove daemon-route/WebSocket categories and describe tests by
  their durable service, transport, and file-surface owners.
- `plan9-kernel-substrate`: State Kernel dependency isolation without retaining a legacy Session
  protocol as a named comparison boundary.

## Impact

- Canonical OpenSpec capabilities, current ADRs, glossary and agent guide, public docs/help, and all
  active changes that mention daemon/session compatibility.
- The accepted target becomes intentionally ahead of the old implementation only for the short
  stacked interval before `remove-daemon-era-implementation` lands.
- `remove-daemon-era-implementation` is a required stacked successor and must be implementation-
  complete and review-green before this change merges.
- No Alan for macOS replacement architecture or compatibility window is introduced.
