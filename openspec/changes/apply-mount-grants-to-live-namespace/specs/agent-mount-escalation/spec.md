## MODIFIED Requirements

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
