# macos-privileged-helper Specification

## Purpose
Defines the signed, install-channel-scoped macOS privileged helper, including
explicit lifecycle authorization, declarative Managed User operations, client
validation, PTY supervision, and sanitized observability.
## Requirements
### Requirement: Privileged Helper Is Channel Scoped
Alan for macOS SHALL install and communicate with a signed privileged helper
whose service identity is scoped to the active macOS install channel.

#### Scenario: Stable and dev helpers are isolated
- **WHEN** stable `Alan.app` and local `Alan Dev.app` are installed on the same
  Mac
- **THEN** each channel uses a distinct helper bundle identifier, launchd
  service label, Mach service name, and helper data root
- **AND** stable Alan cannot send privileged requests to the dev helper
- **AND** Alan Dev cannot send privileged requests to the stable helper

#### Scenario: Helper signature is invalid
- **WHEN** the app detects a helper with a missing, stale, or invalid code
  signature for the current channel
- **THEN** helper-backed privileged operations are unavailable
- **AND** Settings reports a reinstall or update action instead of attempting
  account repair or managed-user PTY launch

### Requirement: Helper Lifecycle Is Explicitly Authorized
Alan for macOS SHALL require explicit system administrator authorization for
installing, updating, or uninstalling the privileged helper and SHALL NOT reuse
administrator credentials for later Managed User operations.

#### Scenario: Helper install is requested
- **WHEN** the user chooses to install the privileged helper from Settings
- **THEN** macOS presents an administrator authorization flow for the helper
  install
- **AND** Alan records only helper status after the operation completes
- **AND** Alan does not store reusable administrator credentials

#### Scenario: Helper is healthy
- **WHEN** the privileged helper is installed, current, signed for the active
  channel, and responding to status checks
- **THEN** Managed User create, repair, verify, and terminal launch requests use
  the helper without presenting per-step administrator password prompts

### Requirement: Helper API Is Declarative
The privileged helper SHALL expose a narrow typed API for Alan-owned privileged
operations and MUST NOT expose arbitrary command execution, raw shell scripts,
raw sudoers content, or arbitrary executable launch.

#### Scenario: Managed User diagnosis is requested
- **WHEN** Alan requests `diagnoseManagedUser` for a structured account
  identifier
- **THEN** the helper returns typed account, home, shell, ownership, legacy
  sudoers, Terminal Profile handoff, and managed PTY readiness state
- **AND** the response does not include raw privileged command payloads

#### Scenario: Managed User repair is requested
- **WHEN** Alan requests `applyManagedUserPlan` with a helper-authored
  declarative plan
- **THEN** the helper applies only the account, home, hidden-login,
  ownership-marker, legacy cleanup, and verification operations represented in
  that plan
- **AND** the helper rejects any operation that is not represented by a known
  typed plan step

#### Scenario: Arbitrary command is requested
- **WHEN** a client attempts to send raw shell text, a raw sudoers fragment, or
  an arbitrary executable path to the helper
- **THEN** the helper rejects the request before performing privileged work
- **AND** the rejection is reported as a typed authorization or validation error

### Requirement: Helper Validates Clients And Requests
The privileged helper SHALL validate the connecting client identity, active
install channel, request capability, account identifier, home path, and shell
allowlist before performing privileged work.

#### Scenario: Client identity does not match helper channel
- **WHEN** a process that is not the matching signed Alan app for the helper's
  channel connects to the helper
- **THEN** the helper rejects the connection or request
- **AND** no account, home, sudoers, or PTY state is changed

#### Scenario: Account identifier is invalid
- **WHEN** a request includes an account name with a slash, whitespace, shell
  metacharacter, empty value, or otherwise invalid Unix identifier
- **THEN** the helper rejects the request as invalid
- **AND** the helper does not derive paths or execute commands from that value

#### Scenario: Account is not Alan managed
- **WHEN** a requested Unix account exists but does not carry Alan-managed
  ownership evidence for the active channel
- **THEN** the helper reports `accountNotAlanManaged`
- **AND** helper repair and managed-user PTY launch are unavailable for that
  account

### Requirement: Helper Owns Managed User PTY Supervision
The privileged helper SHALL provide managed-user PTY sessions only for
Alan-managed accounts and SHALL own the root-only process setup, uid/gid/group
drop, process-group tracking, signal delivery, reaping, and cleanup for those
sessions.

#### Scenario: Managed-user PTY starts
- **WHEN** Alan requests `startManagedUserPTY` for a ready Alan-managed account
- **THEN** the helper allocates a PTY, starts the login shell as the target
  account, tracks the child process group, and returns the PTY endpoint/session
  metadata needed by Alan
- **AND** the helper does not return privileged process handles or root
  credentials to the app

#### Scenario: Client disconnects
- **WHEN** the Alan client connection that owns a helper-managed PTY session
  disconnects
- **THEN** the helper terminates or cleans up sessions bound to that connection
  according to the managed-user PTY policy
- **AND** the helper reports the final session state when possible

### Requirement: Helper Observability Is Sanitized
The privileged helper SHALL log and report privileged operation results with
sanitized identifiers and error codes without recording credentials, terminal
transcripts, raw command payloads, or full privileged scripts.

#### Scenario: Helper operation fails
- **WHEN** a helper operation fails during account repair, legacy cleanup, or
  managed-user PTY launch
- **THEN** logs and app-facing diagnostics include operation id, channel,
  account identifier, high-level step, and sanitized error status
- **AND** logs do not include terminal transcript content, passwords, raw shell
  scripts, or full sudoers contents
