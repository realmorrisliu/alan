# agent-mount-escalation Specification

## Purpose
TBD - created by archiving change add-agent-mount-escalation. Update Purpose after archive.
## Requirements
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
event. The result and event SHALL preserve the approved grant record and SHALL
report live namespace application state independently through
`namespace_applied` and, when application is unavailable or fails, a concise
`namespace_error` or equivalent reason. The result SHALL NOT imply Linux
reification or native subprocess visibility at the requested `/mnt/<name>` path.

#### Scenario: Approved request records successful namespace application
- **WHEN** a pending mount request is resumed with an approve choice and the namespace applicator applies the grant successfully
- **THEN** the runtime records a `host_mount_grant` event with namespace path, host path, access, reason, checkpoint id, and approved status
- **AND** the agent receives a `request_mount` tool result that states the grant is approved
- **AND** the tool result and audit event report `namespace_applied = true`

#### Scenario: Approved request reports unavailable or failed namespace application
- **WHEN** a pending mount request is resumed with an approve choice but no namespace applicator is available or namespace application fails
- **THEN** the runtime records a `host_mount_grant` event with namespace path, host path, access, reason, checkpoint id, and approved status
- **AND** the agent receives a `request_mount` tool result that states the grant is approved
- **AND** the tool result and audit event report `namespace_applied = false`
- **AND** the result includes a concise `namespace_error` or equivalent reason
- **AND** the result does not claim Linux reification or native subprocess visibility at `/mnt/<name>`

#### Scenario: Rejected request returns a rejection result
- **WHEN** a pending mount request is resumed with a reject choice
- **THEN** the runtime returns a `request_mount` tool result with rejected status
- **AND** no approved `host_mount_grant` event is recorded

