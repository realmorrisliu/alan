## MODIFIED Requirements

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
- **AND** the handoff includes the latest observed file path, offset or
  timestamp, and compact status

#### Scenario: Child Process exits
- **WHEN** `/proc/<pid>/status` records a terminal exit
- **THEN** the supervisor classifies the child from that Process exit state
- **AND** a stale activity file or child-run projection cannot keep it running

### Requirement: Child Progress Metadata
The system SHALL update child-run progress metadata by observing monotonic
offsets, timestamps, and current snapshots on the child AgentFS and by reading
generic Process state from `/proc`. Progress sources SHALL include relevant
`io/output`, request/action streams, `machine/ui/events`, and
`machine/ui/activity`; an in-process child runtime event receiver SHALL NOT be a
progress source.

#### Scenario: Child file stream advances
- **WHEN** the parent observes a new record for the active child on an owned
  AgentFS stream
- **THEN** the child-run record updates latest progress time, source path,
  latest offset or sequence, and compact status when derivable

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
