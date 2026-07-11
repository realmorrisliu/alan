## REMOVED Requirements

### Requirement: Renderer hosts read files and write `ctl`

**Reason**: The requirement still contrasts file surfaces with Session view models.

**Migration**: State the file projection and control boundary directly.

### Requirement: Renderer hosts SHALL treat compatibility transports as transitional during migration

**Reason**: The clean break does not retain or authorize a compatibility transport.

**Migration**: None. Renderer hosts use mounted file surfaces.

### Requirement: Local file-backed renderer hosts can launch from a mounted namespace

**Reason**: The requirement defines launch sufficiency by negating daemon Session creation and attachment.

**Migration**: Define the mounted namespace and concrete Process path as sufficient inputs.

## ADDED Requirements

### Requirement: Renderer hosts project mounted Alan OS file state

Alan renderer hosts SHALL derive durable presentation state from files under `/proc`, `/agent`, and mounted service trees, and SHALL translate user actions into file or `ctl` writes.

#### Scenario: Renderer host boundary is reviewed

- **WHEN** an Alan renderer host is reviewed
- **THEN** its durable truth source is the mounted Alan OS namespace
- **AND** it owns presentation only, not Process, Agent Machine, or service truth

### Requirement: A mounted namespace is sufficient for local renderer launch

A local renderer host SHALL start from a mounted Alan OS root plus a concrete Agent Process path.

#### Scenario: Renderer opens a root Agent Process

- **WHEN** the renderer receives a namespace root and `/agent/root`
- **THEN** it reads and tails AgentFS output and state files
- **AND** it writes input and Process control through the corresponding files
