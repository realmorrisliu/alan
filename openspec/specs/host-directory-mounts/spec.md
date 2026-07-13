# host-directory-mounts Specification

## Purpose
Defines confined host-directory aP file servers and the projection of declared
host mounts into Process namespaces and Tool sandbox specifications.
## Requirements
### Requirement: Host directories can be mounted as aP file trees
Alan OS SHALL provide a host-directory-backed aP file server that exposes a
declared host directory as an ordinary mountable file tree. The server SHALL keep
all file operations confined below the declared host root and SHALL return aP
errors rather than leaking host filesystem implementation details.

#### Scenario: Reading a mounted host file
- **WHEN** a host directory containing `notes/today.txt` is mounted at
  `/mnt/project`
- **THEN** an aP client can walk `/mnt/project/notes/today.txt`
- **AND** opening and reading that fid returns the host file bytes

#### Scenario: Directory listings expose child names
- **WHEN** an aP client reads a mounted host directory
- **THEN** the server returns the directory's child names in a stable textual
  listing

#### Scenario: Path traversal cannot escape the host root
- **WHEN** a path resolves through `..` segments or symlinks outside the declared
  host root
- **THEN** the host directory file server rejects the operation
- **AND** no host file outside the declared root is read, written, created, or
  removed

### Requirement: Host mount declarations project into the namespace
Alan OS SHALL assemble host directory mounts from human/config declarations
outside the agent's control. Each declaration SHALL include an aP namespace path,
a host path, and an access level. Applying the declaration SHALL mount a
`HostDirFs` at the namespace path with the declared `Access`.

#### Scenario: Writable host declaration becomes writable namespace mount
- **WHEN** a human/config declaration mounts host path `/host/project` at
  `/mnt/project` with write access
- **THEN** `/mnt/project` is reachable through the namespace
- **AND** aP writes through that mount are permitted subject to normal file-server
  path confinement

#### Scenario: Read-only host declaration masks mutation
- **WHEN** a human/config declaration mounts host path `/host/docs` at
  `/mnt/docs` with read-only access
- **THEN** `/mnt/docs` is reachable through the namespace for walk/read/stat
- **AND** write, create, and remove operations through that mount are rejected
  by namespace access enforcement

### Requirement: Host mount declarations project into SandboxSpec
Alan OS SHALL derive native subprocess sandbox roots from the exact Host Mount
grants present in the Process launch context. Writable grants SHALL contribute
writable Host roots; read-only grants SHALL contribute no write authority; and
virtual mounts SHALL NOT grant Host access. There is no ambient Host-directory seed.

#### Scenario: Writable Host Mount becomes sandbox writable root
- **WHEN** a Process receives one writable Host Mount at `/mnt/source`
- **THEN** the derived sandbox includes only that grant's Host backing as a
  writable root

#### Scenario: No Host Mount is present
- **WHEN** a Process has only virtual Alan OS mounts
- **THEN** no ambient Host cwd, home, or other implicit Host writable root is added

### Requirement: Host files are invisible by default
A Host directory SHALL enter a Process namespace only after explicit Host
authorization creates a Host Mount. Kernel and Agent runtime files MUST use its
Alan OS mount path and MUST NOT expose the raw Host path.

#### Scenario: Alan starts inside a Host directory
- **WHEN** the CLI is launched with that directory as Host cwd
- **THEN** the directory remains absent until the Host explicitly grants and
  mounts it
