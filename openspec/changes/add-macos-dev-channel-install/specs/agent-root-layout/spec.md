## ADDED Requirements

### Requirement: Global agent roots are channel-scoped
Alan SHALL resolve global agent roots from the active install channel's alan
home. Existing `~/.alan/...` global agent-root paths remain the stable-channel
paths; the dev channel SHALL use equivalent paths under `~/.alan-dev/...`.

#### Scenario: Stable default agent root is resolved
- **WHEN** stable-channel `alan` resolves the default global agent root
- **THEN** the root is `~/.alan/agents/default/`
- **AND** existing stable root overlay behavior remains unchanged

#### Scenario: Dev default agent root is resolved
- **WHEN** dev-channel `alan-dev` resolves the default global agent root
- **THEN** the root is `~/.alan-dev/agents/default/`
- **AND** it does not read `~/.alan/agents/default/` as a fallback

#### Scenario: Dev named agent root is resolved
- **WHEN** dev-channel `alan-dev` resolves `agent_name = "reviewer"`
- **THEN** the dev global named root is `~/.alan-dev/agents/reviewer/`
- **AND** stable `~/.alan/agents/reviewer/` is not part of the dev global overlay chain

### Requirement: Agent-root writes use the active channel
Setup, connection pinning, workspace setup, and agent-root mutation flows SHALL
write global agent-root files under the active channel's alan home.

#### Scenario: Stable setup writes global config
- **WHEN** stable-channel setup creates a global default `agent.toml`
- **THEN** it writes `~/.alan/agents/default/agent.toml`
- **AND** it does not create dev-channel global config files

#### Scenario: Dev setup writes global config
- **WHEN** dev-channel setup creates a global default `agent.toml`
- **THEN** it writes `~/.alan-dev/agents/default/agent.toml`
- **AND** it does not create or mutate `~/.alan/agents/default/agent.toml`

#### Scenario: Workspace agent root remains workspace-scoped
- **WHEN** either channel resolves authored workspace agent roots
- **THEN** the workspace default root remains `<workspace>/.alan/agents/default/`
- **AND** the workspace named root remains `<workspace>/.alan/agents/<name>/`
- **AND** channel isolation applies to global roots and generated runtime state, not to authored workspace roots
