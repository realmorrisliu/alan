# child-run-lifecycle Specification

## Purpose
Define delegated child-run lifecycle behavior: registration before first
submission, liveness and timeout classification, progress metadata, projection
authority relative to `/proc` and the agent overlay, and governed
parent-initiated termination.

## Requirements

### Requirement: Child Run Registration
Alan SHALL register each delegated child as a child Agent Process before its initial turn. The
record SHALL contain parent and child Process paths or pids, Process Launch Context and namespace
metadata, rollout/checkpoint path when available, launch metadata, creation time, and starting or
running status. The record SHALL be a projection of `/proc` and `/agent` truth and SHALL identify
execution only through those Process and durable-evidence owners.

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
- **THEN** it reconstructs current lifecycle and compact progress state from
  `/proc`, current AgentFS snapshots, and the durable child-run projection
- **AND** it establishes new observation-local stream offsets from the current
  stream state without promising replay of records missed while detached
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

### Requirement: Named Workstreams support persistent resumption and one-off auto-collapse
Root SHALL manage named child workstreams recorded in the project's System Store metadata. A `persistent` workstream SHALL save its latest checkpoint reference on turn completion and transition to `idle` for later resumption into a fresh child Process, whereas a `one_off` workstream SHALL automatically transition to `archived` upon completion and collapse out of the active roster while retaining inspectable durable rollout evidence.

#### Scenario: Root resumes a persistent workstream
- **WHEN** Root dispatches new work matching an existing `idle` `persistent` workstream
- **THEN** Alan spawns a new child Agent Process from that workstream's latest checkpoint after revalidating current grants and budgets

#### Scenario: One-off workstream collapses automatically after completion
- **WHEN** a `one_off` child workstream completes its task
- **THEN** its record transitions to `archived` and collapses from the active workstream list
- **AND** its durable rollout evidence remains inspectable through existing evidence paths

### Requirement: Foreground Root supports in-instance asynchronous workstream concurrency with tiered write isolation
Foreground Root SHALL run multiple child workstreams asynchronously while staying responsive. Read-only streams SHALL run concurrently; routine writes on the same Host Mount SHALL serialize via a single-writer queue; and concurrent long-running writes SHALL isolate in an on-demand `git worktree` with narrowed write grants and explicit merge handoff.

#### Scenario: Read-only workstreams run concurrently while Root stays interactive
- **WHEN** Root dispatches two `read_only` research workstreams asynchronously
- **THEN** both child Agent Processes execute concurrently inside the foreground instance
- **AND** Root continues accepting and responding to subsequent user prompts

#### Scenario: Routine write workstreams serialize on the shared working tree
- **WHEN** a second routine write workstream targets a Host Mount currently held by an active `shared_queue` writer
- **THEN** it waits in the per-grant FIFO writer queue until the first writer completes and releases the lock

#### Scenario: Concurrent long-running write workstreams isolate via git worktree
- **WHEN** concurrent long-running write execution is requested on a repository with an active writer
- **THEN** Alan allocates an isolated `git worktree` and narrows the new child Process's Host Mount write grant to that worktree path
- **AND** on completion the child returns a diff and merge handoff rather than silently mutating the primary working tree

### Requirement: Per-stream lease locks prevent concurrent checkpoint corruption
Every interactive resumption or scheduled Stateful Cron Stream execution targeting a named workstream SHALL acquire that stream's System Store lease lock before mutating its checkpoint or rollout pointer.

#### Scenario: Concurrent stream resume is prevented by per-stream lease lock
- **WHEN** an interactive session holds the lease lock for a named stream and a second caller attempts to resume that same stream
- **THEN** the second caller observes lock contention and aborts or queues without corrupting the active checkpoint

### Requirement: Delegated child and workstream supervision waits on kernel and agent event streams
`DelegatedChildRunSupervisor` and `async_workstreams` SHALL wait on `/proc/<pid>/wait` and `/agent/<pid>/events` streams rather than fixed `10–250ms` sleep polling, and parent-owned child launches SHALL set `link_parent: true`.

#### Scenario: Child run supervisor reacts to child exit or yield without sleep polling
- **WHEN** a delegated child agent pauses on a request or exits
- **THEN** `DelegatedChildRunSupervisor` wakes directly from `/agent/<child_pid>/events` or `/proc/<child_pid>/wait` within timeout/cancellation bounds

#### Scenario: Parent cancellation cascades to linked child agent
- **WHEN** a parent agent is aborted or exits while supervising a `link_parent: true` child agent
- **THEN** the kernel cancels the child process deterministically and workstream quiescence observes completion without fixed polling ticks
