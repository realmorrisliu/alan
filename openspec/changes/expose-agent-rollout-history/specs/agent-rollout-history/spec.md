## ADDED Requirements

### Requirement: Retained Rollouts are discoverable in the agent namespace
Agent Runtime Service SHALL expose a read-only `/agent/rollouts` directory.
Each retained Rollout SHALL be addressable at
`/agent/rollouts/<rollout-id>` as one JSONL file, where the path component is
the Rollout's existing identifier. The file SHALL expose the Rollout's ordered
records without a directory wrapper or parallel metadata projection. The
surface SHALL remain reconstructible after Agent Process exit and Alan OS Host
restart without exposing a raw System Store path.

#### Scenario: Consumer lists Rollouts after Host restart
- **WHEN** an authorized consumer lists `/agent/rollouts` after Alan OS Host
  restart
- **THEN** every valid retained Rollout appears by its existing `rollout_id`
- **AND** discovery does not depend on a prior Process Reference, renderer
  database, or Host filesystem scan

#### Scenario: Consumer opens one retained Rollout
- **WHEN** an authorized consumer opens `/agent/rollouts/<rollout-id>`
- **THEN** it reads the existing ordered Rollout JSONL records from one file
- **AND** it does not need to reconcile separate metadata, status, result, or
  evidence files

#### Scenario: Consumer attempts to mutate history
- **WHEN** a consumer opens `/agent/rollouts` or one of its descendants for
  writing
- **THEN** Agent Runtime Service rejects the write
- **AND** the discovery surface cannot become a second Rollout writer

### Requirement: Rollout discovery introduces no second execution identity
The Rollout history surface SHALL use `rollout_id` as its only entry identity
and SHALL NOT create another durable execution or interaction identity. A
history listing SHALL be a view over retained Rollouts, not a separately
persisted record set or authority.

#### Scenario: A recovered Agent Process creates a new Rollout
- **WHEN** a new Agent Process recovers Agent Machine state from a prior
  Rollout
- **THEN** the new execution remains represented by its new Rollout
- **AND** the prior Rollout remains source evidence rather than being merged
  under a new cross-execution identity

### Requirement: Retained Rollouts are the durable discovery authority
Agent Runtime Service SHALL reconstruct `/agent/rollouts` from valid retained
Rollouts in its own System Store subtree and SHALL NOT require a separately
persisted history index. Any future cache MUST be rebuildable from Rollouts and
MUST NOT become discovery authority.

#### Scenario: Rebuild starts without cache state
- **WHEN** Agent Runtime Service starts with retained Rollouts and no history
  cache
- **THEN** `/agent/rollouts` is reconstructed from those Rollouts
- **AND** no completed Rollout is lost because a parallel index is absent

### Requirement: Process exit is recorded in the existing Rollout
For an Agent Process with a producing Rollout, Alan SHALL append and flush one
`process_exit` record through the Process terminal finalization hook before
Alan Kernel publishes exit and before clean Agent Runtime Service cleanup. The
record SHALL contain the authoritative numeric Process exit code, a completion
timestamp, and the existing `AgentExecutableResult` when one is available. It
SHALL NOT introduce a second terminal status enum or fabricate a Rollout for a
best-effort execution that started without one.

Before an Agent Process becomes controllable, System Process runner SHALL
register with Agent Runtime Service a pending Process-local terminal-context
barrier and a startup cancellation path. Every Agent startup exit path SHALL
resolve the barrier exactly once with either a producing-Rollout context or an
explicit no-producing-Rollout outcome; an absent or dropped resolution SHALL
NOT be treated as no Rollout.

Immediately after Agent Machine creation succeeds, Agent Runtime Service SHALL
resolve the producing-Rollout context with the existing Rollout metadata and a
retained owning `RuntimeController` or equivalent runtime-task guard and
Process cleanup guard, before later initialization or readiness signaling. A
cloned `RuntimeHandle` alone SHALL NOT satisfy this ownership requirement. The
ordinary Agent Executable run path MAY borrow the handle to await and produce
its `ProcessOutcome`, but SHALL NOT shut down or drop the runtime-task owner or
perform Process cleanup before terminal finalization. Terminal finalization
SHALL first request startup or runtime cancellation and await the barrier. For
a producing Rollout, it SHALL then use the retained live runtime owner to
request quiescence of both ordinary transitions and deferred runtime actions.
Quiescence SHALL cancel or drain every such producer and await a writer fence
proving that none can append another Rollout record before finalization appends
`process_exit`; no Rollout record may be appended after `process_exit`.
Finalization SHALL release the runtime-task owner and perform Process cleanup
only after `process_exit` is flushed. The barrier and its outcomes SHALL remain
internal, Process-local synchronization state and SHALL NOT create a durable
identity or terminal status model.

#### Scenario: Agent Executable completes with a terminal result
- **WHEN** an Agent Process publishes an `AgentExecutableResult` and exits
- **THEN** the ordinary run path transfers or retains the live runtime-task
  owner and Process cleanup guard in the terminal context instead of shutting
  down or dropping them
- **AND** its Rollout ends with a `process_exit` record carrying the Process
  exit code and that existing result
- **AND** the record is flushed before AgentFS runtime cleanup
- **AND** only then does finalization shut down and release the runtime task

#### Scenario: Generic Process control stops execution
- **WHEN** `cancel` or `interrupt` terminates the Process with exit code `130`
- **THEN** terminal finalization quiesces the Agent Machine and Rollout writer
- **AND** Alan Kernel waits for finalization before aborting the runner
- **AND** `process_exit` preserves the numeric code `130`
- **AND** it does not invent separate cancelled and interrupted states that
  the Kernel does not distinguish

#### Scenario: Startup fails after Rollout creation
- **WHEN** Agent Machine creation creates a producing Rollout and a later
  startup step fails before runtime readiness
- **THEN** the pending barrier has already resolved to the producing-Rollout
  context
- **AND** finalization appends and flushes `process_exit` for the startup
  failure

#### Scenario: Control arrives while terminal context is pending
- **WHEN** `/proc/<pid>/ctl` requests exit after Process commit but before
  Agent startup has reported whether it created a Rollout
- **THEN** terminal finalization requests startup cancellation and awaits the
  pending terminal-context barrier
- **AND** startup resolves the barrier with the producing-Rollout context if
  creation succeeded, or with an explicit no-producing-Rollout outcome if it
  did not
- **AND** finalization never infers no Rollout from a delivery race

#### Scenario: Active transition is cancelled
- **WHEN** control requests exit while the Agent Machine can still append
  Rollout records
- **THEN** finalization waits for the writer fence before appending
  `process_exit`
- **AND** `process_exit` is the final Rollout record

#### Scenario: Control exits during a deferred runtime action
- **WHEN** `/proc/<pid>/ctl` requests exit while a deferred runtime action is
  active
- **THEN** Agent Machine quiescence cancels or drains that deferred action
- **AND** finalization waits until the deferred action and its Rollout writer
  have crossed the writer fence
- **AND** Kernel runner abort cannot deadlock behind the fence
- **AND** `process_exit` is the final Rollout record

#### Scenario: Host failure prevents terminal recording
- **WHEN** an older Rollout or abrupt Host failure leaves no `process_exit`
  record
- **THEN** the Rollout remains readable as unterminated evidence
- **AND** Agent Runtime Service does not fabricate a terminal result

### Requirement: Rollout history follows the `/agent` namespace capability
A Process whose namespace includes readable `/agent` SHALL be able to read
`/agent/rollouts`. A Process without that mount SHALL have no Rollout-history
authority. The surface SHALL NOT introduce per-interaction ACLs, renderer-only
Host APIs, or raw System Store access, and its files SHALL remain read-only
even when `/agent` is mounted read-write.

#### Scenario: Agent Process receives the standard agent mount
- **WHEN** a Process namespace includes readable `/agent`
- **THEN** the Process can list and read `/agent/rollouts`
- **AND** it cannot mutate a retained Rollout

#### Scenario: Process has no agent mount
- **WHEN** a Process namespace does not include `/agent`
- **THEN** `/agent/rollouts` is unreachable
- **AND** no Host-private fallback grants access

### Requirement: Discovery follows Agent Runtime Service retention
`/agent/rollouts` SHALL expose every valid Rollout currently retained by Agent
Runtime Service. This change SHALL NOT add a retention duration, TTL, quota,
pin, archive, delete, or garbage-collection control. If a future owning-service
retention policy removes a Rollout, the discovery surface SHALL stop listing it
and SHALL NOT rely on renderer state to preserve a ghost entry.

#### Scenario: No Rollout retention policy is configured
- **WHEN** Agent Runtime Service retains a valid Rollout
- **THEN** `/agent/rollouts` exposes it across Process exit and Host restart
- **AND** the discovery contract makes no permanent-storage guarantee

#### Scenario: A future retention policy expires a Rollout
- **WHEN** Agent Runtime Service no longer retains a Rollout
- **THEN** `/agent/rollouts` no longer lists it
- **AND** renderer state is not authoritative for keeping the entry visible

### Requirement: Discovery includes active and unterminated Rollouts
`/agent/rollouts` SHALL include active, terminal, and valid unterminated
Rollouts. The presence of `process_exit` SHALL NOT be required for discovery.
A renderer MAY prioritize terminal Rollouts or label an unterminated Rollout
as unfinished, but SHALL NOT make its presentation state the discovery
authority.

#### Scenario: Active Rollout is retained
- **WHEN** an Agent Process has an active valid Rollout
- **THEN** the Rollout appears in `/agent/rollouts`
- **AND** `/agent/<pid>` remains the live operational Process view

#### Scenario: Unterminated Rollout survives a Host failure
- **WHEN** Agent Runtime Service retains a valid Rollout without `process_exit`
- **THEN** the Rollout remains discoverable as unfinished evidence
- **AND** the discovery surface does not hide or fabricate its terminal state

### Requirement: Discovery requires no notification protocol
Consumers SHALL be able to refresh `/agent/rollouts` by listing the directory
again. This change SHALL NOT require Agent Runtime Service to expose an event
file, watch stream, or subscription protocol.

#### Scenario: Consumer needs a current listing
- **WHEN** a consumer opens or reactivates its history view
- **THEN** it lists `/agent/rollouts` again
- **AND** discovery does not depend on a retained notification cursor or
  renderer-owned index

### Requirement: Malformed Rollouts are isolated during discovery
Agent Runtime Service SHALL apply the existing Rollout loader's validity rules
during discovery. A torn trailing record MAY be ignored while earlier complete
records remain discoverable. A Rollout with any other malformed record SHALL
be omitted with a diagnostic and SHALL NOT prevent valid Rollouts from being
listed. Discovery SHALL NOT delete or repair its backing file.

#### Scenario: Rollout has a torn trailing record
- **WHEN** a retained Rollout ends with an incomplete trailing JSON or UTF-8
  record
- **THEN** discovery accepts its earlier complete records
- **AND** the torn trailing record is not exposed as valid evidence

#### Scenario: One retained Rollout is malformed
- **WHEN** a retained Rollout contains an invalid non-torn record
- **THEN** Agent Runtime Service omits that Rollout and emits a diagnostic
- **AND** other valid Rollouts remain discoverable
- **AND** the malformed backing file is neither deleted nor repaired

### Requirement: Durable background launch is acknowledged by its Rollout
`SpawnRuntimeOverrides` SHALL accept an optional `durability_required` field.
When it is `true`, Service Manager SHALL apply the existing strict-durability
Agent Runtime setting and SHALL NOT fall back to an in-memory Agent Machine.
Before opening `/mnt/agent-runtime/clone`, a caller SHALL read
`/proc/host/boot_id` and list the currently discoverable `rollout_id` values.
After `/mnt/agent-runtime/clone` returns its ordinary Process PID and commits
an `AgentExecutableRequest`, the caller SHALL treat durable background launch
as accepted only after
`/agent/rollouts/<rollout-id>` exposes valid first-record `AgentMachineMeta`
whose ID was absent from the pre-spawn listing and whose `process_path` equals
`/proc/<pid>`, and after a fresh read confirms `/proc/host/boot_id` is
unchanged.

This acknowledgment SHALL NOT expose a Host rollout path, require internal
`RuntimeStartupMetadata`, or add a startup side API or duplicate AgentFS
metadata file.

#### Scenario: Strict-durability launch creates its Rollout
- **WHEN** a caller pins the current boot identity, lists current Rollout IDs,
  and `/mnt/agent-runtime/clone` allocates a PID for an
  `AgentExecutableRequest` whose `runtime_overrides.durability_required` is
  `true`
- **AND** Agent Runtime Service creates and flushes the producing Rollout
- **THEN** `/agent/rollouts/<rollout-id>` exposes a new ID with first-record
  metadata whose `process_path` identifies that PID
- **AND** `/proc/host/boot_id` still matches the pinned value
- **AND** the caller may acknowledge durable background launch

#### Scenario: Strict-durability launch cannot create a Rollout
- **WHEN** the Process exits before `/agent/rollouts` exposes a valid new
  Rollout absent from the pre-spawn listing whose `process_path` matches its
  PID
- **THEN** the caller reports launch failure
- **AND** no in-memory Agent Machine is accepted as durable background work

#### Scenario: Host restart reuses a prior PID
- **WHEN** the pre-spawn listing contains an older retained Rollout whose
  `process_path` matches the newly allocated PID
- **THEN** the caller excludes that Rollout because its ID was already present
- **AND** it acknowledges only a matching Rollout created after the listing

#### Scenario: Host restarts during launch correlation
- **WHEN** `/proc/host/boot_id` differs from the value pinned before
  `/mnt/agent-runtime/clone`
- **THEN** the caller rejects the launch acknowledgment
- **AND** it does not associate any Rollout from the new boot with the prior
  dispatch
