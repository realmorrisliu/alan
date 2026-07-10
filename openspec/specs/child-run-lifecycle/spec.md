# child-run-lifecycle Specification

## Purpose
Define delegated child-run lifecycle behavior: registration before first
submission, liveness and timeout classification, progress metadata, daemon and
TUI control surfaces, and governed parent-initiated termination.
## Requirements
### Requirement: Child Run Registration
The system SHALL create a child-run record before submitting the first operation to a delegated child runtime.

#### Scenario: Delegated child is launched
- **WHEN** a parent runtime launches a delegated child runtime
- **THEN** the child-run registry contains a record with parent session id, child session id, workspace metadata, rollout path when available, launch metadata, created time, and `starting` or `running` status before the child receives its initial turn
- **AND** the launch metadata includes a bounded summary of the capability-bearing mounts and `/bin` bindings the child was spawned with, so capability investigations do not require the child process to still be alive

#### Scenario: Child launch fails after runtime startup
- **WHEN** child launch fails after a child session id or rollout path is known
- **THEN** the child-run record is updated to `failed` with terminal metadata instead of disappearing from the registry

### Requirement: Child Liveness And Timeout Classification
The system SHALL classify child timeouts by idle liveness freshness rather than only elapsed launch wall-clock time.

#### Scenario: Child exceeds original timeout while heartbeat is fresh
- **WHEN** a child runtime runs longer than its configured idle timeout duration but heartbeat or progress signals continue to arrive within the idle window
- **THEN** the parent runtime MUST NOT classify the child as `timed_out`

#### Scenario: Child becomes idle
- **WHEN** no child heartbeat or progress signal arrives within the idle timeout window
- **THEN** the child-run record is updated to `timed_out` and the handoff includes latest heartbeat/progress metadata

### Requirement: Child Progress Metadata
The system SHALL update child-run progress metadata from child events and heartbeat signals.

#### Scenario: Child emits a runtime event
- **WHEN** the parent observes a child event for the active child submission
- **THEN** the child-run record updates latest progress time, latest event cursor or sequence when available, and current compact status when derivable

#### Scenario: Child is active but quiet
- **WHEN** a child runtime is still active but produces no user-visible output
- **THEN** the child runtime or supervising controller records heartbeat freshness so the operator can distinguish quiet activity from a dead child

### Requirement: Daemon Child-Run Control Plane
The daemon SHALL expose APIs to list, read, and terminate child runs for a parent session.

#### Scenario: List child runs
- **WHEN** a client requests child runs for a parent session
- **THEN** the daemon returns all known child runs for that session with status, workspace, rollout path, latest heartbeat/progress, and terminal metadata

#### Scenario: Read one child run
- **WHEN** a client requests a child run by id under a parent session
- **THEN** the daemon returns the child-run record or a not-found error if the parent has no matching child run

#### Scenario: Request termination
- **WHEN** a client requests child termination with a reason and graceful or forceful mode
- **THEN** the daemon routes the request through the runtime child-run lifecycle transition and records actor, reason, mode, requested time, and final status

### Requirement: TUI Child-Agent Commands
The TUI SHALL expose child-agent management commands backed by the daemon child-run control plane.

#### Scenario: List child agents
- **WHEN** the operator enters `/agents`
- **THEN** the TUI lists child runs for the current session grouped or marked by lifecycle status

#### Scenario: Inspect child agent
- **WHEN** the operator enters `/agent <id>`
- **THEN** the TUI shows status, workspace, rollout path, latest heartbeat/progress, and current tool or plan summary when available

#### Scenario: Terminate child agent
- **WHEN** the operator enters `/agent terminate <id> [reason]`
- **THEN** the TUI requests graceful child-run termination and reports the resulting lifecycle state

#### Scenario: Kill child agent
- **WHEN** the operator enters `/agent kill <id> [reason]`
- **THEN** the TUI requests forceful child-run termination and reports the resulting lifecycle state

### Requirement: Governed Parent Child Termination
The parent runtime SHALL expose a governed virtual tool for terminating a known child run.

#### Scenario: Parent terminates known child
- **WHEN** the parent runtime invokes child termination with a known child id, reason, and mode
- **THEN** the runtime applies governance/audit semantics and records the termination request on the child-run record

#### Scenario: Parent terminates unknown child
- **WHEN** the parent runtime invokes child termination for an unknown child id
- **THEN** the tool returns a structured failure without changing unrelated child-run records

#### Scenario: Parent terminates terminal child
- **WHEN** the parent runtime invokes child termination for a child run that is already terminal
- **THEN** the tool returns the existing terminal state and records no duplicate termination transition
