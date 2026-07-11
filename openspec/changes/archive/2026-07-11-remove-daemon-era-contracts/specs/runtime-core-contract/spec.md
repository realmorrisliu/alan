## REMOVED Requirements

### Requirement: Runtime core contracts live in OpenSpec
**Reason**: The Session-centered runtime-core capability is dissolved into the capabilities that own each durable invariant.
**Migration**: Use Agent Process, Agent Machine, AgentFS, Memory Store, policy, provider, and renderer capabilities.

### Requirement: Runtime object boundaries remain explicit
**Reason**: The requirement treats Session/app-server objects as the organizing runtime boundary.
**Migration**: Kernel and Agent Runtime Service ownership is defined by `plan9-kernel-substrate`, `agent-file-layout-contract`, and related capabilities.

### Requirement: Runtime durability and recovery stay auditable
**Reason**: Durability remains valid but is no longer Session-owned.
**Migration**: Rollout/checkpoint and Memory Store requirements move to `agent-file-layout-contract` and `runtime-memory-contract`.

### Requirement: App-server protocol objects remain stable
**Reason**: The app-server compatibility protocol is removed.
**Migration**: Surviving Event/Op records are internal Agent Execution Engine alphabet projected through owned files.

### Requirement: Compatibility session APIs map to protocol operations
**Reason**: Session APIs and compatibility mappings are removed without replacement.
**Migration**: Use Process and AgentFS file operations.

### Requirement: Input modes have first-class semantics
**Reason**: Input semantics remain valid but are not owned by a Session app-server contract.
**Migration**: Move framed input and turn/steering semantics to Agent IO and Agent Machine owners.

### Requirement: Events use cursor-based recovery
**Reason**: Session event cursors and daemon replay buffers are removed.
**Migration**: Use offset-readable Process and AgentFS Streams.

### Requirement: Session lifecycle distinguishes liveness from existence
**Reason**: Session is removed as a lifecycle entity.
**Migration**: Process state owns liveness; durable files own retained records.

### Requirement: Rollback and compaction expose durability limits
**Reason**: Rollback and compaction remain Agent Machine operations rather than Session API methods.
**Migration**: Use Agent Machine control/checkpoint files and runtime memory contracts.

### Requirement: Reconnect snapshots preserve mobile and TUI recovery state
**Reason**: Reconnect snapshots and their mobile/TUI Session projection are removed.
**Migration**: Renderers hydrate snapshots and streams under `/agent/<pid>/machine/ui` and resume from file offsets.

### Requirement: Errors, backpressure, and governance are protocol-visible
**Reason**: Session protocol visibility is no longer the contract owner.
**Migration**: Expose errors, requests, actions, and governance state through AgentFS and Process files.

### Requirement: Remote and relay routing preserve protocol authority
**Reason**: Daemon relay routing is removed.
**Migration**: Use `remote-access-service` and ordinary attached namespaces.

### Requirement: App-server protocol changes remain backward-compatible
**Reason**: The app-server protocol has no backward-compatibility promise after the clean break.
**Migration**: None.

### Requirement: Runtime confirmation resumes persist checkpoint records
**Reason**: The valid persistence invariant no longer belongs to a Session resume API.
**Migration**: Move it to Agent Machine checkpoint/request/action ownership.

### Requirement: Runtime confirmation checkpoints link to current tape roots when available
**Reason**: The valid checkpoint linkage no longer belongs to runtime-core Session compatibility.
**Migration**: Move it to Agent Machine tape/checkpoint ownership.
