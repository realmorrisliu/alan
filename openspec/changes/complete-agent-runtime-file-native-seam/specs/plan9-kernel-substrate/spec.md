## ADDED Requirements

### Requirement: The current Process namespace is file-addressable
A Process-scoped `/proc` view SHALL expose `/proc/self/namespace` as a
point-in-time description of the current Process namespace authority used for
subsequent clone operations. The file SHALL reflect live namespace mutations
visible at read time. A bootstrap `/proc` view without a current Process SHALL
NOT expose `/proc/self`.

#### Scenario: Alan Shell snapshots its current namespace
- **WHEN** Alan Shell reads `/proc/self/namespace` immediately before spawning a
  command
- **THEN** it receives the mount paths and access rights currently delegable by
  that Shell Process
- **AND** the snapshot requires no Kernel side API or Host Process identity

#### Scenario: Bootstrap has no current Process alias
- **WHEN** a bootstrap component uses a `/proc` view before the first Process
  exists
- **THEN** `/proc/self` is absent
- **AND** bootstrap uses its explicit launch context rather than inventing a
  Process identity
