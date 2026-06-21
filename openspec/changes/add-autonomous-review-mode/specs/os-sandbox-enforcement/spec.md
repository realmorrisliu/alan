## ADDED Requirements

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
