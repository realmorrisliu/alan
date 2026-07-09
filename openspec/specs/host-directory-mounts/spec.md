# host-directory-mounts Specification

## Purpose
TBD - created by archiving change add-host-dir-file-server. Update Purpose after archive.
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
Alan OS SHALL use the same host mount declaration list to derive the OS sandbox
manifest for native subprocesses. The workspace SHALL be the seed host mount.
Writable host declarations SHALL contribute writable roots; read-only host
declarations and virtual mounts SHALL NOT grant native-subprocess write
authority.

#### Scenario: Writable host declaration becomes sandbox writable root
- **WHEN** the declaration list contains the workspace seed and a writable host
  mount at `/host/project`
- **THEN** the derived `SandboxSpec.writable_roots` contains both the workspace
  host path and `/host/project`

#### Scenario: Read-only host declaration does not grant native writes
- **WHEN** the declaration list contains a read-only host mount at `/host/docs`
- **THEN** the derived `SandboxSpec.writable_roots` does not contain
  `/host/docs`

#### Scenario: Virtual mounts do not affect the sandbox
- **WHEN** the namespace also contains virtual mounts such as `/agent` or
  `/mnt/llm`
- **THEN** those virtual mounts do not add host paths to `SandboxSpec`

### Requirement: Mount authority is not agent-expandable at landing
Alan OS SHALL NOT expose an agent tool that can add host directory mounts in this
landing change. Host directory mounts SHALL be declared by human/config
composition before the agent runs, and the active declaration list SHALL remain
fixed for the session absent human action.

#### Scenario: No agent-visible mount command exists
- **WHEN** an agent inspects its tool namespace at landing
- **THEN** it does not receive a tool that can mount arbitrary host paths
- **AND** it cannot broaden its own host filesystem authority without an
  external human/config action

