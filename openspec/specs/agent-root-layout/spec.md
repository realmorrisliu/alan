# agent-root-layout Specification

## Purpose
Define alan's durable external agent-root layout contract: canonical default and
named root paths, default agent semantics, overlay order, asset loading and
writing, singular `.alan/agent/` removal, and repository hygiene. This
capability owns user-visible layout behavior; implementation guardrails for how
code constructs these paths live in `agent-root-layout-contract`.

## Requirements
### Requirement: Canonical agent root directories
alan SHALL store every default and named agent definition root under an `agents`
directory. The default agent root SHALL be named `default`.

#### Scenario: Default roots use the reserved default directory
- **WHEN** alan resolves default agent roots for an alan home and a workspace
- **THEN** the global default root is `~/.alan/agents/default/`
- **AND** the workspace default root is `<workspace>/.alan/agents/default/`
- **AND** alan does not include `~/.alan/agent/` or `<workspace>/.alan/agent/` in the resolved roots

#### Scenario: Named roots remain under agents by name
- **WHEN** alan resolves a named agent root for `reviewer`
- **THEN** the global named root is `~/.alan/agents/reviewer/`
- **AND** the workspace named root is `<workspace>/.alan/agents/reviewer/`

### Requirement: Default agent name semantics
alan SHALL treat the agent name `default` as the reserved default agent identifier.
Omitting `agent_name` SHALL select the same agent definition as explicitly providing
`agent_name = "default"`.

#### Scenario: Omitted agent name selects default
- **WHEN** a session, CLI command, or daemon request omits `agent_name`
- **THEN** alan resolves only the default agent root chain
- **AND** the root chain is `~/.alan/agents/default/ -> <workspace>/.alan/agents/default/` when both scopes exist

#### Scenario: Explicit default selects default
- **WHEN** a session, CLI command, or daemon request sets `agent_name` to `default`
- **THEN** alan resolves the same root chain as omitted `agent_name`
- **AND** alan does not add a separate named overlay for `default`

#### Scenario: Default cannot be used as an ordinary named specialization
- **WHEN** a user creates files under `.alan/agents/default/`
- **THEN** alan treats those files as the default agent definition
- **AND** alan does not treat `default` as a named specialization layered on top of another default root

### Requirement: Named agent overlay order
alan SHALL resolve named agents by layering default roots first and the selected named
roots after them, preserving the existing precedence model with updated paths.

#### Scenario: Named workspace session resolves default then named roots
- **WHEN** alan resolves `agent_name = "reviewer"` for a workspace session
- **THEN** the root order is `~/.alan/agents/default/`
- **AND** then `<workspace>/.alan/agents/default/`
- **AND** then `~/.alan/agents/reviewer/`
- **AND** then `<workspace>/.alan/agents/reviewer/`

#### Scenario: Missing scopes are skipped without changing relative precedence
- **WHEN** alan resolves a named agent without an alan home or without a workspace
- **THEN** alan skips roots from the missing scope
- **AND** remaining default roots still appear before remaining named roots

### Requirement: Agent definition assets load from canonical roots
alan SHALL load agent-root `agent.toml`, `persona/`, `skills/`, and `policy.yaml`
assets from the resolved canonical roots only.

#### Scenario: Default config path changes
- **WHEN** alan loads the default global agent-facing config without `ALAN_CONFIG_PATH`
- **THEN** the default config path is `~/.alan/agents/default/agent.toml`
- **AND** `~/.alan/agent/agent.toml` is not read

#### Scenario: Workspace default assets load from new root
- **WHEN** a workspace default root contains `persona/`, `skills/`, or `policy.yaml`
- **THEN** alan loads those assets from `<workspace>/.alan/agents/default/`
- **AND** equivalent files under `<workspace>/.alan/agent/` are ignored

#### Scenario: Named agent assets extend default assets
- **WHEN** `agent_name = "reviewer"` and both default and reviewer roots contain assets
- **THEN** alan loads default assets from `.alan/agents/default/`
- **AND** alan loads reviewer assets from `.alan/agents/reviewer/`
- **AND** reviewer assets have higher overlay precedence than default assets in the same resolution chain

### Requirement: Writes target canonical default roots
alan SHALL create or update default agent configuration, policy, persona, and agent-root
skill files under `.alan/agents/default/`.

#### Scenario: Global setup writes default agent config
- **WHEN** setup or connection commands create the global default agent config
- **THEN** they write `~/.alan/agents/default/agent.toml`
- **AND** they do not create `~/.alan/agent/agent.toml`

#### Scenario: Workspace default writes use agents default
- **WHEN** workspace-scoped APIs or commands write default agent persona, policy, skill overrides, or skill packages
- **THEN** they write under `<workspace>/.alan/agents/default/`
- **AND** they do not create `<workspace>/.alan/agent/`

#### Scenario: Named writes still use the selected agent directory
- **WHEN** workspace-scoped APIs or commands write for `agent_name = "reviewer"`
- **THEN** they write under `<workspace>/.alan/agents/reviewer/`

### Requirement: Singular agent root removal
alan SHALL remove `.alan/agent/` from the agent-root contract. The singular path SHALL
not be a compatibility alias, fallback, or lower-precedence root.

#### Scenario: Old path exists next to new path
- **WHEN** both `<workspace>/.alan/agent/` and `<workspace>/.alan/agents/default/` exist
- **THEN** alan loads only `<workspace>/.alan/agents/default/`
- **AND** alan does not merge files from `<workspace>/.alan/agent/`

#### Scenario: Only old path exists
- **WHEN** `<workspace>/.alan/agent/` exists and `<workspace>/.alan/agents/default/` does not exist
- **THEN** alan does not load the old path as an agent root
- **AND** the workspace contributes no default agent-root overlay from the old path

#### Scenario: Diagnostics do not imply compatibility
- **WHEN** alan reports that `.alan/agent/` is no longer a supported root
- **THEN** the report is diagnostic only
- **AND** alan still does not read, write, merge, or migrate files from `.alan/agent/`

### Requirement: Repository hygiene reflects canonical roots
Repository ignore rules and documentation SHALL distinguish generated `.alan` runtime
state from authored agent definitions using the canonical `.alan/agents/` layout.

#### Scenario: Source-controlled agent roots remain trackable
- **WHEN** a workspace contains authored files under `.alan/agents/default/` or `.alan/agents/<name>/`
- **THEN** repository ignore rules allow those files to be tracked
- **AND** generated `.alan/sessions/` and `.alan/memory/` files remain ignored

#### Scenario: Old singular root is not allowlisted
- **WHEN** a workspace contains files under `.alan/agent/`
- **THEN** repository ignore rules do not allowlist that path as an authored agent root
- **AND** documentation instructs users to move authored files to `.alan/agents/default/`

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
