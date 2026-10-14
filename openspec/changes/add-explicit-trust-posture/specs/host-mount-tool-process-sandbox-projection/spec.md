# Spec Delta

## MODIFIED Requirements

### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
These native projection guarantees SHALL apply to confined execution. Explicit fully trusted Host-account native execution uses separately selected account authority, not a fabricated mount-derived grant. Read-only aP handles, FileServer access checks and service grant identity SHALL remain unchanged; native account rights do not widen those handles.
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
These native projection guarantees SHALL apply to confined execution. Explicit fully trusted Host-account native execution uses separately selected account authority, not a fabricated mount-derived grant. Read-only aP handles, FileServer access checks and service grant identity SHALL remain unchanged; native account rights do not widen those handles.
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
These native projection guarantees SHALL apply to confined execution. Explicit fully trusted Host-account native execution uses separately selected account authority, not a fabricated mount-derived grant. Read-only aP handles, FileServer access checks and service grant identity SHALL remain unchanged; native account rights do not widen those handles.
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


### Requirement: Native shell reads explicitly delegated read-only dependencies
These native projection guarantees SHALL apply to confined execution. Explicit fully trusted Host-account native execution uses separately selected account authority, not a fabricated mount-derived grant. Read-only aP handles, FileServer access checks and service grant identity SHALL remain unchanged; native account rights do not widen those handles.
For supported enforcing native execution, the Host adapter SHALL retain the cwd
selected grant first and SHALL additionally project live read-only Host Mounts
explicitly held by the same Process. Only the selected grant SHALL contribute
native writable authority. Other writable grants and grants held only by another
Process MUST NOT gain native access through this projection. The Host adapter
SHALL preserve each grant's effective access, current reconciliation and native
path containment without command rewriting or inferred common-parent authority.
This is a proposed extension to ADR-0058's delivered single-grant slice; adoption
requires review and user merge of this delivery.

#### Scenario: A project builds against its separately granted read-only dependency
- **WHEN** the Process holds the selected project and a live separate read-only dependency grant
- **THEN** the enforcing native build can read that dependency through supported relative or absolute native manifest paths and complete its tests
- **AND** native writes to the dependency, reads/writes in another writable project and projection of another Process's grant remain denied

#### Scenario: An external dependency grant is revoked after successful compilation
- **WHEN** the Process revokes its read-only dependency grant and requests the next build with retained successful output
- **THEN** launch reconciles current authority and cannot read the revoked dependency
- **AND** the cached earlier success does not become a new successful execution

#### Scenario: A read-only dependency source escapes through a symlink
- **WHEN** a live read-only dependency resolves a source outside its granted backing
- **THEN** structured access rejects the escaped path and enforcing native compilation fails without consuming the outside source
- **AND** no extra root or writable authority is inferred from the dependency manifest or symlink

#### Scenario: A stale or unknown cwd cannot select another writable grant
- **WHEN** a non-root requested cwd is outside every live same-Process Host Mount
- **THEN** Host reconciliation refuses and requires an explicit directory selection
- **AND** an unrelated live writable grant is not silently substituted for that cwd

#### Scenario: An approved backing root is retargeted or unavailable
- **WHEN** a live grant's approved canonical backing root is missing, is no longer a directory, resolves to a different root or names a replacement directory instead of the retained file-tree root before Tool adapter construction
- **THEN** the native Host adapter refuses projection before deriving sandbox authority
- **AND** the failure identifies the public namespace without exposing private backing paths or granting the new target
