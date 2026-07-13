# alan-os-system-store Specification

## Purpose
Defines channel-isolated durable backing ownership for Alan OS services,
ephemeral Process state, and ownership-safe migration from legacy Host paths.
## Requirements
### Requirement: Durable state uses a channel System Store
The Host SHALL provide one channel-isolated Alan OS System Store backing root.
Each durable File-Server Service SHALL own its subtree and format; Agent
Processes MUST NOT receive the raw backing path as identity or an implicit
mount.

#### Scenario: Stable and dev persist state
- **WHEN** stable and dev services write durable state
- **THEN** they write through separate System Store roots
- **AND** neither observes the other's state implicitly

### Requirement: Live Process state is not restored
Process tables, PIDs, descriptors, live namespaces, and runtime tasks SHALL be
ephemeral. Rollouts, Checkpoints, Memory Stores, packages, Agent Definitions,
and necessary service metadata MAY be durable under their owning stores.

#### Scenario: Host restarts
- **WHEN** an Alan OS Host restarts
- **THEN** it creates new Processes and PIDs
- **AND** prior work is available only through durable owners

### Requirement: Legacy state cleanup is ownership-safe
Upgrade cleanup SHALL delete only recognized generated state automatically,
SHALL migrate and verify legacy connection metadata before deleting it, and
SHALL require explicit import before removing possibly user-authored content.
No compatibility reader SHALL remain after migration.

#### Scenario: Authored Skill source is found
- **WHEN** cleanup finds a Skill under a former implicit Host-directory source
- **THEN** it reports the source without deleting or loading it
- **AND** removal is offered only after explicit import succeeds

#### Scenario: Explicit legacy roots follow the active channel
- **GIVEN** dev-channel inspection or cleanup receives an explicit Host project root
- **WHEN** it resolves former generated and authored source locations
- **THEN** it inspects `.alan-dev` and `.agents-dev` under that root
- **AND** stable `.alan` and `.agents` content remains untouched
