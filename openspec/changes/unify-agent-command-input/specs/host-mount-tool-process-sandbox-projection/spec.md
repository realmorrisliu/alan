## MODIFIED Requirements

### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
The Host adapter SHALL derive native writable authority from the same
service-owned grant when alan9 starts a native Tool Process with an explicitly
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
Host adapter checks and OS sandbox profiles for native shell actions SHALL
include only the native backing of the Host Mount grant
referenced by the Process shared cwd and SHALL reject paths outside that grant's
effective authority. Other delegated mounts remain available to structured Agent
file operations, but SHALL NOT appear as native path aliases in this shell action.
A later action can select another already-delegated grant only after standalone
`!cd /mnt/<grant>` updates the shared cwd. A single shell action SHALL NOT be
required to span disjoint grants.
Native sandbox-root construction SHALL remain Host-adapter implementation data.
Raw native execution cwd/paths SHALL remain private to the Host adapter's launch
and sandbox context. Alan-captured output, AgentFS path metadata and correlated
durable evidence SHALL use public grant-relative project paths or opaque
references under existing redaction and retention rules. Paths in stdout/stderr
captured by Alan SHALL be projected relative to that submission's shared cwd (`.`
for the cwd); they SHALL NOT expose the raw Host root or use `/mnt` aliases as a
replacement. This projection does not intercept native shell redirection or
rewrite command-created files. Files written within the active delegated grant
remain ordinary project data and may contain native path strings. The redirection
itself does not copy those contents into Alan-generated command output or
evidence; a later file read is ordinary project data under the same grant, and
path strings confer no authority. Such references SHALL NOT replace
capability-passed grants, reveal unrelated backing, or authorize reconstruction
of sandbox policy. Both explicit user and Agent shell actions SHALL use this same
boundary; `!` SHALL NOT imply unrestricted execution.

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
- **AND** launch and durable records retain only logical cwd/grant references,
  while native cwd and sandbox construction remain adapter-private

#### Scenario: Bash preserves cwd across multiple delegated mounts
- **WHEN** a Tool Process cwd is below one of multiple explicitly delegated Host
  Mounts
- **THEN** Host authority reconciliation resolves the cwd through that covering
  mount independent of grant iteration order
- **AND** the native shell sandbox contains only that selected grant
- **AND** every other delegated mount remains available to structured Agent file
  operations with its effective access

#### Scenario: Native shell switches to another delegated grant
- **WHEN** a Process with multiple delegated Host Mounts accepts standalone
  `!cd /mnt/fixtures`
- **THEN** Host authority reconciliation selects that grant as the shared cwd
- **AND** the next native shell action receives only that grant's native backing
- **AND** the Process identity and Host backing file remain unchanged

#### Scenario: One native shell action cannot span disjoint grants
- **WHEN** a shell action runs under one grant and names a path in another
  delegated grant
- **THEN** the inactive grant is not exposed as a native path in that action
- **AND** structured Agent file operations may address it through their own
  delegated Host Mount checks

#### Scenario: Opaque mount-local executables are rejected when reads are not kernel-confined
- **WHEN** the active OS sandbox backend does not confine reads and a command's
  executable resolves from the active Host Mount
- **THEN** native Tool execution rejects the command before launch because the
  executable could read paths that command-text validation cannot inspect

#### Scenario: ProtectedOnly rejects opaque project-code dispatch
- **WHEN** a read-unconfined backend uses ProtectedOnly checks and a Git alias,
  package script, or project build/test/run/generation command dispatches code
- **THEN** native Tool execution rejects the opaque dispatcher before launch
- **AND** dispatched project code cannot read outside the active Host Mount
  through a path hidden from the submitted command text

#### Scenario: ProtectedOnly encounters an unknown executable
- **WHEN** a command names an executable outside the backend's inspected command set
- **THEN** execution fails closed before launch, including through command wrappers
- **AND** an unknown runner does not become permitted merely because it is absent
  from the known project-dispatcher list
- **AND** this bounded command restriction does not apply to a backend with kernel read confinement

#### Scenario: Native output paths remain shell-usable and private
- **WHEN** native stdout or stderr contains a path under the active cwd grant
- **THEN** the Host adapter projects it relative to that submission's shared cwd
- **AND** it does not expose the raw Host backing root or replace the path with an
  `/mnt` alias
- **AND** the same projected path is used in user output and Agent evidence

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
Project read, edit and search tools SHALL accept public grant-relative paths for
any delegated Host Mount and shared-cwd-relative paths for the active shell grant.
The Host adapter SHALL resolve structured path arguments against explicitly
delegated Host Mounts to existing file operations. Agent edits and native
commands SHALL address the same backing files without a shadow project copy,
protocol knowledge or command-string rewriting; to use the same file through the
native shell, the shared cwd SHALL reference that file's grant. This SHALL NOT
materialize virtual services as Host files or alter existing authorization,
stale-content checks and save/commit semantics.

#### Scenario: Committed Agent edit is inspected by a native command
- **WHEN** an Agent edit to a file in the active shell grant reports committed success
- **THEN** a subsequent native read or git diff observes the modified backing file
- **AND** the Agent tool and native command use consistent public project paths

#### Scenario: Native edit is read by the Agent
- **WHEN** a native command or external editor saves a file in the active shell grant
- **THEN** a subsequent Agent file read observes that backing file's current contents
- **AND** a stale expected-content edit fails rather than silently overwriting concurrent work

#### Scenario: A noncurrent grant is selected before shell inspection
- **WHEN** an Agent file tool edits a file in another delegated grant and the user
  accepts standalone `!cd /mnt/fixtures`
- **THEN** a later native shell read observes that same backing file
- **AND** no single native shell action needs access to both grants

#### Scenario: An edit is pending or cannot be saved
- **WHEN** a buffer has uncommitted changes or the backing-file save fails
- **THEN** the tool reports pending or failed state rather than committed project success
- **AND** no guarantee of atomicity across concurrent tools is fabricated

#### Scenario: Structured path exceeds delegated authority
- **WHEN** a project tool receives a path outside its grant, a disallowed symlink target or a revoked grant
- **THEN** it rejects unauthorized access through the existing Host adapter checks
- **AND** native command rights remain governed by the same delegated grants and sandbox policy
