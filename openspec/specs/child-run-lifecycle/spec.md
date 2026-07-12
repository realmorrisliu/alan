# child-run-lifecycle Specification

## Purpose
Define delegated child-run lifecycle behavior: registration before first
submission, liveness and timeout classification, progress metadata, projection
authority relative to `/proc` and the agent overlay, and governed
parent-initiated termination.
## Requirements
### Requirement: Child Run Registration
Alan SHALL register each delegated child as a child Agent Process before its initial turn. The
record SHALL contain parent and child Process paths or pids, workspace and namespace metadata,
rollout/checkpoint path when available, launch metadata, creation time, and starting or running
status. The record SHALL be a projection of `/proc` and `/agent` truth and SHALL identify execution
only through those Process and durable-evidence owners.

#### Scenario: Child registration precedes initial input
- **WHEN** a parent Agent Process spawns a child Agent Executable
- **THEN** the child record is visible before the initial turn is delivered
- **AND** parent/child identity resolves to concrete Process and AgentFS paths

#### Scenario: Child launch fails after allocation
- **WHEN** launch fails after a child pid, Process path, or rollout/checkpoint path is known
- **THEN** the child record reaches a terminal failure state with that evidence
- **AND** the allocated Process record does not remain in a starting or running state

### Requirement: Child Liveness And Timeout Classification
The system SHALL classify child timeouts from authoritative `/proc` lifecycle
state and freshness observed on child-owned AgentFS activity/progress files,
rather than only elapsed launch wall-clock time or an engine broadcast
heartbeat. The supervisor SHALL NOT require the child's runtime handle or event
receiver.

#### Scenario: Child exceeds original timeout while file heartbeat is fresh
- **WHEN** a child Agent Process runs longer than its configured idle timeout
  but `/proc/<pid>` remains live and `machine/ui/activity` or another owned
  progress file remains fresh inside the idle window
- **THEN** the parent runtime MUST NOT classify the child as `timed_out`

#### Scenario: Child becomes idle
- **WHEN** `/proc/<pid>` remains non-terminal but no child-owned heartbeat or
  progress file advances inside the idle timeout window
- **THEN** the child-run record is updated to `timed_out`
- **AND** the handoff includes the latest observed event kind and compact
  status summary when available

#### Scenario: Child Process exits
- **WHEN** `/proc/<pid>/status` records a terminal exit
- **THEN** the supervisor classifies the child from that Process exit state
- **AND** a stale activity file or child-run projection cannot keep it running

### Requirement: Child Progress Metadata
The system SHALL update child-run progress metadata by observing monotonic
offsets, timestamps, and current snapshots on the child AgentFS and by reading
generic Process state from `/proc`. The child-run record SHALL retain the latest
progress time, event kind, and compact status summary; structured per-source
offsets MAY remain observation-local. Progress sources SHALL include relevant
`io/output`, request/action streams, `machine/ui/events`, and
`machine/ui/activity`; an in-process child runtime event receiver SHALL NOT be a
progress source.

#### Scenario: Child file stream advances
- **WHEN** the parent observes a new record for the active child on an owned
  AgentFS stream
- **THEN** the child-run record updates latest progress time, event kind, and a
  compact status summary derived from the observed offsets or sequence

#### Scenario: Child is active but quiet
- **WHEN** a child Agent Process remains active but produces no user-visible
  output
- **THEN** the child updates its owned activity snapshot or heartbeat timestamp
  so the supervisor can distinguish quiet activity from a dead child

#### Scenario: Supervisor restarts observation
- **WHEN** the parent supervisor reattaches after missing live time
- **THEN** it resumes from recorded file offsets and current `/proc` state
- **AND** progress reconstruction does not depend on replaying an engine
  broadcast channel

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
