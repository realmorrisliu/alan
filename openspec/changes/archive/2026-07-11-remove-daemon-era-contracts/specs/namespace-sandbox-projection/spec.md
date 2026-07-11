## MODIFIED Requirements

### Requirement: One mount declaration list projects into two enforcement mechanisms
Alan OS SHALL assemble one mount declaration list for each Process creation and project it into both
the Alan namespace and the host OS sandbox. The two projections SHALL derive from the same bounded
delegation decision recorded for that Process creation.

#### Scenario: A Process is spawned with bounded mounts
- **WHEN** a parent spawns a child Process with delegated mount declarations
- **THEN** Alan assembles the child namespace and host sandbox projection from the same list
- **AND** the list is attributable to the child Process lifecycle

### Requirement: The projection preserves crate layering
The composition root that creates a Process SHALL assemble the mount and sandbox projections. Alan
Kernel, File-Server Services, clients, and provider backends SHALL NOT acquire cross-layer
dependencies to reconstruct that composition.

#### Scenario: Projection ownership is reviewed
- **WHEN** mount-to-sandbox projection code is inspected
- **THEN** assembly remains at the Process composition root
- **AND** the composition root remains the only owner that joins both projections

### Requirement: Mounts are authorized outside the agent's control
Mount visibility and access SHALL be fixed when the Process namespace is assembled unless an
authorized external actor applies an explicit namespace change. An Agent Process SHALL NOT amplify
its mounts through an internal mount tool.

#### Scenario: Agent Process starts with only a workspace
- **WHEN** the child namespace contains only the delegated workspace and standard system mounts
- **THEN** the host sandbox contains the matching projection
- **AND** the Agent Process cannot add an undelegated mount
