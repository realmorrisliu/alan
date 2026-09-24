## MODIFIED Requirements

### Requirement: Host Mount Service is the grant authority
Host Mount Service SHALL own logical request records, native authorization
coordination, user decisions, hostfs exports, grants, namespace projection,
revocation, audit, and status. The Host adapter SHALL be the only component that
selects and resolves native Host backing. Logical service records SHALL expose
request and grant identity, label, access, provenance, status and `/mnt` path,
without raw Host paths. For an explicitly delegated local grant, the adapter MAY
supply scoped native cwd/path metadata to its authorized command context, Agent
and correlated evidence. This exception SHALL NOT expose undelegated grants or
private virtual-service backing and SHALL NOT turn path strings into authority.
Engine and Kernel SHALL NOT reconstruct sandbox roots from this metadata.

#### Scenario: User approves a writable directory
- **WHEN** a Host adapter authorizes a native directory and returns a writable
  hostfs export for a pending logical request
- **THEN** Host Mount Service records the grant and mounts its handle into the
  requesting Process live namespace at the approved Alan OS path
- **AND** logical Host Mount request, grant, result and service audit records omit
  native backing; authorized execution context may carry the scoped native path

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

#### Scenario: Agent prepares a native command
- **WHEN** the calling Process has an explicitly delegated local grant
- **THEN** the Host adapter may expose its usable native execution cwd/path to that Process
- **AND** knowing or copying that path does not authorize another Process
- **AND** unrelated Host Store and virtual service backing paths remain private
