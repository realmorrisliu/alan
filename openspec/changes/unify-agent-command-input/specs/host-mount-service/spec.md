## MODIFIED Requirements

### Requirement: Host Mount Service is the grant authority
Host Mount Service SHALL own logical request records, native authorization
coordination, user decisions, hostfs exports, grants, namespace projection,
revocation, audit, and status. The Host adapter SHALL be the only component that
selects and resolves native Host backing. Logical service records SHALL expose
request and grant identity, label, access, provenance, status and `/mnt` path,
without raw Host paths. For an explicitly delegated local grant, the adapter MAY
use scoped native cwd/path metadata only as ephemeral Host-adapter spawn and
sandbox inputs, outside the Alan Process exec manifest. Alan-generated Host
Mount and execution path metadata, including AgentFS/Machine path fields and
service results, SHALL retain public project paths or opaque grant references,
not raw backing-path values. Native paths in command output captured by Alan and
persisted as execution-path evidence SHALL likewise be projected. This does not
rewrite arbitrary contents in an explicitly delegated Host file:
native shell redirection may store a native path there, and that file remains
ordinary project data which may be read through the same grant. Such content
does not become Host Mount metadata or confer authority. This SHALL NOT expose
undelegated grants or private virtual-service backing. Engine and Kernel SHALL
NOT reconstruct sandbox roots from this metadata.

#### Scenario: User approves a writable directory
- **WHEN** a Host adapter authorizes a native directory and returns a writable
  hostfs export for a pending logical request
- **THEN** Host Mount Service records the grant and mounts its handle into the
  requesting Process live namespace at the approved Alan OS path
- **AND** logical Host Mount request, grant, result and service audit records omit
  native backing; only the Host adapter's launch context may carry the scoped native path

#### Scenario: User dismisses native directory authorization
- **WHEN** a Host adapter presents a pending request and the user dismisses the
  native directory authorization panel
- **THEN** the adapter asks Host Mount Service to publish a terminal `cancelled`
  result through the same-user Host command plane
- **AND** Agent Runtime resumes the waiting Agent Process without a grant

#### Scenario: AgentFS receives an approval write
- **WHEN** an Agent Process or renderer writes an approve-like value to an
  AgentFS request or Machine control file
- **THEN** no Host Mount grant is created unless Host Mount Service receives a
  Host-adapter authorization
- **AND** AgentFS cannot act as a second grant authority

#### Scenario: Agent prepares a command for an authorized project
- **WHEN** the calling Process has an explicitly delegated local grant
- **THEN** the Agent supplies its public project path or grant-relative cwd
- **AND** the Host adapter resolves it to a native execution path only at launch
- **AND** AgentFS and correlated evidence do not receive the raw backing path
- **AND** knowing or copying the public path does not authorize another Process
- **AND** unrelated Host Store and virtual service backing paths remain private
