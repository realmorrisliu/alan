## MODIFIED Requirements

### Requirement: Confinement input is a projected SandboxSpec
Alan SHALL derive a `SandboxSpec` from the concrete Process namespace, descriptors, credentials,
network policy, executable, and delegated mounts. The spec SHALL be attributable to that Process and
SHALL contain the complete inputs needed by the selected OS sandbox backend.

#### Scenario: A Tool Process receives workspace-only confinement
- **WHEN** a Tool Process is spawned with only a workspace mount and no network authority
- **THEN** the selected OS sandbox backend receives a matching `SandboxSpec`
- **AND** the Process namespace, descriptors, credentials, and policy determine its confinement
