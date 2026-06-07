## ADDED Requirements

### Requirement: Alan-owned terminal process lifecycle is authoritative
The macOS shell host SHALL treat Alan runtime service process state as
authoritative for terminal lifecycle, close guards, control-plane delivery, and
metadata projection when a terminal ContentInstance uses the Alan-owned PTY
runtime path.

#### Scenario: Foreground process changes
- **WHEN** the Alan-owned PTY runtime observes foreground process or process-group changes
- **THEN** shell lifecycle metadata updates the corresponding terminal ContentInstance
- **AND** the update does not depend on the terminal view being visible

#### Scenario: Renderer reports stale process state
- **WHEN** renderer metadata conflicts with Alan-owned process lifecycle state
- **THEN** Alan-owned runtime state wins for child-process status, close guards, signal eligibility, and text-delivery acceptance
- **AND** renderer metadata may be retained as diagnostics

### Requirement: Terminal shutdown uses Alan-owned process control
For Alan-owned PTY runtimes, confirmed close and runtime shutdown SHALL use
Alan-owned process and process-group controls before finalizing terminal
ContentInstance state.

#### Scenario: Graceful close is confirmed
- **WHEN** a user confirms closing terminal content with active foreground work
- **THEN** Alan requests graceful shutdown through the Alan-owned PTY/process runtime
- **AND** Alan observes bounded output or exit state before force finalization policy runs

#### Scenario: Force close is required
- **WHEN** graceful shutdown times out or the process ignores the request
- **THEN** Alan may escalate through configured process-group signal policy
- **AND** the final shell state reports interrupted or forced shutdown metadata without exposing raw process handles

### Requirement: Runtime replacement does not claim cross-app continuity
Alan-owned PTY runtime ownership SHALL improve in-process terminal control, but
MUST NOT claim terminal process continuity across Alan app termination unless a
separate daemon-owned runtime capability is implemented.

#### Scenario: App restarts after Alan-owned PTY runtime
- **WHEN** alan restores a terminal ContentInstance after app restart
- **THEN** alan creates a new runtime from persisted snapshot data
- **AND** alan does not claim that the prior PTY, process group, foreground application, or file descriptors are still live

#### Scenario: Daemon ownership is added later
- **WHEN** a future change introduces daemon-owned PTY runtime survival across app quit
- **THEN** that change updates lifecycle and persistence specs before exposing cross-app terminal continuity

### Requirement: Terminal delivery follows PTY readiness
For Alan-owned PTY runtimes, terminal text delivery SHALL be acknowledged only
after Alan-owned PTY input accepts or durably queues the bytes according to the
terminal ContentInstance delivery policy.

#### Scenario: PTY accepts input
- **WHEN** `terminal.send_text` targets terminal content with an input-ready Alan-owned PTY runtime
- **THEN** the response reports `applied: true`, accepted byte count, and terminal `content_id`
- **AND** the response does not depend on renderer visibility

#### Scenario: Renderer is ready but PTY is closed
- **WHEN** `terminal.send_text` targets terminal content whose renderer is still attached but whose Alan-owned PTY is closed
- **THEN** the response reports `applied: false` with a stable closed-runtime error
- **AND** no accepted bytes are claimed
