## Why

The accepted file-native architecture is not yet complete: Agent Execution
Engine still receives Kernel-shaped launch and lifecycle collaborators, while
Host Mount requests and Tool sandbox projection still carry raw Host OS paths
through Agent Machine and evidence. These transitional seams put Alan OS and
Host adapter authority in the transition engine.

## What Changes

- **BREAKING** Remove `host_path` from the `request_mount` request, AgentFS
  projection, Tool result, rollout, and audit contracts. Requests carry only the
  Alan OS `/mnt` path, access, reason, and optional label.
- Make Host Mount Service the sole request, native approval, grant, projection,
  revocation, status, and audit authority through a clone-via-open service tree;
  the Host adapter alone sees and authorizes the raw Host OS path.
- Replace raw Host Mount backing and aggregate `HostMounts` inheritance with
  explicit opaque grant handles or mounted file-tree descriptors. Grant IDs are
  metadata, never authority, and child Processes receive no Host Mount by
  default.
- Launch child Agent Processes as ordinary `/proc/clone` executions of
  `/bin/alan-agent`; Agent Runtime Service implements the Agent Executable and
  owns AgentFS binding, namespace assembly, connection selection, Machine
  startup, and lifecycle cleanup.
- Delete engine-owned launch context, child assembly/lifecycle callbacks, live
  mount applicators, and Kernel-shaped DTOs after their responsibilities move.
- Remove the normal `alan-agent-engine` dependency on `alan-kernel`; a
  development-only dependency for public contract tests remains allowed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-namespace-runtime`: Complete the Agent Runtime Service/Agent Executable
  assembly boundary and remove Kernel-shaped engine collaborators.
- `host-mount-service`: Define the logical request tree, native approval
  authority, opaque grant handles, and terminal request states.
- `process-launch-context`: Replace engine-owned raw Host Mount backing and
  aggregate inheritance with explicit namespace mounts and descriptors.
- `agent-mount-escalation`: Remove raw Host paths and engine-local approval from
  the Agent-visible mount request contract and evidence.
- `live-mount-grant-namespace-projection`: Move live projection from an engine
  applicator callback to Host Mount Service handle projection.
- `mount-grant-tool-sandbox-projection`: Derive native Tool sandbox authority
  from Host Mount Service grants inside Host adapters rather than engine-owned
  raw writable roots.

## Impact

This change follows `deepen-agent-machine-transition-module` and affects Agent
Runtime Service, Service Manager composition, Agent Execution Engine, Host Mount
Service, Host adapters, Tool Process launch, AgentFS request projection, rollout
records, and the affected aP file contracts. The old request and flat service
surfaces are removed rather than retained as compatibility paths; callers and
fixtures must migrate atomically in focused stacked PRs.
