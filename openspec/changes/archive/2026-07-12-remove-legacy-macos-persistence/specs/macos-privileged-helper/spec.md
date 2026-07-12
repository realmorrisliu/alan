## MODIFIED Requirements

### Requirement: Helper API Is Declarative
The privileged helper SHALL expose a narrow typed API for current Alan-owned
privileged operations and MUST NOT expose arbitrary command execution, raw shell
scripts, raw sudoers content, arbitrary executable launch, or legacy-sudoers
diagnosis and cleanup operations.

#### Scenario: Managed User diagnosis is requested
- **WHEN** Alan requests `diagnoseManagedUser` for a structured account
  identifier
- **THEN** the helper returns typed account, home, shell, ownership-marker,
  Terminal Profile handoff, and managed PTY readiness state
- **AND** the response contains no sudoers state, legacy path, or raw privileged
  command payload

#### Scenario: Managed User repair is requested
- **WHEN** Alan requests `applyManagedUserPlan` with a helper-authored
  declarative plan
- **THEN** the helper applies only current account, home, hidden-login,
  ownership-marker, and verification operations represented in that plan
- **AND** the helper rejects any operation not represented by a known current
  typed plan step

#### Scenario: Legacy cleanup operation is requested
- **WHEN** a client sends a retired legacy-sudoers diagnosis, cleanup, or
  rollback step after the hard cut
- **THEN** the helper rejects the operation as unsupported
- **AND** it does not inspect or mutate the retired sudoers path

#### Scenario: Arbitrary command is requested
- **WHEN** a client attempts to send raw shell text, a raw sudoers fragment, or
  an arbitrary executable path to the helper
- **THEN** the helper rejects the request before performing privileged work
- **AND** the rejection is reported as a typed authorization or validation error

### Requirement: Helper Observability Is Sanitized
The privileged helper SHALL log and report current privileged operation results
with sanitized identifiers and error codes without recording credentials,
terminal transcripts, raw command payloads, full privileged scripts, or sudoers
content and paths.

#### Scenario: Helper operation fails
- **WHEN** a helper operation fails during account repair, ownership-marker
  maintenance, verification, or managed-user PTY launch
- **THEN** logs and app-facing diagnostics include operation id, channel,
  account identifier, high-level step, and sanitized error status
- **AND** logs do not include terminal transcript content, passwords, raw shell
  scripts, sudoers paths, or sudoers contents
