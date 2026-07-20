## MODIFIED Requirements

### Requirement: Process launch has no workspace identity
Alan OS SHALL create every Process from its parent namespace snapshot, explicit
mounts and descriptors, credentials, and normalized initial namespace cwd. It
MUST NOT assign a workspace ID or Host root identity, carry raw Host Mount
backing records, or use a grant ID as launch authority.

#### Scenario: Agent starts with a Host Mount
- **WHEN** a Shell Process spawns an Agent Executable with a Host Mount handle
  explicitly projected at `/mnt/source` and cwd set to `/mnt/source`
- **THEN** the child receives that namespace mount without a workspace field or
  raw Host OS path
- **AND** native sandbox authority is derived from the same delegated grant by
  the Host adapter

#### Scenario: Child launch requests a non-normal namespace cwd
- **WHEN** a child launch requests a cwd containing `.` or `..` path components
- **THEN** the Process launch is rejected before storing the cwd
- **AND** Tool Process binding cannot fall back to a different Host Mount

### Requirement: Child context follows namespace inheritance
A child Process SHALL start from the namespace capabilities its spawner can
delegate and SHALL gain additional authority only through explicitly passed
mounts or descriptors. A Process launch MUST NOT contain an aggregate
all-Host-Mounts handle, and Host Mount inheritance SHALL default to none unless
each selected grant handle and target namespace path is listed explicitly.

#### Scenario: Child lacks an unpassed mount
- **WHEN** a parent launches a child without listing a parent Host Mount
- **THEN** the child cannot reach that Host Mount by grant ID, Host-path
  inference, cwd inference, or ambient discovery

#### Scenario: Child requests amplified access
- **WHEN** a parent attempts to pass a Host Mount handle or access mode it cannot
  itself delegate
- **THEN** Process creation rejects the launch
- **AND** the child never receives the amplified mount
