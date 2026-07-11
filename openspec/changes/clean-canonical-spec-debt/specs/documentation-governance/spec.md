## ADDED Requirements

### Requirement: Canonical and active OpenSpec surfaces are complete and current
Alan SHALL keep canonical capability metadata, repository OpenSpec
configuration, and active change references complete and valid for the current
repository and installed OpenSpec schema. Immutable archived changes SHALL be
excluded from current-surface rewrite requirements.

#### Scenario: Canonical capability is inspected
- **WHEN** a specification under `openspec/specs/` is validated
- **THEN** its Purpose describes the capability's current ownership and scope
- **AND** it does not contain a generated placeholder or archive reminder

#### Scenario: OpenSpec artifact instructions are loaded
- **WHEN** the repository asks OpenSpec for proposal, design, specs, or tasks
  instructions
- **THEN** every configured artifact-rule key is supported by the active schema
- **AND** instruction lookup emits no unknown-artifact warning

#### Scenario: Active change cites implementation scope
- **WHEN** a non-archived change names a source path, baseline, or implementation
  owner
- **THEN** the referenced surface exists or is explicitly introduced by that
  change
- **AND** deleted Console and retired remote-control surfaces are not counted as
  current scope

### Requirement: Active planning does not authorize temporary architecture bridges
Alan SHALL reject active OpenSpec work that authorizes a temporary callback,
DTO, ContentInstance, host-action, host-compatibility, or namespace-bootstrap
bridge in place of an accepted aP, file-tree, package, or binfs boundary.

#### Scenario: Dependent feature lacks its native boundary
- **WHEN** an active change depends on host attachment, a service file tree, or
  a mounted package command that does not yet exist
- **THEN** the active change records that foundation as an entry criterion or
  dependency
- **AND** it does not schedule a temporary bridge followed by a deletion task

#### Scenario: Current-surface validation finds bridge authorization
- **WHEN** a canonical spec or active change permits a named compatibility
  bridge or an equivalent temporary authority path
- **THEN** repository verification fails with the owning file and matched rule
- **AND** immutable archived change history remains outside the failure scope
