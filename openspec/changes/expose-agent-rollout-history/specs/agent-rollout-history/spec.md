## ADDED Requirements

### Requirement: Retained Rollouts are discoverable in the agent namespace
Agent Runtime Service SHALL expose a read-only `/agent/rollouts` directory.
Each retained Rollout whose records and identifier pass discovery validation
SHALL be addressable at
`/agent/rollouts/<rollout-id>` as one JSONL file, where the path component is
the Rollout's existing identifier. The file SHALL expose the Rollout's ordered
records without a directory wrapper or parallel metadata projection. The
surface SHALL remain reconstructible after Agent Process exit and Alan OS Host
restart without exposing a raw System Store path.

Each successful open SHALL receive a service-owned immutable snapshot of the
validation-approved complete JSONL prefix. An open fid SHALL NOT retain or
forward reads to the Host backing inode. The snapshot SHALL remain unchanged
while the fid is open; reopening the path SHALL create a new snapshot and MAY
observe later complete records. Agent Runtime Service SHALL order snapshot
creation and removal from discovery under the same lock, without introducing a
lease, generation, or revocation protocol.

Snapshots with identical approved prefixes SHALL share one immutable buffer.
Agent Runtime Service SHALL enforce fixed limits for one snapshot's bytes,
total open history fids, and aggregate unique snapshot bytes. It SHALL reserve
count and byte budget before materialization, release them on clunk, reject an
open with resource exhaustion when any limit would be exceeded, and publish no
fid when materialization fails.

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

#### Scenario: Active Rollout grows after open
- **WHEN** an active Rollout appends a complete record after a consumer opens
  its history file
- **THEN** the existing fid remains an immutable snapshot
- **AND** reopening the path may expose the later complete record

#### Scenario: Retained snapshots reach a service limit
- **WHEN** another history open would exceed the per-snapshot byte, open-fid,
  or aggregate unique-snapshot-byte limit
- **THEN** Agent Runtime Service rejects that open with resource exhaustion
- **AND** existing fids and Rollout evidence remain unchanged
- **AND** clunk releases the corresponding count and byte budget

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
For an Agent Process with a producing Rollout, Alan SHALL attempt to append and
flush one `process_exit` record through the Process terminal finalization hook
before Alan Kernel publishes exit and before clean Agent Runtime Service
cleanup. On successful persistence, the record SHALL contain the authoritative
numeric Process exit code, a completion timestamp, and the existing
`AgentExecutableResult` when one is available. It SHALL NOT introduce a second
terminal status enum or fabricate a Rollout for a best-effort execution that
started without one.

Before an Agent Process becomes controllable, System Process runner SHALL
register with Agent Runtime Service a pending Process-local terminal-context
barrier and a startup cancellation path. Every Agent startup exit path SHALL
resolve the barrier exactly once with either a producing-Rollout context or an
explicit no-producing-Rollout outcome; an absent or dropped resolution SHALL
NOT be treated as no Rollout. Either outcome SHALL carry deferred cleanup for
any AgentFS already bound; a pre-dispatch outcome before Agent Runtime Service
accepts ownership MAY carry none.

Agent Runtime Service SHALL create a Rollout at an internal staging path and
register an independently cancellable creation owner before awaiting the Host
open. It SHALL register writer containment before the initial
`AgentMachineMeta` flush and SHALL atomically publish the inode into the
discoverable subtree only after that flush succeeds. Cancellation or deadline
expiry before publication SHALL leave no discoverable backing file.

System Process runner SHALL apply the same committed-namespace executable
eligibility check during terminal preparation as it applies before dispatch in
`run`. An invocation whose `/bin/alan-agent` executable is not mounted SHALL
retain the generic no-op finalizer and SHALL NOT register an Agent terminal
barrier. After a barrier is registered, every System Process runner return
before Agent Runtime Service accepts ownership SHALL explicitly resolve it as
no producing Rollout, including a return caused by an unavailable Agent Runtime
Service.

Immediately after Agent Machine creation succeeds, Agent Runtime Service SHALL
resolve the producing-Rollout context with the existing Rollout metadata and a
retained owning `RuntimeController` or equivalent runtime-task guard and
deferred AgentFS cleanup action, before later initialization or readiness
signaling. A cloned `RuntimeHandle` alone SHALL NOT satisfy this ownership
requirement. The ordinary Agent Executable run path MAY borrow the handle to
await and produce its `ProcessOutcome`, but SHALL NOT shut down or drop the
runtime-task owner or perform AgentFS cleanup before terminal finalization.
Terminal finalization SHALL first request startup or runtime cancellation and
await the barrier. For a producing Rollout, it SHALL then use the retained live
runtime owner to
request quiescence of both ordinary transitions and deferred runtime actions.
Quiescence SHALL cancel or drain every such producer and await a writer fence
proving that none can append another Rollout record before finalization appends
`process_exit`; no Rollout record may be appended after `process_exit`. One
fixed internal absolute deadline SHALL bound Agent terminal finalization and
SHALL reserve a fixed final interval for containment. Context-barrier wait,
quiescence, writer fence, the single terminal append-and-flush attempt, and
pre-exit runtime shutdown SHALL stop at the earlier containment cutoff. The
attempt SHALL NOT be retried after an ambiguous flush result.

On error or containment-cutoff expiry at any pre-exit stage, Agent Runtime
Service SHALL emit a structured diagnostic containing the PID, available
Rollout ID, intended exit code, failed stage, and failure; cancel the logical
writer and runtime owners without awaiting stuck Host I/O; remove the entry
from discovery under the snapshot-open lock; and atomically move the current
backing inode out of the discoverable subtree into internal quarantine during
the reserved interval. Only after successful containment SHALL it complete
non-blocking logical-owner release and release terminal finalization so Alan
Kernel can publish the authoritative Process exit. Already-submitted Host I/O
SHALL retain only the quarantined inode; every existing history fid SHALL
retain only its immutable pre-removal snapshot. Recovery SHALL wait until no
writer owner remains, validate the complete Rollout envelope, and MAY
atomically republish the same Rollout ID. Quarantine SHALL NOT be exposed as a
Rollout, status, or execution identity.

If containment returns an error or has not returned when the absolute deadline
expires, Agent Runtime Service SHALL signal the injected Alan OS Host lifecycle
adapter without awaiting the containment operation. The Host owner SHALL commit
the fatal storage-integrity transition or abort the Host process if the signal
cannot be accepted. Kernel SHALL NOT publish the Process exit or continue the
Host while a stale writer can mutate a discoverable Rollout. A complete valid
`process_exit` that reached the file SHALL remain authoritative even if the
append or flush result was ambiguous. If no complete terminal record is
discoverable, the Rollout SHALL remain unterminated or recoverably torn
evidence; failure SHALL NOT fabricate terminal evidence.
Finalization SHALL release the runtime-task owner only after `process_exit` is
flushed or this bounded persistence-failure path has cancelled the writer and
successfully contained its backing inode. It SHALL return the deferred AgentFS
cleanup action to Alan Kernel. Kernel SHALL publish the terminal `/proc` state
before invoking that action; only then may Agent Runtime Service unbind
`/agent/<pid>` and release Process-scoped AgentFS backing. The barrier, action,
and outcomes SHALL remain internal, Process-local synchronization state and
SHALL NOT create a durable identity or terminal status model.

#### Scenario: Agent Executable completes with a terminal result
- **WHEN** an Agent Process publishes an `AgentExecutableResult` and exits
- **THEN** the ordinary run path transfers or retains the live runtime-task
  owner and deferred AgentFS cleanup action in the terminal context instead of
  shutting down or dropping them
- **AND** its Rollout ends with a `process_exit` record carrying the Process
  exit code and that existing result
- **AND** the record is flushed before finalization shuts down and releases the
  runtime task
- **AND** `/proc/<pid>` publishes terminal state before the returned cleanup
  action unbinds AgentFS

#### Scenario: Control wins after a runner result is produced
- **WHEN** runner completion has a candidate `AgentExecutableResult` but
  control wins the serialized terminal claim with exit code `130`
- **THEN** finalization receives no losing runner outcome
- **AND** `process_exit` contains code `130` without the contradictory result

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

#### Scenario: Agent executable is rejected before service dispatch
- **WHEN** a committed invocation names `/bin/alan-agent` but its namespace
  does not contain that executable
- **THEN** terminal preparation uses the generic no-op instead of registering
  an Agent Runtime barrier
- **AND** System Process runner may publish exit `127` without finalization
  waiting for an Agent startup owner

#### Scenario: Agent Runtime becomes unavailable before dispatch
- **WHEN** terminal preparation registered an Agent barrier but System Process
  runner cannot upgrade the Agent Runtime Service reference
- **THEN** the pre-dispatch return explicitly resolves no producing Rollout
- **AND** terminal finalization completes without waiting on an orphaned
  barrier

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

#### Scenario: Terminal persistence cannot flush
- **WHEN** appending or flushing `process_exit` returns an I/O error or exceeds
  the containment cutoff that reserves time for containment
- **THEN** Agent Runtime Service stops the write attempt without retrying an
  ambiguous result and emits a structured diagnostic
- **AND** no writer appends another Rollout record
- **AND** it uses the reserved interval to quarantine the backing inode
- **AND** after successful containment, logical runtime ownership is released
  without awaiting stuck Host I/O
- **AND** Alan Kernel publishes the authoritative exit instead of leaving the
  Process running or blocking Host shutdown
- **AND** AgentFS cleanup begins only after that exit is published
- **AND** discovery treats a complete valid `process_exit` that reached the
  file as authoritative despite the ambiguous error
- **AND** only an absent or torn terminal record remains incomplete evidence

#### Scenario: An earlier writer blocks the terminal fence
- **WHEN** a prior Rollout write or flush remains stuck in storage I/O while
  terminal finalization waits for its writer fence
- **THEN** the containment cutoff expires while containment time remains
- **AND** Agent Runtime Service cancels the logical writer and runtime owners
  without awaiting the stuck Host I/O
- **AND** it atomically quarantines the backing inode before releasing Kernel
- **AND** stale Host I/O can modify only the quarantined inode
- **AND** Alan Kernel can publish exit and Host shutdown can progress

#### Scenario: Quarantine blocks in failing storage
- **WHEN** containment has not returned by the absolute finalization deadline
- **THEN** Agent Runtime Service signals the injected Host lifecycle adapter
  without awaiting the stuck quarantine operation
- **AND** the Host owner stops attachment and new-work admission and terminates
- **AND** an unavailable adapter causes fail-stop process abort, not continued
  operation
- **AND** Kernel does not publish the Process exit or continue the Host with a
  discoverable stale writer

#### Scenario: History fid remains open during quarantine
- **WHEN** a consumer opened a Rollout history file before terminal containment
- **THEN** containment removes the entry from discovery before releasing Kernel
- **AND** the existing fid continues reading only its immutable pre-removal
  snapshot
- **AND** stale Host I/O against the quarantined inode is not visible through
  that fid

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
during discovery and SHALL additionally validate the Rollout envelope. Every
discoverable Rollout SHALL contain exactly one `AgentMachineMeta`; it SHALL be
the first complete record, and no later record may be another
`AgentMachineMeta`. Discovery SHALL derive the entry ID and launch-correlation
`process_path` only from that leading record. An empty Rollout, a non-metadata
first record, absent metadata, or repeated metadata SHALL be omitted with a
diagnostic. A discoverable Rollout SHALL contain at most one `process_exit`; if
present, it SHALL be the final complete record and no later complete or torn
record bytes may follow it. Conflicting `process_exit` records or any
post-terminal bytes SHALL cause omission rather than outcome selection. A torn
trailing record MAY be ignored only when its earlier complete records satisfy
the envelope and contain no `process_exit`. A Rollout with any other malformed
record SHALL be omitted with a diagnostic and SHALL NOT prevent valid Rollouts
from being listed.

Because `rollout_id` becomes one file name, discovery SHALL additionally
require it to be nonempty, neither `.` nor `..`, and free of `/` and NUL. It
SHALL be unique among retained Rollouts in one listing. Discovery SHALL omit
every Rollout participating in an identifier collision rather than select one
or mint a replacement identity. Invalid or duplicate identifiers SHALL emit
diagnostics and SHALL NOT block unrelated valid Rollouts. Discovery SHALL NOT
delete or repair any backing file.

#### Scenario: Rollout has a torn trailing record
- **WHEN** a retained Rollout whose earlier complete records contain no
  `process_exit` ends with an incomplete trailing JSON or UTF-8 record
- **THEN** discovery accepts its earlier complete records
- **AND** the torn trailing record is not exposed as valid evidence

#### Scenario: Rollout metadata is absent or misordered
- **WHEN** a retained Rollout is empty, begins with a non-metadata record, or
  contains another `AgentMachineMeta` after its leading metadata
- **THEN** Agent Runtime Service omits it with a diagnostic
- **AND** discovery does not infer an identifier or `process_path` from a
  later or ambiguous record

#### Scenario: Rollout terminal records conflict or are not final
- **WHEN** a retained Rollout contains multiple `process_exit` records or any
  complete or torn record bytes after `process_exit`
- **THEN** Agent Runtime Service omits it with a diagnostic
- **AND** discovery does not choose between conflicting outcomes or expose
  post-terminal evidence

#### Scenario: One retained Rollout is malformed
- **WHEN** a retained Rollout contains an invalid non-torn record
- **THEN** Agent Runtime Service omits that Rollout and emits a diagnostic
- **AND** other valid Rollouts remain discoverable
- **AND** the malformed backing file is neither deleted nor repaired

#### Scenario: Rollout identifier is not a safe child name
- **WHEN** a retained Rollout has an empty identifier, `.` or `..`, or an
  identifier containing `/` or NUL
- **THEN** Agent Runtime Service omits it with a diagnostic
- **AND** the identifier cannot escape, alias, or add levels beneath
  `/agent/rollouts`

#### Scenario: Retained Rollouts claim the same identifier
- **WHEN** two or more retained Rollouts have the same otherwise-safe
  `rollout_id`
- **THEN** Agent Runtime Service omits every colliding Rollout with diagnostics
- **AND** it neither chooses an arbitrary backing file nor creates another
  identity
- **AND** unrelated valid Rollouts remain discoverable

### Requirement: Durable background launch is acknowledged by its Rollout
`SpawnRuntimeOverrides` SHALL accept an optional `durability_required` field.
When it is `true`, Service Manager SHALL apply the existing strict-durability
Agent Runtime setting and SHALL NOT fall back to an in-memory Agent Machine.
Strict durability SHALL guarantee creation of the producing Rollout, not
infallibility of later terminal persistence. A completed outcome SHALL be
reconstructible after Process exit or Host restart only when `process_exit`
is discovered as a complete valid record, including after an ambiguous flush
error; otherwise the retained Rollout SHALL remain unterminated or incomplete
evidence without a fabricated result.
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

Agent Runtime Service SHALL complete quarantine recovery before exposing
`/agent/rollouts` or `/mnt/agent-runtime/clone`. A Rollout quarantined during
the current boot SHALL remain hidden until the next service start; recovery
SHALL NOT republish Rollouts while launch handshakes are possible.

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

#### Scenario: Initial metadata flush stalls
- **WHEN** control or the terminal deadline fires after Rollout file creation
  but before the initial metadata flush completes
- **THEN** the pre-registered containment owner closes and quarantines the
  backing inode
- **AND** Kernel does not release a discoverable stale writer

#### Scenario: Backing-file creation outlives cancellation
- **WHEN** cancellation or the deadline fires while Host file creation is
  still pending and the open later completes
- **THEN** the file exists only under the internal staging path
- **AND** no empty orphan appears in the discoverable Rollout subtree

#### Scenario: Recovery precedes launch correlation
- **WHEN** Agent Runtime Service starts with quarantined Rollouts
- **THEN** it finishes recovery before exposing discovery or clone capability
- **AND** no recovered prior-boot Rollout can appear between a pre-spawn
  listing and launch acknowledgment
- **AND** current-boot quarantine waits for the next service start

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
