# Proposal

## Why

Alan needs one Agent interaction and execution model for both exact commands and
reasoning. The current always-generate entry and separate Shell semantics prevent
users from issuing a predictable command while retaining the same context.

## What Changes

- **BREAKING**: `!` selects direct governed Host shell execution; `:` selects Agent interpretation; unprefixed input
  remains Agent-routed across terminal and redirected input.
- Execute deterministic commands within the same Agent Machine and governance
  boundary, with shared Process-owned cwd, ordered submissions and Action evidence.
- Reuse a mature Host shell for complete scripts; reserve only standalone user
  `cd` for shared cwd. Do not translate command strings or implement a shell.
- Keep aP internal behind task-oriented alan9 control commands; users need not
  learn the protocol and Agents use ordinary governed executable contracts.
- Make project read/edit/search and native commands use consistent public paths
  and the same authorized backing files, without a shadow project copy.
- Keep aP resource access and native execution distinct. Reuse Host Mount grants
  for HostFS and sandbox authority; resolve native paths only inside the Host adapter.
- Reconcile Linux reified mounts with native path identity; retain confinement
  and existing degraded-backend rules without requiring VM/FUSE deployment.
- Preserve non-owning terminal attachment; interrupt pauses queued work, detach
  leaves submitted work running, and recovery never automatically replays work.
- Expose command results to users directly and to later model input through bounded
  output and evidence references. Report missing response channels explicitly.
- Deliver explicit prefixes first. Keep unprefixed Agent behavior until typed
  classification passes shadow evaluation and an explicit activation decision.
- Preserve the interview's provenance in the
  [archived TUI handoff](../archive/2026-09-24-define-alan-interaction-model/unified-input-exploration.md).
  This change's `design-interview.md` and accepted decisions are authoritative;
  the completed TUI change does not reactivate its superseded desktop or
  background-launch plans.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `host-command-plane`: distinguish thin alan9 command clients from duplicate Host managers.

- `host-mount-service`: private grant-to-native-path resolution, preserving logical grants.
- `host-mount-tool-process-sandbox-projection`: shared launch authority with private native metadata.
- `os-sandbox-enforcement`: native project-path identity in reified views with unchanged confinement.

- `alan-shell`: unified entry routing, Host shell execution, aP path boundaries and redirected IO.
- `alan-renderer-host-contract`: route/cwd presentation, non-owning attachment,
  direct-command input and interruption behavior.
- `agent-namespace-runtime`: deterministic command transitions, ordered execution,
  Process-owned cwd and conservative restart handling.
- `agent-file-layout-contract`: observable submission/queue state and bounded
  command result projection over existing evidence owners.

- `coding-steward-contract`: preserve queue-pause precedence over follow-up scheduling.

## Impact

Affected implementation areas are CLI entry, the native shell execution adapter,
file-backed TUI, Agent Machine and AgentFS, and Process cwd/launch integration.
Reuse existing Process execution, permissions, sandbox and Action evidence.
Generic typed evaluation and provider capabilities remain dependencies owned by
`add-cognitive-model-routing`; Jev remains a candidate adapter, not a prerequisite
for explicit commands. No new Kernel Process type, global router, private renderer
executor, executable packaging framework or Host data migration is introduced.

Automatic typed routing, qualification and activation now belong to the active
[qualify-agent-input-routing](../qualify-agent-input-routing/) successor; no future-only
routing guarantees are included in this delivery's syncable deltas.

ADR-0058 records the accepted direction, with final user confirmation on
2026-09-24. No runtime behavior is claimed implemented. Canonical specs are synced only after
the corresponding implementation is delivered and merged.
