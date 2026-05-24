# workspace-runtime-state-hygiene Specification

## Purpose
Define workspace runtime-state hygiene requirements: generated `.alan` session
and memory state should stay out of normal source control, authored agent
definitions should remain trackable, alan home should not become a nested
workspace, and workspace identity comparisons should use canonical paths where
available.
## Requirements
### Requirement: Generated Workspace State Ignore Rules
Repository ignore rules SHALL ignore generated workspace `.alan` runtime state by default while allowing authored agent definitions to remain trackable.

#### Scenario: Generated sessions and memory exist
- **WHEN** a workspace contains generated `.alan` runtime state such as `.alan/sessions/` or `.alan/memory/`
- **THEN** normal repository status does not show those generated files as untracked source changes

#### Scenario: Authored agent definitions exist
- **WHEN** a workspace contains `.alan/agents/default/`, `.alan/agents/<name>/`, `.alan/models.toml`, policies, or authored skill packages intended for source control
- **THEN** repository ignore rules do not prevent those authored files from being tracked
- **AND** documentation explains that `.alan/agents/default/` is the workspace default agent definition root while `.alan/agents/<name>/` is the workspace named-agent definition root

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

### Requirement: Generated State Documentation
The documentation SHALL explain which `.alan` paths are generated runtime state and which paths may be source-controlled.

#### Scenario: Developer reads workspace state docs
- **WHEN** a developer checks the repository documentation for `.alan` workspace state
- **THEN** the docs identify generated sessions/memory paths separately from authored agent roots, policies, and skills

### Requirement: Generated workspace runtime state is channel-scoped
Generated workspace runtime state SHALL include an install-channel namespace so
stable Alan and Alan Dev do not read or overwrite each other's workspace-local
sessions, memory, caches, shell restore state, or runtime metadata.

#### Scenario: Dev channel writes workspace runtime state
- **WHEN** Alan Dev creates generated runtime state for a workspace
- **THEN** the state is written under a dev-channel generated path such as `<workspace>/.alan/runtime/dev/`
- **AND** it is not written to legacy stable generated paths such as `<workspace>/.alan/sessions/` or `<workspace>/.alan/memory/`

#### Scenario: Stable channel reads legacy state
- **WHEN** stable Alan reads existing legacy generated workspace state
- **THEN** it may continue to read stable-compatible legacy paths for compatibility
- **AND** it does not treat dev-channel generated state as stable runtime state

#### Scenario: Both channels use the same workspace
- **WHEN** stable Alan and Alan Dev both open the same source workspace
- **THEN** each channel can maintain its own generated runtime state
- **AND** session, memory, cache, shell restore, and runtime metadata written by one channel are not consumed as authoritative state by the other channel

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
