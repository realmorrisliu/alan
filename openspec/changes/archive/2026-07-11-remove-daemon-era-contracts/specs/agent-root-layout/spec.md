## REMOVED Requirements

### Requirement: Default agent name semantics

**Reason**: The requirement defines agent selection through the retired Session and daemon request boundary.

**Migration**: Replace it with Process launch semantics that resolve an omitted or explicit `default` agent name through the same canonical root chain.

### Requirement: Named agent overlay order

**Reason**: The requirement scopes overlay resolution to a workspace Session rather than an Agent Process launch.

**Migration**: Preserve the overlay order under Process-shaped agent-definition resolution.

### Requirement: Repository hygiene reflects canonical roots

**Reason**: The requirement treats `.alan/sessions/` and session-shaped memory directories as the generated runtime-state contract.

**Migration**: Ignore generated Process, Agent Machine, rollout, checkpoint, and Memory Store state while keeping authored `.alan/agents/` roots trackable.

## ADDED Requirements

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
