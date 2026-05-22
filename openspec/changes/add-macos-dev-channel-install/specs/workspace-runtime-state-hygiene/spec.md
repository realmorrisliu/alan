## ADDED Requirements

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
