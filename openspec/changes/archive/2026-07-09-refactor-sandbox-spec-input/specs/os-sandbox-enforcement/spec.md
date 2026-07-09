## ADDED Requirements

### Requirement: Confinement input is a projected SandboxSpec
The OS sandbox SHALL confine a native subprocess from a `SandboxSpec` value —
writable roots, a read denylist, and a default network posture — rather than a
single hard-coded workspace path. The spec SHALL be a projection of the mount
declaration list, with the workspace modeled as the seed (first, default)
writable entry. When the spec carries exactly one writable root and an empty read
denylist, the emitted Seatbelt profile and Landlock ruleset SHALL be identical to
those produced from that path directly (this change is behavior-preserving).

#### Scenario: Single-root spec preserves the current profile
- **WHEN** a `SandboxSpec` is built with one writable root (the workspace) and an
  empty read denylist
- **THEN** the generated Seatbelt profile / Landlock ruleset is byte-for-byte the
  one produced today from a lone `workspace_root`
- **AND** no read-deny rule is emitted

#### Scenario: The workspace is the seed writable entry
- **WHEN** a session confines tool execution with only a workspace
- **THEN** the spec's writable roots contain exactly the workspace path
- **AND** enforcement is indistinguishable from the prior single-path confinement

#### Scenario: The read denylist is plumbed but inert at this stage
- **WHEN** the spec's read denylist is empty
- **THEN** the backends emit no read-deny rules and reads follow the existing
  allow-default posture
- **AND** the denylist parameter still threads to the backends so it can be
  populated later without changing their signatures

## MODIFIED Requirements

### Requirement: Kernel-enforced confinement independent of command syntax
When an OS sandbox backend is active, the operating system SHALL enforce
confinement of filesystem writes to the writable roots of the active sandbox spec
and control of network access, regardless of how a command is written. With a
single-entry spec the writable set is exactly the workspace, so the enforced
boundary is unchanged.

#### Scenario: Internal-write command is confined
- **WHEN** a command writes outside the spec's writable roots through
  program-internal logic without an explicit path operand
- **THEN** the OS sandbox blocks the out-of-writable-roots write
- **AND** confinement does not depend on parsing the command string

#### Scenario: Network is controlled by the sandbox
- **WHEN** a confined command attempts network access that the active policy does
  not permit
- **THEN** the OS sandbox denies the network access
- **AND** an approved network call still runs with network permitted while
  remaining filesystem-confined
