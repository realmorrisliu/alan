## MODIFIED Requirements

### Requirement: Kernel-enforced confinement independent of command syntax
When an OS sandbox backend is active, the operating system SHALL enforce
filesystem writes to the writable roots of the active `SandboxSpec` and control
network access regardless of how a command is written. The workspace is the seed
writable root, and additional human/config-declared writable host directory
mounts SHALL also be writable roots. Read-only host directory mounts SHALL NOT
grant native-subprocess write authority.

#### Scenario: Internal-write command is confined
- **WHEN** a command writes outside the active `SandboxSpec` writable roots
  through program-internal logic without an explicit path operand
- **THEN** the OS sandbox blocks the out-of-writable-roots write
- **AND** confinement does not depend on parsing the command string

#### Scenario: Network is controlled by the sandbox
- **WHEN** a confined command attempts network access that the active policy does
  not permit
- **THEN** the OS sandbox prevents it

#### Scenario: Declared writable host mount permits native writes there
- **WHEN** a host directory is declared with write access and projected into the
  active `SandboxSpec`
- **THEN** a native subprocess confined by the OS sandbox can write below that
  declared host path
- **AND** writes outside all writable roots remain blocked

#### Scenario: Declared read-only host mount does not permit native writes
- **WHEN** a host directory is declared read-only
- **THEN** that host path is not included in the active `SandboxSpec` writable
  roots
- **AND** a native subprocess is not granted write authority there by the
  sandbox projection
