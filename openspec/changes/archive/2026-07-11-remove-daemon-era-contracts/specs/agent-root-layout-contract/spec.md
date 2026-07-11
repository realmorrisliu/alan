## REMOVED Requirements

### Requirement: Centralized agent-name semantics

**Reason**: The scenarios name daemon callers as a canonical agent-name consumer.

**Migration**: Centralize the same normalization and validation for CLI and Agent Process launch callers.

### Requirement: Writers and readers use the same layout contract

**Reason**: The requirement assigns authored agent-root writes to daemon workspace APIs.

**Migration**: Keep reads and writes behind the runtime-owned layout API and direct CLI or authoring owners.

### Requirement: Client path mirrors are constrained

**Reason**: The requirement exists to coordinate daemon responses with an offline client-side path mirror.

**Migration**: Remove the mirror and require production consumers to use typed owner APIs or explicit mounted roots.

## ADDED Requirements

### Requirement: Agent-name semantics are shared by direct launch callers

Alan SHALL centralize agent-name normalization, validation, and `default` reservation semantics for CLI commands and Agent Process launch paths.

#### Scenario: Explicit default is normalized

- **WHEN** a CLI or Agent Process launch caller supplies agent name `default`
- **THEN** the runtime layout API selects the default root chain
- **AND** it does not construct a named overlay for `default`

#### Scenario: Named agent is validated

- **WHEN** a direct caller supplies a named agent value
- **THEN** the runtime layout owner validates it as a safe single path component
- **AND** the caller does not duplicate traversal or normalization rules

### Requirement: Direct readers and writers share the runtime layout owner

Alan SHALL use the runtime-owned layout contract for agent-root reads and for direct CLI or authoring writes. Production consumers MUST NOT construct canonical agent-root paths independently.

#### Scenario: Setup writes a loadable default config

- **WHEN** `alan init`, connection pinning, or another direct authoring flow writes a default `agent.toml`
- **THEN** it writes the path returned by the runtime layout owner
- **AND** Agent Process definition resolution reads the same path without a transport-specific mapping

#### Scenario: Renderer receives an explicit mounted root

- **WHEN** a renderer or host needs to inspect an Agent Process definition or file surface
- **THEN** it receives a typed path or mounted root from the owning boundary
- **AND** it does not recompute canonical paths from a duplicated client mirror
