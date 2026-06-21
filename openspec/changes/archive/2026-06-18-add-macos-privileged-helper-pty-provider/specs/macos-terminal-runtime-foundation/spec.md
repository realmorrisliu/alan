## ADDED Requirements

### Requirement: Managed User PTY Provider Depends On Alan-Owned Runtime
Helper-backed Managed User terminal launch SHALL use Alan-owned PTY runtime
handles and MUST NOT launch managed-user terminals through renderer-owned
process state or `sudo` command fallback.

#### Scenario: Alan-owned PTY runtime is unavailable
- **WHEN** a `managed_user` Terminal Profile is selected before the Alan-owned
  PTY runtime and Ghostty external-PTY attachment are available
- **THEN** alan reports the managed-user terminal launch path as unavailable
- **AND** alan does not fall back to `sudo_user`, `osascript`, or a raw custom
  command to enter the Managed User

#### Scenario: Helper provides PTY endpoint
- **WHEN** Alan creates a terminal ContentInstance for a ready `managed_user`
  profile
- **THEN** the terminal runtime requests a helper-owned PTY endpoint for the
  account
- **AND** the resulting runtime handle remains keyed by terminal
  ContentInstance identity
- **AND** Ghostty attaches as renderer/protocol adapter over the Alan-provided
  endpoint

### Requirement: Managed User PTY Control Routes Through Helper Sessions
For helper-owned Managed User PTY sessions, the terminal runtime service SHALL
route resize, text delivery, EOF, interrupt, terminate, kill, and exit
observation through the Alan runtime handle and helper session boundary.

#### Scenario: Managed user terminal is resized
- **WHEN** a helper-backed Managed User terminal ContentInstance size changes
- **THEN** Alan applies the PTY window size through the helper session
- **AND** renderer resize follows the same dimensions without becoming the
  source of process truth

#### Scenario: Managed user terminal receives input
- **WHEN** terminal input is delivered to a helper-backed Managed User terminal
- **THEN** Alan writes the input through the Alan-owned PTY runtime handle
- **AND** the helper session remains responsible for the privileged PTY child
  and process-group lifecycle

#### Scenario: Managed user child exits
- **WHEN** the helper-owned child process exits
- **THEN** the helper reports exit status to Alan
- **AND** the terminal runtime projects final lifecycle metadata to the terminal
  ContentInstance
