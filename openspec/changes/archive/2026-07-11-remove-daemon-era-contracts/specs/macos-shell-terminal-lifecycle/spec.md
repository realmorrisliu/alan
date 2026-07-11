## MODIFIED Requirements

### Requirement: Runtime replacement does not claim cross-app continuity

Alan-owned PTY runtime ownership SHALL improve in-process terminal control, but MUST NOT claim terminal process continuity across Alan app termination.

#### Scenario: App restarts after Alan-owned PTY runtime

- **WHEN** Alan restores a terminal ContentInstance after app restart
- **THEN** Alan creates a new runtime from persisted snapshot data
- **AND** Alan does not claim that the prior PTY, process group, foreground application, or file descriptors are still live

#### Scenario: Cross-app continuity is proposed later

- **WHEN** a future change proposes PTY survival across app termination
- **THEN** that change defines the lifecycle owner, persistence semantics, security boundary, and failure behavior in OpenSpec before exposing continuity
- **AND** this cleanup does not preselect the owning service or attachment mechanism
