## MODIFIED Requirements

### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
The Host adapter SHALL derive native writable authority from the same
service-owned grant when Alan OS starts a native Tool Process with an explicitly
delegated read-write Host Mount. Agent Execution Engine
MUST NOT own a native backing registry, add native paths to an engine-owned
sandbox list, or apply sandbox authority from a grant ID or disclosed path alone.
The Host adapter MAY use ephemeral native cwd/path metadata as spawn attributes
outside the Alan Process exec manifest; it SHALL NOT serialize raw backing paths
into AgentFS, Machine state, Agent-visible results or durable evidence. Host Mount
Service and its adapter remain the authority for each launch.

#### Scenario: Approved read-write grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-write Host Mount
  handle mounted at `/mnt/project`
- **THEN** the Host adapter adds the grant's native backing to the OS sandbox
  with writable access
- **AND** aP clients address the namespace tree and native shell programs use the
  adapter-supplied Host cwd/paths without rewriting command text

#### Scenario: Duplicate grant delegation is idempotent
- **WHEN** the same read-write grant handle is included more than once in one
  Tool Process launch
- **THEN** the Host adapter emits one effective native sandbox authorization

### Requirement: Host adapters enforce delegated Tool Process sandbox authority
Host adapter containment checks and OS sandbox profile generation SHALL include
the native backing of every explicitly delegated read-write Host Mount grant
and SHALL reject paths outside the Tool Process's delegated writable authority.
Native sandbox-root construction SHALL remain Host-adapter implementation data.
Raw native execution cwd/paths SHALL remain private to the Host adapter's launch
and sandbox context. Agent context, AgentFS and correlated durable evidence SHALL
use public project paths or opaque grant references under existing redaction and
retention rules. Such references SHALL NOT replace capability-passed grants,
reveal unrelated backing, or authorize reconstruction of sandbox policy. Both
explicit user and Agent shell actions SHALL use this same boundary; `!` SHALL NOT
imply unrestricted execution.

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
- **AND** launch and durable records retain only logical cwd/grant references,
  while native cwd and sandbox construction remain adapter-private

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

#### Scenario: Command mentions an undelegated native path
- **WHEN** command text contains a Host path not authorized by its grants and execution policy
- **THEN** mentioning that path does not add sandbox rights
- **AND** existing sandbox enforcement and safe-degradation rules apply

#### Scenario: Grant is revoked before command launch
- **WHEN** a queued command's Host Mount has been revoked
- **THEN** current launch authority is revalidated and unavailable access fails
- **AND** cached cwd/path metadata cannot restore that grant

## ADDED Requirements

### Requirement: Project tools and native commands share public paths and backing files
Project read, edit and search tools SHALL accept public grant-relative or
shared-cwd-relative project paths consistent with native commands. The Host adapter
SHALL resolve structured path arguments against explicitly delegated Host Mounts
to existing file operations. Agent edits and native commands SHALL address the
same backing files without a shadow project copy, protocol knowledge or
command-string rewriting. This SHALL NOT materialize virtual services as Host
files or alter existing authorization, stale-content checks and save/commit
semantics.

#### Scenario: Committed Agent edit is inspected by a native command
- **WHEN** an Agent edit to an authorized project file reports committed success
- **THEN** a subsequent native read or git diff observes the modified backing file
- **AND** the Agent tool and native command use consistent public project paths

#### Scenario: Native edit is read by the Agent
- **WHEN** a native command or external editor saves a project file
- **THEN** a subsequent Agent file read observes that backing file's current contents
- **AND** a stale expected-content edit fails rather than silently overwriting concurrent work

#### Scenario: An edit is pending or cannot be saved
- **WHEN** a buffer has uncommitted changes or the backing-file save fails
- **THEN** the tool reports pending or failed state rather than committed project success
- **AND** no guarantee of atomicity across concurrent tools is fabricated

#### Scenario: Structured path exceeds delegated authority
- **WHEN** a project tool receives a path outside its grant, a disallowed symlink target or a revoked grant
- **THEN** it rejects unauthorized access through the existing Host adapter checks
- **AND** native command rights remain governed by the same delegated grants and sandbox policy
