# child-run-lifecycle Specification

## Purpose
Define delegated child-run lifecycle behavior: registration before first
submission, liveness and timeout classification, progress metadata, projection
authority relative to `/proc` and the agent overlay, and governed
parent-initiated termination.
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

### Requirement: Child-Run Records Are A Projection, Not A Second Process Table
Child-run records SHALL be a delegation-scoped projection over `/proc`
parentage plus launch and handoff metadata. For process state (existence,
parentage, liveness, terminal exit), `/proc` and the agent overlay
(`children/`, aggregate `events`) SHALL be authoritative; a child-run record
that disagrees with them SHALL be treated as stale and corrected from the
authoritative surfaces, never the reverse.

#### Scenario: Record disagrees with /proc
- **WHEN** a child-run record reports a child as running while `/proc` shows the
  process exited
- **THEN** consumers and reconciliation logic treat the process state from
  `/proc` as truth and update the record to terminal

#### Scenario: Delegation metadata has no /proc equivalent
- **WHEN** a consumer needs delegation-scoped metadata (launch metadata,
  classified requirements, handoff references, termination actor/reason)
- **THEN** the child-run record is the owner of that metadata, because `/proc`
  deliberately does not model delegation semantics

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
