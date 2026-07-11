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
- **WHEN** alan resolves default agent roots for the active alan home and a workspace
- **THEN** the global default root is `<alan-home>/agents/default/`
- **AND** the workspace default root is `<workspace>/.alan/agents/default/`
- **AND** alan does not include `<alan-home>/agent/` or `<workspace>/.alan/agent/` in the resolved roots

#### Scenario: Named roots remain under agents by name
- **WHEN** alan resolves a named agent root for `reviewer` in the active alan home
- **THEN** the global named root is `<alan-home>/agents/reviewer/`
- **AND** the workspace named root is `<workspace>/.alan/agents/reviewer/`

### Requirement: Agent definition assets load from canonical roots
alan SHALL load agent-root `agent.toml`, `persona/`, `skills/`, and `policy.yaml`
assets from the resolved canonical roots only.

#### Scenario: Default config path changes
- **WHEN** alan loads the default global agent-facing config without `ALAN_CONFIG_PATH`
- **THEN** the default config path is `<alan-home>/agents/default/agent.toml`
- **AND** `<alan-home>/agent/agent.toml` is not read

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
- **THEN** they write `<alan-home>/agents/default/agent.toml`
- **AND** they do not create `<alan-home>/agent/agent.toml`

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

### Requirement: Process launch uses default agent-name semantics

Alan SHALL treat `default` as the reserved default agent-definition identifier when launching an Agent Process. Omitting the agent name and explicitly selecting `default` SHALL resolve the same canonical default root chain.

#### Scenario: Agent Process launch omits agent name

- **WHEN** an Agent Process is launched without an agent name
- **THEN** Alan resolves `<alan-home>/agents/default/ -> <workspace>/.alan/agents/default/` for the scopes that exist
- **AND** Alan does not add a named overlay

#### Scenario: Agent Process launch selects default explicitly

- **WHEN** an Agent Process is launched with agent name `default`
- **THEN** Alan resolves the same root chain as an omitted name
- **AND** `default` is not treated as an ordinary named specialization

### Requirement: Named Agent Processes resolve default roots before named roots

Alan SHALL resolve a named Agent Process by layering canonical default roots before the selected named roots while preserving scope precedence.

#### Scenario: Reviewer Agent Process is launched in a workspace

- **WHEN** Alan resolves agent name `reviewer` for an Agent Process with global and workspace scopes
- **THEN** the root order is `<alan-home>/agents/default/`, `<workspace>/.alan/agents/default/`, `<alan-home>/agents/reviewer/`, then `<workspace>/.alan/agents/reviewer/`
- **AND** missing scopes are skipped without changing the relative default-before-named order

### Requirement: Repository hygiene distinguishes authored definitions from generated Process state

Repository ignore rules and current documentation SHALL keep authored files under `.alan/agents/` trackable while excluding generated Process, Agent Machine, rollout, checkpoint, and Memory Store state.

#### Scenario: Workspace contains authored and generated Alan state

- **WHEN** a repository contains authored agent definitions and generated execution state
- **THEN** files under `.alan/agents/default/` and `.alan/agents/<name>/` remain trackable
- **AND** generated Process, machine, rollout, checkpoint, and Memory Store paths are ignored through their canonical owners
- **AND** current documentation names each generated path by its owning component
