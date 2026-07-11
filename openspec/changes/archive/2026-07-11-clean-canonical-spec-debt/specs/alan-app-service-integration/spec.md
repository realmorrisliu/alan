## ADDED Requirements

### Requirement: Client integration waits for the direct file boundary
Alan for macOS and other Alan OS clients SHALL integrate an app or host service
through its authoritative mounted aP tree and normal Process namespace. A
missing attachment, service tree, package mount, or binfs implementation SHALL
block the dependent client feature rather than authorize a temporary
client-facing bridge.

#### Scenario: macOS client attachment is not implemented
- **WHEN** an Alan App change requires Alan for macOS to render or mutate service
  state but the host cannot yet open and watch the mounted service files
- **THEN** the client-integration task remains blocked on direct file attachment
- **AND** the change does not introduce a callback, DTO, ContentInstance, or
  host-action bridge as an interim authority

#### Scenario: Packaged command is not mounted
- **WHEN** a feature requires a package-provided command but the package store is
  not yet projected through the canonical package/binfs mount into `/bin`
- **THEN** command discovery and launch remain blocked on that mount
- **AND** the feature does not synthesize a namespace-bootstrap compatibility
  projection
