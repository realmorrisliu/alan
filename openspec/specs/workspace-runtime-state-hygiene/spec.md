# workspace-runtime-state-hygiene Specification

## Purpose
Define workspace runtime-state hygiene requirements: generated Process, Agent
Machine, rollout, checkpoint, and Memory Store state should stay out of normal
source control, authored agent definitions should remain trackable, alan home
should not become a nested workspace, and workspace identity comparisons should
use canonical paths where available.
## Requirements
### Requirement: alan Home Workspace State
The system SHALL prevent alan home from being treated as a normal workspace that creates nested `~/.alan/.alan/` runtime state.

#### Scenario: alan home is selected as workspace
- **WHEN** the resolved workspace root is alan home
- **THEN** runtime state paths resolve to the canonical alan home layout rather than appending another `.alan`

#### Scenario: Legacy nested state exists
- **WHEN** legacy nested alan-home runtime state is detected
- **THEN** the system reports the condition safely without deleting data implicitly

### Requirement: Canonical Workspace Identity
The system SHALL compare workspace identities using canonical paths where available.

#### Scenario: Same workspace uses path casing variants
- **WHEN** two paths refer to the same workspace on a case-insensitive filesystem
- **THEN** runtime manager and registry identity checks resolve them to one workspace identity

#### Scenario: Path cannot be canonicalized
- **WHEN** a workspace path cannot be canonicalized because it does not exist yet
- **THEN** the system uses a deterministic normalized fallback and canonicalizes after creation where practical

### Requirement: Authored workspace content remains shared by workspace semantics
Channel isolation SHALL NOT make authored workspace agent definitions or
workspace public skills private to one install channel.

#### Scenario: Authored workspace agent root exists
- **WHEN** either channel resolves `<workspace>/.alan/agents/default/`
- **THEN** the authored workspace root remains available according to normal agent-root overlay rules
- **AND** the channel does not create a duplicate authored workspace root under a channel-specific source-controlled path

#### Scenario: Workspace public skill package exists
- **WHEN** either channel discovers `<workspace>/.agents/skills/<skill>/SKILL.md`
- **THEN** the skill package remains workspace-authored content
- **AND** generated evaluation output, caches, or runtime state for that skill remain channel-scoped when written by Alan

### Requirement: Ignore rules cover channel-scoped generated state
Repository hygiene rules SHALL ignore channel-scoped generated workspace runtime
state while continuing to allow authored workspace definitions to be tracked.

#### Scenario: Channel generated state exists
- **WHEN** a workspace contains generated state under a channel-scoped runtime path
- **THEN** normal repository status does not show that generated state as untracked source changes
- **AND** authored `.alan/agents/` and `.agents/skills/` paths remain trackable when intentionally committed

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
