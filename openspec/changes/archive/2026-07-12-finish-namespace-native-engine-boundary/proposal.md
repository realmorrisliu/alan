## Why

The Agent Execution Engine now generates through LLMFS and spawns Tool Processes
through `/proc`, but its live state still carries an in-process `ToolRegistry` and
publishes a parallel `RuntimeEventEnvelope` broadcast that drives UI projection
and delegated-child supervision. Those side channels contradict the canonical
namespace-native contract and leave the pre-file-native engine boundary alive
under new names.

## What Changes

- **BREAKING**: remove `RuntimeEventEnvelope`, `RuntimeHandle.event_sender`, the
  internal event-forwarding task, and host/test APIs that subscribe to the live
  broadcast.
- Make runtime components write their owning AgentFS files and streams directly;
  renderer hosts continue to hydrate and watch `io/`, `requests/`, `actions/`,
  `machine/tape`, and `machine/ui/` without an event projector as intermediate
  authority.
- Rework delegated-child supervision to observe `/proc`, the child AgentFS tree,
  and owned liveness/progress files instead of subscribing to child runtime
  events.
- Remove `ToolRegistry` from `RuntimeLoopState` and derive model-visible Tool
  definitions, capability classification, locality, and execution metadata from
  the mounted `/bin` entries and `/lib/exec/<tool>/manifest` files.
- Replace the registry-backed `RuntimeToolProcessRunner` materializer with
  executable owners reached through the Process namespace; no engine-private
  registry may execute a Tool effect or grant a Tool absent from the namespace.
- Collapse the single-variant `RuntimeEnvironment` wrapper into the namespace
  handle used by the engine loop.
- Keep semantic Event/Op record types only where they remain useful as file
  record schemas or transition-local values; the change does not preserve a
  publish/subscribe runtime transport.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-namespace-runtime`: make the namespace handle the literal sole engine
  environment, with no event sink or in-process Tool registry on the live path.
- `agent-file-layout-contract`: make mounted command/manifests and AgentFS files
  the complete Tool-discovery and runtime-state boundary.
- `governance-tooling-contract`: move Tool identity, capability, locality, and
  binding metadata from an in-process catalog authority to mounted executable
  package files and Process execution context.
- `agent-runtime-ui-file-surfaces`: require direct owner writes to `machine/ui/`
  snapshots and streams rather than projection from runtime events.
- `child-run-lifecycle`: derive progress and liveness from Process and file
  surfaces rather than a child runtime broadcast receiver.

## Impact

- Public and internal Rust APIs in `crates/agent-engine`, especially runtime
  engine, turn execution, Tool orchestration, child supervision, UI surfaces,
  tests, and host launch seams.
- Tool package composition under `/bin`, `/lib/exec`, and `/man`; the composition
  owner must mount complete Tool packages before the engine transition starts.
- Rust TUI remains file-backed and should require little product behavior change;
  engine and Alan integration tests must switch from event subscriptions to file
  observation.
- This change does not design Alan for macOS attachment, Service Manager boot, or
  host persistence roots.
- This change begins only after `clean-canonical-spec-debt`,
  `remove-residual-compatibility-shims`, and
  `remove-legacy-macos-persistence` are complete.
