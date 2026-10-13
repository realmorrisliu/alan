# host-mount-tool-process-sandbox-projection Specification

## Purpose
Defines how Host adapters derive per-Tool-Process native sandbox authority from
explicitly delegated service-owned Host Mount handles without exposing raw Host
backing or retaining engine-owned sandbox roots.
## Requirements
### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
The Host adapter SHALL derive native writable authority from the same
service-owned grant when alan9 starts a native Tool Process with an explicitly
delegated read-write Host Mount. Agent Execution Engine
MUST NOT store the raw Host backing, add native paths to an engine-owned sandbox
list, or apply sandbox authority from a grant ID alone.

#### Scenario: Approved read-write grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-write Host Mount
  handle mounted at `/mnt/project`
- **THEN** the Host adapter adds the grant's native backing to the OS sandbox
  with writable access
- **AND** Tool code continues to address the tree through the alan9 namespace

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
  and executes below its alan9 mount
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

### Requirement: Native shell guidance distinguishes namespace and shell operands
Model-visible native Bash Tool guidance SHALL distinguish Host-resolved Process
cwd and file-Tool namespace paths from operands embedded in unchanged shell
command text. It SHALL describe cwd-relative operands for mounted project files
and file Tools for absolute Alan namespace paths. A namespace-absolute shell
operand refusal MUST NOT be described as proof that the selected project lacks
authorization. Guidance MUST NOT expose private Host backing, add authority or
claim that arbitrary namespace paths are automatically rewritten for a shell.

#### Scenario: Model receives Bash guidance for a mounted project
- **WHEN** a real Agent receives the builtin Bash definition and command schema
- **THEN** both explain the supported cwd-relative shell path behavior
- **AND** the guidance distinguishes a namespace operand limitation from missing project authority

#### Scenario: Qualification follows a guidance correction
- **WHEN** Tool guidance changes after a failed task
- **THEN** the original failed attempt remains retained
- **AND** a fresh candidate runs the unchanged frozen task before claiming a qualified completion or resolved model-usage finding
