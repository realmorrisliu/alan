## ADDED Requirements

### Requirement: Helper Managed User Sessions Have Truthful Lifecycle States
The macOS shell host SHALL represent helper-backed Managed User terminal
sessions with explicit lifecycle and error states for helper availability,
authorization, account readiness, PTY spawn, renderer attachment, child exit,
and cleanup.

#### Scenario: Helper is unavailable during launch
- **WHEN** a terminal launch resolves to a `managed_user` profile and the
  privileged helper is missing, outdated, invalid, or unreachable
- **THEN** the terminal ContentInstance records a non-ready helper state
- **AND** the UI and control plane do not report terminal input as accepted by a
  live managed-user process

#### Scenario: Helper rejects launch
- **WHEN** the helper rejects `startManagedUserPTY` because the account is not
  Alan managed, not ready, invalid, or not allowed for the current channel
- **THEN** the terminal ContentInstance records the sanitized helper rejection
  state
- **AND** Alan does not retry through sudoers or an unmanaged command path

#### Scenario: Renderer attachment fails after PTY starts
- **WHEN** the helper starts a managed-user PTY session but Ghostty attachment
  fails
- **THEN** Alan records renderer failure separately from helper PTY creation
- **AND** the helper session is terminated or cleaned up according to terminal
  close policy

#### Scenario: Managed user child exits
- **WHEN** the helper reports that a managed-user child process exited
- **THEN** terminal lifecycle metadata records exit status and helper session
  finality
- **AND** later text delivery does not claim success unless a new runtime is
  explicitly started

### Requirement: Helper Session Cleanup Follows Terminal Ownership
The macOS shell host SHALL close helper-backed Managed User PTY sessions through
the same terminal ContentInstance runtime finalization boundary used by ordinary
terminal runtimes.

#### Scenario: Managed user pane is closed
- **WHEN** a user closes a PaneSlot that mounts helper-backed Managed User
  terminal content
- **THEN** the runtime service finalizes the terminal ContentInstance exactly
  once
- **AND** the helper receives the corresponding terminate or cleanup request for
  the managed-user PTY session

#### Scenario: Client connection is lost
- **WHEN** Alan loses its helper connection while helper-backed terminal
  sessions are active
- **THEN** shell state records helper disconnect diagnostics for affected
  terminal ContentInstances
- **AND** the helper cleans up sessions bound to that connection when possible
