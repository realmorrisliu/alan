## ADDED Requirements

### Requirement: Agent-requested host mounts require approval
Agent Processes SHALL be able to request host directory mounts through a
runtime-provided `request_mount` tool. The tool SHALL validate the requested
namespace path, host path, access mode, and reason before creating an approval
request. A valid mount request SHALL pause the turn with a confirmation Yield and
SHALL NOT grant access before approval.

#### Scenario: Valid mount request pauses for approval
- **WHEN** an Agent Process calls `request_mount` with an absolute `/mnt/<name>` namespace path, an absolute host path, a supported access mode, and a non-empty reason
- **THEN** the runtime emits a confirmation Yield for that request
- **AND** the turn pauses without changing the mounted namespace

#### Scenario: Invalid mount request is rejected before approval
- **WHEN** an Agent Process calls `request_mount` with a reserved namespace path, relative path component, relative host path, unsupported access mode, or blank reason
- **THEN** the runtime completes the tool call with an invalid-request result
- **AND** no approval Yield or mount grant is created

### Requirement: Mount authorization cannot be auto-allowed
The runtime SHALL treat every valid mount request as an authorization boundary.
Policy evaluation MAY deny the request, but an allow decision SHALL be upgraded
to escalation so the request is approved outside the agent's control.

#### Scenario: Policy allow is upgraded to escalation
- **WHEN** policy evaluation allows a valid `request_mount` call
- **THEN** the runtime still emits an approval Yield
- **AND** the audit reason states that host mount grants require approval

#### Scenario: Policy deny blocks a mount request
- **WHEN** policy evaluation denies a valid `request_mount` call
- **THEN** the runtime returns a blocked-by-policy result
- **AND** no approval Yield or mount grant is created

### Requirement: Approved mount requests produce auditable grants
When a mount request is approved, the runtime SHALL return a structured
`request_mount` tool result and record a normalized `host_mount_grant` audit
event. This slice SHALL NOT claim the approved grant has already reconfigured
the live namespace or OS sandbox.

#### Scenario: Approved request is recorded
- **WHEN** a pending mount request is resumed with an approve choice
- **THEN** the runtime records a `host_mount_grant` event with namespace path, host path, access, reason, checkpoint id, and approved status
- **AND** the agent receives a `request_mount` tool result that states the grant is approved but not yet applied live

#### Scenario: Rejected request returns a rejection result
- **WHEN** a pending mount request is resumed with a reject choice
- **THEN** the runtime returns a `request_mount` tool result with rejected status
- **AND** no approved `host_mount_grant` event is recorded
