## REMOVED Requirements

### Requirement: Generated Workspace State Ignore Rules
**Reason**: The existing rule names generated Session directories as current state.
**Migration**: Ignore current Agent Machine, rollout/checkpoint, Memory Store, cache, and shell restoration paths.

### Requirement: Generated State Documentation
**Reason**: The existing documentation boundary describes generated Sessions and memory.
**Migration**: Distinguish authored AgentRoot content from generated Process/machine, rollout/checkpoint, Memory Store, cache, and shell state.

### Requirement: Generated workspace runtime state is channel-scoped
**Reason**: The channel contract treats Session paths as a current runtime owner.
**Migration**: Scope current generated owners by install channel and remove legacy Session paths.

## ADDED Requirements

### Requirement: Generated Process and machine state is ignored and separated from authored roots
Workspaces SHALL ignore generated Agent Process, Agent Machine, rollout/checkpoint, Memory Store,
cache, and shell restoration state while preserving authored AgentRoot definitions, policies,
Skills, and models as reviewable content.

#### Scenario: Generated runtime state exists in a workspace
- **WHEN** Alan writes current generated runtime state beneath a workspace `.alan` tree
- **THEN** repository ignore rules cover the generated paths
- **AND** documentation associates each path with its Process, machine, rollout, Memory Store, cache, or shell owner

### Requirement: Generated runtime state is channel-scoped by its actual owner
Stable and dev channels SHALL use distinct generated Process/machine, rollout/checkpoint, Memory
Store, cache, auth, registry, and shell-state roots.

#### Scenario: Stable and dev operate on the same workspace
- **WHEN** both channels create generated state
- **THEN** each reads and writes only its channel-owned generated roots
- **AND** the authored AgentRoot and workspace content remain shared according to their contracts
