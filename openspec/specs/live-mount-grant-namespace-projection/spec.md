# live-mount-grant-namespace-projection Specification

## Purpose
Defines how approved mount grants update a running Agent Process namespace,
preserve host/Kernel/engine layering, remain idempotent, and record application
outcomes independently.
## Requirements
### Requirement: Approved mount grants update the live Agent Process namespace
Host Mount Service SHALL project an approved grant's mountable file-server
handle into the requesting Process live namespace at the requested absolute
`/mnt/<name>` path. Future aP walks SHALL resolve through that handle. The same
live namespace SHALL remain the source of truth for file walks,
`/proc/<pid>/namespace`, and snapshots used for later Process spawn.

#### Scenario: Approved read-write grant is mounted into the namespace
- **WHEN** Host Mount Service approves a logical `/mnt/project` request with
  `access = read_write`
- **THEN** future aP walks and mutations under `/mnt/project` use the approved
  grant handle and access rights
- **AND** the request status reports the successful projection

#### Scenario: Approved read-only grant is mounted into the namespace
- **WHEN** Host Mount Service approves a logical `/mnt/docs` request with
  `access = read_only`
- **THEN** future aP walks and reads under `/mnt/docs` use the approved grant
  handle
- **AND** aP mutations are rejected and native writable authority is not added

#### Scenario: Proc namespace views and child spawns observe live grants
- **WHEN** an approved grant changes the requesting Process live namespace
- **THEN** future `/proc/<pid>/namespace` reads include the mount
- **AND** a later child receives it only when the spawner explicitly delegates
  that grant handle in the child's namespace
- **AND** Host Mount Service does not report success while Process namespace
  state remains stale

#### Scenario: Live namespace mutation invalidates namespace metadata
- **WHEN** Host Mount Service mutates the live namespace
- **THEN** the live namespace generation and affected synthetic qids or versions
  change as one observable mutation
- **AND** cache-by-qid clients can detect that namespace metadata must be reread
- **AND** a Process launcher cannot pair the new mount table with the prior
  generation

### Requirement: Namespace projection preserves host/kernel/engine layering
The Host adapter SHALL construct the host-backed file-server export from the raw
Host OS path and return that opaque export to Host Mount Service. Host Mount
Service SHALL project only the service-issued handle, namespace path, and access
into the Process namespace. `alan-agent-engine` MUST NOT construct `HostDirFs`,
depend on `alan_hostfs`, receive the raw path, or invoke a live mount applicator;
Alan Kernel MUST NOT store Host path provenance.

#### Scenario: Host composition owns HostDirFs construction
- **WHEN** a pending request receives native authorization
- **THEN** the Host adapter constructs the host-backed export and Host Mount
  Service projects its handle
- **AND** Agent Execution Engine observes only service request status through aP
- **AND** Alan Kernel records only the mounted file server and access mode

#### Scenario: Host export construction fails
- **WHEN** the Host adapter cannot construct or authorize the requested export
- **THEN** Host Mount Service records `failed` with a concise error
- **AND** no namespace mount or approved grant is reported

### Requirement: Namespace projection is idempotent by namespace path
Host Mount Service SHALL replace the exact requested namespace mount path for
future walks instead of accumulating duplicate mounts. Rejected, cancelled, or
failed requests SHALL NOT change the live namespace.

#### Scenario: Repeated approved grant replaces the same namespace path
- **WHEN** the same Process receives a later approved grant for an existing
  exact namespace path
- **THEN** future walks resolve through one latest mounted grant handle
- **AND** namespace descriptions do not accumulate duplicate exact-path entries

#### Scenario: Non-approved request leaves the namespace unchanged
- **WHEN** a request reaches `rejected`, `cancelled`, or `failed`
- **THEN** its requested namespace path is not mounted

### Requirement: Namespace application outcome is audited independently
Host Mount Service request status and audit records SHALL report grant approval
and namespace projection without relying on an engine-owned grant event. Native
Tool sandbox derivation MAY have a separate outcome, but both projections SHALL
refer to the same service-owned grant and MUST NOT expose its raw Host backing.

#### Scenario: Namespace apply failure is reported without false approval
- **WHEN** native authorization succeeds but live namespace projection fails
- **THEN** Host Mount Service records the projection failure and does not expose
  an usable approved grant to the requesting Process
- **AND** the Agent-visible result contains only the logical request and concise
  error

#### Scenario: Read-only grant has no writable Tool projection
- **WHEN** an approved read-only grant is mounted into the Process namespace
- **THEN** Host Mount Service reports the namespace projection
- **AND** the Host adapter does not derive native write authority for Tool
  Processes
