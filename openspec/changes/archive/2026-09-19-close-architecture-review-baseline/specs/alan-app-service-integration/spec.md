## MODIFIED Requirements

### Requirement: UI, Tools, and Agent Processes share the same authority tree
Alan OS renderers, Tool Processes, and Agent Processes SHALL derive domain behavior
from the same mounted service tree. Host-local view models, caches, and snapshots MAY optimize
presentation but SHALL remain projections and MUST NOT become a second source of domain truth. This
requirement does not choose how any client attaches to Alan OS. An external
terminal host such as Herdr need not implement aP to host Alan's terminal renderer.

#### Scenario: A UI invokes an app operation
- **WHEN** a user acts through an Alan App surface
- **THEN** the UI ultimately performs the same authorized file write, `ctl` write, or executable
  spawn available to another file client
- **AND** the authoritative result is readable from the service tree

### Requirement: Client integration waits for the direct file boundary
Alan OS clients SHALL integrate an app or host service
through its authoritative mounted aP tree and normal Process namespace. A
missing attachment, service tree, package mount, or binfs implementation SHALL
block the dependent client feature rather than authorize a temporary
client-facing bridge. New work SHALL select a surviving consumer; the retired
Alan for macOS client is not a prerequisite for delivering other clients.

#### Scenario: macOS client attachment is not implemented
- **WHEN** retained legacy macOS client maintenance needs service state but
  that consumer cannot open and watch the mounted service files
- **THEN** its integration remains blocked on direct file attachment
- **AND** this does not require restoring the retired App for other consumers
- **AND** no client-owned operation surface substitutes for the authorized aP tree

#### Scenario: Surviving client requires service state
- **WHEN** a new Alan App feature selects a supported Alan OS client
- **THEN** its prerequisites are that client's authorized attachment and service tree
- **AND** Alan for macOS implementation is not a delivery prerequisite

#### Scenario: Packaged command is not mounted
- **WHEN** a feature requires a package-provided command but the package store is
  not yet projected through the canonical package/binfs mount into `/bin`
- **THEN** command discovery and launch remain blocked on that mount
- **AND** the feature reports the missing capability rather than fabricating
  an executable binding outside the canonical package/binfs owner
