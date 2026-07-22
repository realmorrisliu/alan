# host-mount-tool-process-sandbox-projection Specification

## Purpose
Defines how Host adapters derive per-Tool-Process native sandbox authority from
explicitly delegated service-owned Host Mount handles without exposing raw Host
backing or retaining engine-owned sandbox roots.
## Requirements
### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
The Host adapter SHALL derive native writable authority from the same
service-owned grant when Alan OS starts a native Tool Process with an explicitly
delegated read-write Host Mount. Agent Execution Engine
MUST NOT store the raw Host backing, add native paths to an engine-owned sandbox
list, or apply sandbox authority from a grant ID alone.

#### Scenario: Approved read-write grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-write Host Mount
  handle mounted at `/mnt/project`
- **THEN** the Host adapter adds the grant's native backing to the OS sandbox
  with writable access
- **AND** Tool code continues to address the tree through the Alan OS namespace

#### Scenario: Duplicate grant delegation is idempotent
- **WHEN** the same read-write grant handle is included more than once in one
  Tool Process launch
- **THEN** the Host adapter emits one effective native sandbox authorization

### Requirement: Read-only Host Mounts do not grant native write authority
The Host adapter SHALL NOT derive native writable authority from a read-only
Host Mount grant. The approved grant MAY remain readable through its mounted
file-server handle according to the Tool Process namespace and Host sandbox
policy.

#### Scenario: Approved read-only grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-only Host Mount
  handle
- **THEN** the Host adapter does not add its native backing as a writable root
- **AND** no engine-owned sandbox state is mutated

### Requirement: Host adapters enforce delegated Tool Process sandbox authority
Host adapter containment checks and OS sandbox profile generation SHALL include
the native backing of every explicitly delegated read-write Host Mount grant
and SHALL reject paths outside the Tool Process's delegated writable authority.
Those native roots SHALL remain Host-adapter implementation data and MUST NOT be
serialized into Agent Machine, AgentFS, rollout/checkpoint, or Tool result
records.

#### Scenario: Tool can access an approved writable grant
- **WHEN** the Tool Process has an explicitly delegated read-write Host Mount
  and executes below its Alan OS mount
- **THEN** Host containment checks and the OS sandbox permit the corresponding
  native access
- **AND** a path outside every delegated writable grant remains rejected

#### Scenario: Bash uses an approved writable grant as cwd
- **WHEN** bash starts with cwd below an explicitly delegated read-write Host
  Mount
- **THEN** the Host adapter resolves the grant internally and permits the native
  cwd
- **AND** bash launch records do not expose the raw Host path to Agent Execution
  Engine

#### Scenario: Bash preserves cwd across multiple delegated mounts
- **WHEN** a Tool Process cwd is below one of multiple explicitly delegated Host
  Mounts
- **THEN** Host authority reconciliation resolves the cwd through that covering
  mount independent of grant iteration order
- **AND** every other delegated mount remains available with its effective access

#### Scenario: Process requests overlapping Host Mount projections
- **WHEN** a Process already holds an active Host Mount projection and another
  grant would mount at a strict parent or child namespace path
- **THEN** Host Mount Service rejects the overlapping projection before native
  Tool sandbox authority is constructed
- **AND** the existing projection remains unchanged
