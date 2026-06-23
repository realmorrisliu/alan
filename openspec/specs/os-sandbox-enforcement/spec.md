# os-sandbox-enforcement Specification

## Purpose
TBD - created by archiving change add-os-sandbox-enforcement. Update Purpose after archive.
## Requirements
### Requirement: Pluggable OS sandbox backends
Tool execution SHALL be constrained by a selectable sandbox backend behind a common abstraction, with macOS and Linux backends and a degraded path-guard fallback.

#### Scenario: macOS selects the Seatbelt backend
- **WHEN** the agent runs on macOS with Seatbelt available
- **THEN** tool execution is confined by the Seatbelt backend

#### Scenario: Linux selects an OS backend when available
- **WHEN** the agent runs on Linux with the required kernel capabilities
- **THEN** tool execution is confined by the Linux backend (filesystem and network controls)

#### Scenario: Fallback to the path guard
- **WHEN** no OS sandbox backend is available
- **THEN** backend selection falls back to the workspace path guard

### Requirement: Kernel-enforced confinement independent of command syntax
When an OS sandbox backend is active, confinement of filesystem writes to the workspace and control of network access SHALL be enforced by the operating system regardless of how a command is written.

#### Scenario: Internal-write command is confined
- **WHEN** a command writes outside the workspace through program-internal logic without an explicit path operand
- **THEN** the OS sandbox blocks the out-of-workspace write
- **AND** confinement does not depend on parsing the command string

#### Scenario: Network is controlled by the sandbox
- **WHEN** a confined command attempts network access that the active policy does not permit
- **THEN** the OS sandbox prevents it

### Requirement: Safe degradation when no sandbox is available
When no enforcing sandbox backend is available, the system SHALL NOT auto-approve bash or network operations and SHALL escalate them instead. Sandbox-unavailable SHALL NOT be treated as sandbox-disabled-and-allow.

#### Scenario: No backend means escalate
- **WHEN** the host has no available OS sandbox backend
- **THEN** bash and network operations are escalated for human approval rather than auto-approved

#### Scenario: Degraded state is detectable
- **WHEN** the sandbox backend is unavailable or degraded
- **THEN** the active backend/degraded state is reported (e.g. via the decision audit or backend name)

### Requirement: Heuristic command parsing is not the enforcement mechanism
With an OS sandbox backend active, the bash command-string heuristic SHALL NOT be the line of defense for confinement and SHALL be reducible to advisory pre-flight or removed.

#### Scenario: Enforcement comes from the kernel
- **WHEN** a tool runs under an active OS sandbox backend
- **THEN** confinement correctness does not rely on the bash command-shape parser

### Requirement: Linux network confinement via seccomp
On Linux, the OS sandbox backend SHALL confine network access (in addition to Landlock filesystem confinement) so that filesystem and network are both contained, matching macOS Seatbelt. This removes the platform asymmetry so that network escalations can be reviewer-judged uniformly rather than always requiring a human on Linux.

#### Scenario: Linux confines network alongside the filesystem
- **WHEN** a tool runs under the active Linux sandbox backend
- **THEN** disallowed network access is prevented by the OS (via seccomp or equivalent), as filesystem writes are by Landlock

#### Scenario: Network escalation is reviewer-eligible on both platforms
- **WHEN** an operation requests network access and an OS backend that confines network is active
- **THEN** the escalation is eligible for reviewer judgment rather than being forced to a human due to a missing network backstop

#### Scenario: Missing network confinement falls back to the human
- **WHEN** no backend that confines network is available on the host
- **THEN** network operations are surfaced to the human rather than reviewer-judged or auto-allowed

