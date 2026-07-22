## MODIFIED Requirements

### Requirement: Host Mount Service is the grant authority
Host Mount Service SHALL own logical request records, native authorization
coordination, user decisions, hostfs exports, grants, namespace projection,
revocation, audit, and status. The Host adapter SHALL be the only component that
selects or observes the raw Host OS path. Alan OS-visible records SHALL expose
request and grant identity, label, access, provenance, status, and `/mnt` path
but MUST NOT expose the raw Host OS path.

#### Scenario: User approves a writable directory
- **WHEN** a Host adapter authorizes a native directory and returns a writable
  hostfs export for a pending logical request
- **THEN** Host Mount Service records the grant and mounts its handle into the
  requesting Process live namespace at the approved Alan OS path
- **AND** no Alan OS-visible request, result, Machine, or audit record contains
  the native directory path

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

### Requirement: Grant authority is capability-passed
Knowing a grant ID SHALL confer no access. A Process SHALL gain access only when
the service-issued file-server handle or mount is explicitly projected into its
namespace, and native sandbox rights SHALL derive from that same grant. Host
Mount Service MUST reject delegation that the spawner cannot authorize from its
own namespace and access rights.

#### Scenario: Child does not receive parent grant
- **WHEN** a parent launches a child without explicitly passing a Host Mount
  handle and target namespace path
- **THEN** the child cannot use the grant ID, infer the Host backing, or discover
  the grant through its cwd

#### Scenario: Child receives selected grant
- **WHEN** a parent explicitly delegates a Host Mount handle it already holds
  with equal or narrower access
- **THEN** the child may reach the mounted tree at the specified Alan OS path
- **AND** no other parent Host Mount is inherited

## ADDED Requirements

### Requirement: Host Mount requests use a clone-based logical file protocol
Host Mount Service SHALL expose logical requests through
`/mnt/host-mount/requests/clone` using clone-via-open and commit-on-clunk. A
committed request SHALL contain a normalized `/mnt/<name>` namespace path,
access, non-empty reason, and optional label, and MUST NOT contain a raw Host OS
path. The service SHALL expose each committed request at
`requests/<id>/{request,status,grant,error}`, request updates at
`requests/events`, grant trees at `grants/<id>`, and service updates at
`events`.

#### Scenario: Agent commits a valid request
- **WHEN** an Agent Process opens `requests/clone`, writes a valid logical
  request document, and clunks the fid
- **THEN** Host Mount Service publishes one `pending` request with a stable ID
- **AND** no grant or namespace access exists before native authorization

#### Scenario: Agent commits an invalid request
- **WHEN** the request document has a reserved or non-normal namespace path,
  unsupported access, blank reason, or a Host path field
- **THEN** clunk fails without publishing a request or grant

#### Scenario: Legacy flat protocol is inspected
- **WHEN** a client walks the Host Mount Service tree
- **THEN** no flat writable approval, projection, or legacy `request` surface is
  present beside the clone-based protocol

### Requirement: Host Mount request status is terminal and resumable
A committed Host Mount request SHALL move from `pending` to exactly one of
`approved`, `rejected`, `cancelled`, or `failed`. Terminal status SHALL be
immutable, `grant` SHALL identify an approved service grant, `error` SHALL carry
concise terminal failure or rejection detail, and clients SHALL be able to
resume waiting by request reference and event offset after a Yield or runtime
restart. The requesting Process MAY cancel only its own pending request through
the Process-scoped request tree by writing `cancelled`; it MUST NOT write
`approved`, `rejected`, or `failed`. Agent Runtime SHALL settle or cancel a
pending service request before clearing the corresponding Agent Machine wait.

#### Scenario: Pending request is approved
- **WHEN** the Host adapter authorizes a directory and Host Mount Service creates
  its export and projection
- **THEN** the request becomes `approved` and its `grant` file references the
  service-owned grant
- **AND** later decision attempts cannot replace that terminal result

#### Scenario: Runtime resumes a pending mount Yield
- **WHEN** Agent Machine restarts with the request reference and last event
  offset recorded in durable evidence
- **THEN** it rereads status or continues the request event stream
- **AND** it does not recreate the request or depend on an in-memory approval
  callback

#### Scenario: Runtime abandons a pending mount Yield
- **WHEN** a new turn or cancellation clears an Agent Machine wait for a pending
  Host Mount request
- **THEN** Agent Runtime first asks Host Mount Service to mark that request
  `cancelled` through the requesting Process's scoped status file
- **AND** the Machine wait is not cleared while the service request can still be
  approved
- **AND** approval-like status writes from the requesting Process are rejected

#### Scenario: Approval wins the race with runtime abandonment
- **WHEN** Host Mount Service has claimed an approval decision before the
  requesting runtime's cancellation write commits
- **THEN** Agent Runtime waits for the service to publish an immutable terminal
  result instead of treating the still-pending decision as cancellation failure
- **AND** an approved result retains its opaque grant reference and logical
  projection before clearing the Machine wait
- **AND** later child launch isolation cannot mistake the live mount for ambient
  authority
