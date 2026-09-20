# Spec Delta

## MODIFIED Requirements

### Requirement: Client integration waits for the direct file boundary
Alan OS clients SHALL integrate an app or host service through its authoritative
mounted aP tree and normal Process namespace. A missing attachment, service
tree, package mount, or binfs implementation SHALL block the dependent client
feature rather than authorize a temporary client-facing bridge. Retired Alan
for macOS delivery is not a prerequisite for delivering other clients.

#### Scenario: Retained Apple source is not a delivery consumer
- **WHEN** maintenance work inspects the retained Apple source or shell-core
  crates
- **THEN** it does not treat the retired app bundle as a required client for
  Alan OS service delivery
- **AND** any surviving platform capability uses its authoritative file or
  adapter boundary

#### Scenario: macOS client attachment is not implemented
- **WHEN** retained legacy macOS source maintenance needs service state but
  that source cannot open and watch the mounted service files
- **THEN** its integration remains blocked on direct file attachment
- **AND** this does not authorize restoring the retired app delivery path
- **AND** no client-owned operation surface substitutes for the authorized aP
  tree

#### Scenario: Surviving client requires service state
- **WHEN** a new Alan App feature selects a supported Alan OS client
- **THEN** its prerequisites are that client's authorized attachment and service
  tree
- **AND** Alan for macOS packaging is not a delivery prerequisite

#### Scenario: Packaged command is not mounted
- **WHEN** a feature requires a package-provided command but the package store
  is not yet projected through the canonical package/binfs mount into `/bin`
- **THEN** command discovery and launch remain blocked on that mount
- **AND** the feature reports the missing capability rather than fabricating an
  executable binding outside the canonical package/binfs owner
