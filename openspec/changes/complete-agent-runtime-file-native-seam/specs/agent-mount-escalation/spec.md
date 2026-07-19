## MODIFIED Requirements

### Requirement: Agent-requested host mounts require approval
Agent Processes SHALL request Host Mounts through the runtime-provided
`request_mount` control operation. The operation SHALL validate a normalized
absolute `/mnt/<name>` namespace path, supported access, non-empty reason, and
optional label, then commit that logical request to Host Mount Service. It MUST
NOT accept or derive a raw Host OS path. A valid request SHALL pause the turn
with an authorization-wait Yield while service status is `pending` and SHALL
NOT grant access before Host-adapter authorization.

#### Scenario: Valid logical mount request pauses for approval
- **WHEN** an Agent Process calls `request_mount` with a valid namespace path,
  supported access, non-empty reason, and optional label
- **THEN** the runtime commits one Host Mount Service request and emits a Yield
  containing its opaque request reference
- **AND** the turn pauses without changing the Process namespace

#### Scenario: Invalid mount request is rejected before approval
- **WHEN** an Agent Process calls `request_mount` with a reserved or non-normal
  namespace path, unsupported access, blank reason, or a raw Host path field
- **THEN** the runtime completes the control call with an invalid-request result
- **AND** no Host Mount Service request, Yield, or grant is created

### Requirement: Mount authorization cannot be auto-allowed
Every valid mount request SHALL cross Host Mount Service and a Host-adapter
native authorization boundary. Agent policy evaluation MAY deny the request,
but an allow decision MUST only permit creation of the pending service request
and MUST NOT create or approve a grant. AgentFS and Agent Machine decision files
MUST NOT bypass native authorization.

#### Scenario: Policy allow creates only a pending request
- **WHEN** policy evaluation allows a valid `request_mount` call
- **THEN** the runtime creates a pending Host Mount Service request and Yields
- **AND** only the Host adapter and Host Mount Service can produce approval

#### Scenario: Policy deny blocks a mount request
- **WHEN** policy evaluation denies a valid `request_mount` call
- **THEN** the runtime returns a blocked-by-policy result
- **AND** no service request or grant is created

#### Scenario: Agent writes an approval decision
- **WHEN** an Agent Process writes an approval value to its own AgentFS request
- **THEN** Host Mount Service status remains unchanged
- **AND** no namespace or native sandbox authority is granted

### Requirement: Approved mount requests produce auditable grants
When Host Mount Service reaches a terminal request status, Agent Machine SHALL
resume from the opaque request reference and return a structured
`request_mount` result containing request identity, logical namespace path,
access, status, and approved grant reference or concise error. Agent-visible
results, AgentFS, Machine state, rollout/checkpoint evidence, and Alan OS audit
records MUST NOT contain the raw Host OS path. Projection and revocation truth
SHALL remain in Host Mount Service rather than an engine-owned audit event.

#### Scenario: Approved request resumes execution
- **WHEN** Host Mount Service reports `approved` and exposes the service-owned
  grant
- **THEN** Agent Machine resumes the control call with an approved result and
  opaque grant reference
- **AND** future namespace and Tool Process access derive from the mounted grant
  handle
- **AND** neither result nor durable evidence contains a Host path

#### Scenario: Rejected request resumes execution
- **WHEN** Host Mount Service reports `rejected`
- **THEN** Agent Machine returns a rejected result with the logical request
  identity and concise reason
- **AND** no grant or namespace access is created

#### Scenario: Failed or cancelled request resumes execution
- **WHEN** Host Mount Service reports `failed` or `cancelled`
- **THEN** Agent Machine returns the matching terminal result without claiming
  namespace or Tool sandbox projection
