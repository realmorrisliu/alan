## MODIFIED Requirements

### Requirement: Process launch has no workspace identity
Alan OS SHALL create every Process from a spawner-authorized namespace manifest
that selects only the mounts and descriptors permitted for that launch, plus
credentials and a normalized initial namespace cwd. It MUST NOT begin from the
full parent live namespace, assign a workspace ID or Host root identity, carry
raw Host Mount backing records, or use a grant ID as launch authority.

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

### Requirement: Child context follows explicit namespace delegation
A child Process SHALL start from the namespace capabilities its spawner can
delegate and SHALL gain additional authority only through explicitly passed
mounts or descriptors. A Process launch MUST NOT contain an aggregate
all-Host-Mounts handle, and Host Mount inheritance SHALL default to none unless
each selected grant handle and target namespace path is listed explicitly.
Every `/proc/clone` exec document MUST contain an explicit namespace manifest;
Alan Kernel SHALL reject a missing manifest instead of inheriting the pending
namespace implicitly.

#### Scenario: Child lacks an unpassed mount
- **WHEN** a parent launches a child without listing a parent Host Mount
- **THEN** the child cannot reach that Host Mount by grant ID, Host-path
  inference, cwd inference, or ambient discovery

#### Scenario: Child requests amplified access
- **WHEN** a parent attempts to pass a Host Mount handle or access mode it cannot
  itself delegate
- **THEN** Process creation rejects the launch
- **AND** the child never receives the amplified mount

#### Scenario: Tool Process receives an explicit namespace snapshot
- **WHEN** an Agent Process starts a Tool Process through `/proc/clone`
- **THEN** the exec document lists the namespace mounts delegated at that launch
- **AND** a mount added to the parent's live namespace after the snapshot is not
  inherited by the Tool Process

#### Scenario: Alan Shell launches an ordinary Process
- **WHEN** Alan Shell launches a command through `/proc/clone`
- **THEN** it reads `/proc/self/namespace` and writes those delegated mounts into
  the exec document
- **AND** an unavailable or malformed current-namespace file fails the launch
  without falling back to ambient inheritance

#### Scenario: Exec document omits its namespace manifest
- **WHEN** a spawner commits an exec document without a namespace manifest
- **THEN** Alan Kernel rejects the commit and discards the pending Process slot
- **AND** no child inherits the spawner's namespace implicitly
