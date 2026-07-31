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

The Rollout owner and Host backing adapter SHALL enforce append-only published
backing: they MAY append but MUST NOT overwrite or truncate any byte in an
existing published prefix. At service startup, Agent Runtime Service SHALL
validate each retained source once and record its source identity and approved
complete-prefix length in a rebuildable in-memory discovery table. For a live
Rollout, its owning writer SHALL advance that length under the discovery lock
only after a complete owned append passes envelope validation. The table SHALL
NOT be durable authority or another execution identity.

The owning writer SHALL serialize append admission and track a poisoned state.
Before storage submission, it SHALL serialize each record through a capped sink
with a fixed service-owned `MAX_ROLLOUT_RECORD_BYTES`; exceeding that cap SHALL
reject the record without allocating or submitting the complete oversized
payload, atomically poison the writer, close Agent Machine effect admission,
and signal the Process-scoped runtime failure latch before the caller or writer
loop can continue. The cap SHALL apply to every record kind, including
`AgentMachineMeta` and `process_exit`. An over-cap evidence record MUST NOT be
logged and ignored while Tool or other external effects continue.

Before any failed, timed-out, or ambiguous append returns control to its caller
or the writer loop can accept another command, Agent Runtime Service SHALL
atomically poison the writer, close append admission, leave the approved length
unchanged, close Agent Machine transition and Tool admission, cancel in-flight
runtime actions, and signal the same failure latch. This applies even when the
failure may have occurred before writing bytes, because the Host result does
not authorize another append. No later ordinary record or `process_exit` SHALL
be submitted through a poisoned writer.

Before the Agent Process becomes controllable, the owning `RuntimeController`
SHALL provide one Process-scoped, single-assignment failure latch to both the
writer and Agent Executable run future. The run future SHALL select that latch
against readiness and normal completion. On writer poison it SHALL promptly
return the existing nonzero runner-failure outcome, causing Kernel's ordinary
runner-completion path to compete for the one Process terminal claim. A
simultaneous control or Host exit MAY win that same claim; no second Process
state or Agent-specific Kernel transition SHALL be introduced.

Storage containment SHALL be adopted by the winning terminal finalizer and
SHALL NOT independently report success, release the runtime owner, or leave the
Process running before a terminal claim exists. The finalizer SHALL fence any
still-running append before quarantine; containment failure SHALL use the
synchronously non-returning Host-fatal path. If the failure latch receiver has
already closed, Agent Runtime Service SHALL require proof that a terminal claim
already exists; otherwise it SHALL invoke the Host-fatal path rather than leave
a poisoned Process running. A complete record later found during recovery MAY
be validated as retained evidence, but no new bytes may be placed after a
possibly torn append.

Each successful open SHALL retain only a pinned read-only source descriptor and
the current approved length. It SHALL reserve its handle and pool open slots
and capture both values from the discovery table under the same lock used for
containment removal; it SHALL NOT rescan the complete prefix. Failed open or
clunk SHALL release the slots.
Agent Runtime Service SHALL enforce a fixed service-owned
`MAX_HISTORY_READ_BYTES` no greater than the aP wire payload limit on every
history read, including in-process calls. It SHALL reject a requested `count`
above that cap before permit acquisition, offset arithmetic, scratch or result
allocation, or storage I/O; it SHALL NOT rely on an importer or caller to
clamp the request. Accepted offset and count arithmetic SHALL be checked for
overflow. Each read SHALL fetch at most that accepted requested range from the
pinned descriptor and MUST NOT read beyond the approved length. The storage
adapter's returned data SHALL also be rejected if it exceeds the accepted
count. Storage work and scratch/result memory SHALL be proportional to the
capped requested range rather than the Rollout length. An unreadable
descriptor SHALL return an error. Appends beyond the captured length SHALL
remain invisible, while reopening MAY capture a later complete prefix.

Agent Runtime Service SHALL issue quota-scoped `/agent` FileServer handles for
namespace assembly. Each handle SHALL have a fixed open-history cap, and
inherited delegation SHALL share that account rather than minting more
capacity. Every Process namespace, including a Local Entry Shell Process
namespace, SHALL receive an ordinary handle backed by a fixed ordinary pool.
Only an authorized renderer attachment view over that Shell Process namespace
SHALL overlay `/agent` with a handle backed by a separate reserved pool whose
capacity exceeds one handle's cap. Ordinary handles SHALL NOT consume that
reserve. Agent Runtime Service SHALL reserve a handle and pool open slot before
capturing the discovery entry and release both on failure or clunk.

Before allocating scratch or result storage for a history read, Agent Runtime
Service SHALL non-blockingly acquire both a per-handle in-flight read permit
and a permit from that handle's ordinary or renderer-reserved pool. If either
permit is unavailable, the read SHALL fail immediately with resource
exhaustion and SHALL NOT queue inside the service. Both permits SHALL be
released on success or error. Fixed open-slot and read-permit totals SHALL
bound retained descriptor memory, concurrent scratch and result memory, and
range-read bandwidth, including concurrent tagged reads through
one fid. One holder SHALL NOT exhaust its corresponding pool.

Alan SHALL NOT impose a Rollout-size limit or retain a full-file buffer. This
representation SHALL keep all valid Rollouts readable with memory bounded
independently of Rollout size. Startup discovery SHALL scan each source in
fixed-size chunks through an incremental JSON/SAX validator. The validator
SHALL apply the existing loader's syntax and nesting limits, stream and discard
unbounded payload strings, retain only fixed envelope state and bounded
projection fields required for `rollout_id`, `process_path`, and terminal
classification, and record each complete line boundary as an approved prefix.
It SHALL NOT call a whole-file loader or retain a byte buffer, complete record,
or item vector proportional to the Rollout or record length.

`MAX_ROLLOUT_RECORD_BYTES` SHALL govern new writer admission only. Startup
discovery SHALL continue to accept an existing record larger than that cap
when the incremental validator proves it would have been valid under the
pre-cap loader contract. Required projected metadata fields SHALL retain their
existing safe-name and bounded service-field rules, but large message,
turn-context, prompt, result, or other evidence payloads SHALL NOT be omitted
merely because the new writer would reject creating them. Discovery SHALL NOT
rewrite the retained Rollout or hide it solely to migrate the cap.
This representation SHALL NOT introduce a history-read snapshot store, lease,
generation, revocation protocol, or caller identity inside Agent Runtime
Service. This constraint does not prohibit the bounded Process-local
publication generation required by terminal containment below. Quota accounts
SHALL belong to mounted capability handles and SHALL NOT be durable state.

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
- **AND** open captures the already-approved prefix without rescanning it

#### Scenario: Active Rollout grows after open
- **WHEN** an active Rollout appends a complete record after a consumer opens
  its history file
- **THEN** the existing fid continues to expose only its captured immutable
  complete prefix
- **AND** reopening the path may expose the later complete record

#### Scenario: Nonterminal append fails after a partial write
- **WHEN** an ordinary Rollout append partially reaches storage and then
  returns an error, timeout, or ambiguous result
- **THEN** the writer is poisoned and append admission closes before the writer
  loop can accept another command
- **AND** the approved length does not advance
- **AND** no later ordinary record or `process_exit` is appended after the
  possibly torn bytes
- **AND** the failure latch wakes the Agent Executable run future, whose
  nonzero return enters Kernel's ordinary terminal-claim path
- **AND** the winning finalizer adopts containment, fences the append owner, and
  quarantines the inode before releasing Kernel, or enters the non-returning
  Host-fatal path

#### Scenario: History opens reach the service limit
- **WHEN** another in-flight or retained history open would exceed its handle
  cap or backing pool
- **THEN** Agent Runtime Service rejects that open with resource exhaustion
- **AND** existing fids and Rollout evidence remain unchanged
- **AND** failure or clunk releases the reserved handle and pool slots

#### Scenario: Agent Process exhausts its history account
- **WHEN** one Agent Process retains its quota-scoped handle's maximum history
  fids
- **THEN** another open through that handle is rejected
- **AND** an authorized renderer attachment handle can still consume its
  reserved pool

#### Scenario: Concurrent reads reuse one fid
- **WHEN** tagged aP requests concurrently read one history fid beyond its
  handle or backing pool's in-flight read limit
- **THEN** excess reads fail immediately with resource exhaustion
- **AND** they do not queue or allocate scratch or result storage
- **AND** every acquired permit is released after either a successful or
  failed range read

#### Scenario: In-process caller requests an oversized history read
- **WHEN** an in-process namespace handle requests a history read whose count
  exceeds `MAX_HISTORY_READ_BYTES`, including `u32::MAX`
- **THEN** Agent Runtime Service rejects it before permit acquisition,
  allocation, offset arithmetic, or storage I/O
- **AND** it does not depend on `ImportedFileServer` to clamp the count

#### Scenario: History read reaches the service cap
- **WHEN** a caller requests exactly `MAX_HISTORY_READ_BYTES`
- **THEN** Agent Runtime Service may accept the read subject to permits,
  checked offset arithmetic, and the approved prefix length
- **AND** the returned data cannot exceed that cap or the accepted count

#### Scenario: Shell child inherits its Process namespace
- **WHEN** the Local Entry Shell launches an ordinary child from the namespace
  described by `/proc/self/namespace`
- **THEN** the child receives an ordinary `/agent` quota handle
- **AND** it does not inherit the authorized renderer attachment's reserved
  quota account

#### Scenario: Agent Process delegates its agent mount
- **WHEN** an Agent Process delegates the same `/agent` capability handle to a
  child
- **THEN** parent and child share one history quota account
- **AND** delegation does not multiply their open capacity

#### Scenario: A valid Rollout is larger than memory policy
- **WHEN** a valid retained Rollout is larger than any bounded read scratch
  buffer
- **THEN** startup validates its complete prefix with a fixed-chunk,
  incremental JSON/SAX scanner and the history fid exposes it over capped range
  reads
- **AND** validation memory is bounded by chunk, parser, and bounded projection
  state rather than record or file length
- **AND** Agent Runtime Service does not call the whole-file Rollout loader or
  retain the complete item vector

#### Scenario: A Rollout record exceeds the owning cap
- **WHEN** a writer attempts a record larger than
  `MAX_ROLLOUT_RECORD_BYTES`
- **THEN** capped serialization rejects it before storage submission without
  allocating the complete oversized payload
- **AND** it poisons the writer, closes Agent Machine effect admission, and
  signals the runtime failure latch before Tool or other external effects can
  continue
- **AND** the Agent Executable runner returns a nonzero failure so Kernel owns
  the terminal transition and finalizer-driven containment
- **AND** no later record or `process_exit` can create a terminal envelope that
  silently omits the rejected evidence

#### Scenario: A pre-cap Rollout contains a large valid record
- **WHEN** startup scans an existing loader-valid record larger than
  `MAX_ROLLOUT_RECORD_BYTES`
- **THEN** incremental validation streams its payload without retaining the
  complete record and preserves the Rollout in discovery
- **AND** the new writer cap does not retroactively invalidate, rewrite, or
  hide that evidence
- **AND** the total Rollout may remain unlimited through any number of valid
  records

#### Scenario: Consumer reads a large Rollout in small ranges
- **WHEN** a consumer reads an approved Rollout prefix through sequential small
  protocol-bounded ranges
- **THEN** each read fetches only its requested range
- **AND** Agent Runtime Service does not rescan or rehash the complete prefix
  for that read

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
durably sync one `process_exit` record through the Process terminal
finalization hook before Alan Kernel publishes exit and before clean Agent
Runtime Service cleanup. On successful persistence, the record SHALL contain
the authoritative numeric Process exit code, a completion timestamp, and the
existing `AgentExecutableResult` when one is available. Plain buffered or
async-writer flush SHALL NOT count as successful persistence. Alan SHALL NOT
introduce a second terminal status enum or fabricate a Rollout for a
best-effort execution that started without one.

Before an Agent Process becomes controllable, System Process runner SHALL
register with Agent Runtime Service a pending Process-local terminal-context
barrier and a startup cancellation path. Every Agent startup exit path SHALL
resolve the barrier exactly once with either a producing-Rollout context or an
explicit no-producing-Rollout outcome; an absent or dropped resolution SHALL
NOT be treated as no Rollout. Either outcome SHALL carry deferred cleanup for
any AgentFS already bound, an owning runtime guard for any live Agent Machine,
and any charged prepublication cleanup lease. A pre-dispatch outcome before
Agent Runtime Service accepts ownership MAY carry none.

Agent Runtime Service SHALL reserve a slot from a fixed service-wide
staging-creation pool, create a Rollout at an internal staging path, and
register an independently cancellable cleanup lease before awaiting the Host
open. Host open completion SHALL be delivered to that lease even after Process
startup cancellation. Agent Runtime Service SHALL register writer containment
before writing the initial `AgentMachineMeta`.

Initial publication SHALL write the complete metadata record, durably sync the
file data and metadata, atomically rename the same inode into the discoverable
subtree, and durably commit every affected directory entry, or use a durable
store transaction with equivalent crash semantics. Plain buffered or
async-writer flush SHALL NOT satisfy this barrier. Only after the complete
barrier succeeds SHALL Agent Runtime Service insert the source into its
discovery table, resolve a producing-Rollout terminal context, or permit the
Agent Machine to run a transition, spawn a Tool, or cause another external side
effect. The staging slot SHALL be released only after this barrier succeeds,
the staging inode is successfully unlinked, or destination-claimed containment
successfully fences its owner and durably quarantines or removes every possible
destination and stale staging alias.

Before issuing the publication rename, Agent Runtime Service SHALL atomically
claim a non-cancellable publication critical section under the same lock used
for cancellation and capture its current transition-local publication
generation. Cancellation or deadline expiry that wins before this claim SHALL
irrevocably revoke publication and switch the lease to staging reclaim-only
mode. Once the claim wins, ordinary cancellation SHALL remain pending and MUST
NOT reclassify the inode as staging or interrupt rename and directory commit.
After the barrier succeeds, Agent Runtime Service SHALL revalidate the
generation under that lock, resolve the producing-Rollout context, and only
then service pending ordinary cancellation; no Agent Machine side effect may
occur in between.

A late-created staging file SHALL be closed and unlinked by the service reaper.
The staging slot SHALL remain charged until successful publication, staging
unlink, or destination-claimed containment; a failed unlink or containment
SHALL emit a diagnostic and MUST NOT release the slot. Exhausting the pool
SHALL reject later creation with resource exhaustion. Before exposing
`/agent/rollouts` or `/mnt/agent-runtime/clone` at startup, Agent Runtime
Service SHALL sweep abandoned staging entries. Sweep failure SHALL prevent
service readiness. Staging entries and cleanup leases SHALL NOT be
discoverable evidence or durable execution identities.

Every completion before the publication claim, including a late Host open,
write, or file-sync result, SHALL recheck publish permission under the same
lock used to revoke it. After staging reclaim-only mode wins, no operation
SHALL issue the publication rename. Once rename has been issued or its result
is ambiguous, the lease SHALL enter destination-claimed state. A failed,
timed-out, or ambiguous post-rename barrier SHALL synchronously exclude the
destination from discovery and durably quarantine or remove every possible
destination and stale staging alias before releasing the slot or terminal
finalization. This SHALL use the published-storage containment and Host-fatal
failure rules, not staging reclaim.

When terminal containment must supersede a publication critical section, Agent
Runtime Service SHALL atomically advance the transition-local publication
generation, close publication and Agent Machine effect admission, and resolve
the pending terminal barrier to a destination-claimed non-producing outcome
that owns the publication task and cleanup lease. Every Host completion SHALL
revalidate its captured generation before resolving a producing context,
inserting discovery, releasing the slot, or enabling Agent Machine work. A
generation mismatch SHALL suppress all of those actions and only signal that
the superseded publication owner has ended.

Destination-claimed containment SHALL obtain that publication-owner fence
before it inspects and quarantines possible destination and staging names. It
SHALL NOT report containment success or release Kernel while an older Host
operation can still create or recreate the destination. If the fence,
quarantine, or durable directory commit cannot complete within the absolute
terminal deadline, containment SHALL invoke the synchronously non-returning
Host-fatal path. The generation and fence SHALL be bounded Process-local
synchronization, not a durable identity or lifecycle model.

After a Host restart, a complete valid final-name entry that survived the
atomic rename SHALL be recovered as a committed retained Rollout; any duplicate
staging alias to that inode SHALL be durably removed before readiness. An
invalid or torn final-name entry SHALL be quarantined before readiness. This
recovery rule is safe because cancellation cannot win after the publication
claim. A recovered committed Rollout MAY be unterminated evidence of the Host
interruption, but a cancelled pre-claim staging entry MUST NOT become
discoverable.

System Process runner SHALL apply the same committed-namespace executable
eligibility check during terminal preparation as it applies before dispatch in
`run`. An invocation whose `/bin/alan-agent` executable is not mounted SHALL
retain the generic no-op finalizer and SHALL NOT register an Agent terminal
barrier. After a barrier is registered, every System Process runner return
before Agent Runtime Service accepts ownership SHALL explicitly resolve it as
no producing Rollout, including a return caused by an unavailable Agent Runtime
Service.

Immediately after Agent Machine creation succeeds, Agent Runtime Service SHALL
resolve the terminal context with any existing Rollout metadata, a retained
owning `RuntimeController` or equivalent runtime-task guard, and the deferred
AgentFS cleanup action, before later initialization or readiness signaling. A
no-producing-Rollout outcome with a live in-memory Agent Machine SHALL retain
the runtime owner too. A cloned `RuntimeHandle` alone SHALL NOT satisfy this
ownership requirement. The ordinary Agent Executable run path MAY borrow the
handle to await and produce its `ProcessOutcome`, but SHALL NOT shut down or
drop the runtime-task owner or perform AgentFS cleanup before terminal
finalization.

Terminal finalization SHALL first request startup or runtime cancellation and
await the barrier. Whenever the outcome has a live runtime owner, it SHALL
request quiescence of both ordinary transitions and deferred runtime actions.
Quiescence SHALL cancel or drain every such producer. For a producing Rollout,
it SHALL then await a writer fence proving that none can append another Rollout
record before finalization appends `process_exit`; no Rollout record may be
appended after `process_exit`. If the writer is poisoned, finalization SHALL
NOT attempt `process_exit`; it SHALL proceed directly to published-storage
containment while preserving the original append failure in its diagnostic.
One
fixed internal absolute deadline SHALL bound Agent terminal finalization and
SHALL reserve a fixed final interval for containment. Context-barrier wait,
quiescence, writer fence, the single terminal append-and-durable-sync attempt,
and pre-exit runtime shutdown SHALL stop at the earlier containment cutoff.
The attempt SHALL NOT be retried after an ambiguous durable-sync result.

For a clean no-producing-Rollout outcome, successful quiescence and shutdown of
any live runtime owner SHALL be a normal non-storage completion. If that
outcome owns a pending-open or staging cleanup lease, Agent Runtime Service
SHALL first revoke publication and transfer the charged lease to the bounded
service reaper; it SHALL NOT wait for Host open, prepublication I/O, or unlink.
If it owns no creation lease or backing inode, no storage action is required.
After this disposition succeeds, finalization SHALL release the runtime owner
and may return the deferred AgentFS cleanup action so Kernel can publish exit.
It SHALL NOT require a `process_exit`, fabricate a Rollout, or route clean
no-Rollout completion through an error-only containment branch.

On error or containment-cutoff expiry at any pre-exit stage, Agent Runtime
Service SHALL emit a structured diagnostic containing the PID, available
Rollout ID, intended exit code, failed stage, and failure; close runtime and
writer admission; and force-abort their logical owners without awaiting stuck
work.

For a published producing Rollout or destination-claimed publication, Agent
Runtime Service SHALL remove or exclude the entry from discovery under the
discovery-table lock used by open. For destination-claimed publication it SHALL
first fence the superseded publication owner. It SHALL then atomically move
every possible current backing inode out of the discoverable subtree into
internal quarantine during the reserved interval, durably commit the affected
directory entries, and remove any stale staging alias before reporting
containment success. For an unpublished pending-open or staging outcome that
never claimed publication,
it SHALL revoke publication and transfer the charged cleanup lease to the
bounded service reaper; this SHALL be successful non-storage containment
without awaiting Host open, prepublication I/O, or unlink. For an explicit
no-producing-Rollout outcome with no creation lease or backing inode, closing
admission and force-aborting any live runtime owner SHALL be successful
non-storage containment; Agent Runtime Service SHALL NOT remove a discovery
entry or attempt an inode rename. A pre-dispatch outcome with no live owner
SHALL complete immediately.

Only after the applicable containment branch succeeds SHALL Agent Runtime
Service complete non-blocking logical-owner release and release terminal
finalization so Alan Kernel can publish the authoritative Process exit.
Already-submitted published Rollout I/O SHALL retain only the quarantined inode
and MUST NOT overwrite or truncate its prior published prefix. Every existing
history fid SHALL retain its pinned read-only descriptor and SHALL NOT read
beyond its fixed prefix length.
Recovery SHALL wait until no writer owner remains, validate the complete
Rollout envelope, and MAY atomically republish the same Rollout ID. Quarantine
SHALL NOT be exposed as a Rollout, status, or execution identity.

Only published-Rollout or destination-claimed inode containment SHALL use the
fatal path. If that containment returns an error or has not returned when the
absolute deadline expires, Agent Runtime Service SHALL signal the injected
Alan OS Host lifecycle adapter without awaiting the containment operation.
That Host-owned call SHALL be synchronously non-returning whether it enters
normal fail-stop termination or aborts after internal signaling failure. Agent
terminal finalization SHALL never return to Kernel on this path, so Kernel
SHALL NOT publish the Process exit or continue the Host while a stale writer
can mutate a discoverable Rollout. A complete valid `process_exit` that reached
the file SHALL remain authoritative even if the append or durable-sync result
was ambiguous. If no complete terminal record is discoverable, the Rollout
SHALL remain unterminated or
recoverably torn evidence; failure SHALL NOT fabricate terminal evidence.
Finalization SHALL release the runtime-task owner only after `process_exit` is
durably synced, clean no-producing-Rollout non-storage completion succeeds, or
the applicable error-containment branch succeeds.
It SHALL return the deferred AgentFS cleanup action to Alan Kernel. Kernel
SHALL publish the terminal `/proc` state before invoking that action; only then
may Agent Runtime Service unbind
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
- **AND** the record is durably synced before finalization shuts down and
  releases the runtime task
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

#### Scenario: Best-effort fallback starts an in-memory Agent Machine
- **WHEN** Rollout creation fails but best-effort policy starts a live in-memory
  Agent Machine and later completes cleanly
- **THEN** the no-producing-Rollout outcome retains the owning runtime guard
- **AND** terminal finalization quiesces and shuts down that runtime as a
  successful non-storage completion
- **AND** it revokes and transfers any charged pending-open or staging cleanup
  lease to the bounded reaper before releasing the runtime owner
- **AND** it returns deferred AgentFS cleanup so Kernel can publish Process
  exit
- **AND** no terminal Rollout record is fabricated

#### Scenario: In-memory Agent Machine misses the containment cutoff
- **WHEN** an explicit no-producing-Rollout outcome has a live runtime owner
  whose quiescence or shutdown reaches the containment cutoff
- **THEN** Agent Runtime Service closes runtime work admission and force-aborts
  the runtime owner
- **AND** it completes successful non-storage containment without looking up a
  discovery entry or renaming an inode
- **AND** Alan Kernel may publish the controlled Process exit without invoking
  the Host-fatal storage path

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
- **AND** its no-owner outcome performs no storage containment operation

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

#### Scenario: Terminal persistence cannot durably sync
- **WHEN** appending or durably syncing `process_exit` returns an I/O error or
  exceeds the containment cutoff that reserves time for containment
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
- **WHEN** a prior Rollout write or durable sync remains stuck in storage I/O while
  terminal finalization waits for its writer fence
- **THEN** the containment cutoff expires while containment time remains
- **AND** Agent Runtime Service cancels the logical writer and runtime owners
  without awaiting the stuck Host I/O
- **AND** it atomically quarantines the backing inode before releasing Kernel
- **AND** stale Host I/O can only append to the quarantined inode and cannot
  overwrite or truncate its prior published prefix
- **AND** Alan Kernel can publish exit and Host shutdown can progress

#### Scenario: Quarantine blocks in failing storage
- **WHEN** containment of a published Rollout inode has not returned by the
  absolute finalization deadline
- **THEN** Agent Runtime Service signals the injected Host lifecycle adapter
  without awaiting the stuck quarantine operation
- **AND** the Host owner stops attachment and new-work admission and enters
  fail-stop termination
- **AND** the adapter call never returns to Agent terminal finalization
- **AND** internal signaling failure aborts the Host process without returning
- **AND** Kernel does not publish the Process exit or continue the Host with a
  discoverable stale writer

#### Scenario: History fid remains open during quarantine
- **WHEN** a consumer opened a Rollout history file before terminal containment
- **THEN** containment removes the entry from discovery before releasing Kernel
- **AND** the existing fid ignores bytes beyond its captured prefix length
- **AND** stale Host I/O cannot overwrite or truncate that immutable prefix
- **AND** each read fetches only its requested range from the pinned descriptor

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

Because `rollout_id` becomes one file name and directory listings delimit names
with line endings, discovery SHALL additionally require it to be nonempty,
neither `.` nor `..`, and free of `/`, NUL, carriage return, and line feed. It
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
  identifier containing `/`, NUL, carriage return, or line feed
- **THEN** Agent Runtime Service omits it with a diagnostic
- **AND** the identifier cannot escape, alias, add levels beneath
  `/agent/rollouts`, or split a directory listing into apparent child names

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
Strict durability SHALL guarantee crash-stable publication of the producing
Rollout: file data and metadata sync, atomic rename, and durable directory
commit SHALL all succeed before discovery or acknowledgment. The Agent Machine
MUST NOT run a transition, spawn a Tool, or cause another external side effect
before that barrier succeeds. Strict durability SHALL NOT make later terminal
persistence infallible. A completed outcome SHALL be reconstructible after
Process exit or Host restart only when `process_exit` is discovered as a
complete valid record, including after an ambiguous append or durable-sync
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

A request rejected before commit SHALL be reported as a definite failure
because Agent Runtime Service has not started the Process. Once commit succeeds
or its result becomes ambiguous, the Process may execute. If the caller cannot
observe the matching Rollout, observes Process exit first, loses the
attachment, reaches its correlation deadline, or observes a changed boot
identity, it SHALL report the launch outcome as indeterminate and MUST NOT
automatically retry. Missing correlation after commit SHALL NOT be interpreted
as proof that no Tool or other external side effect occurred.

This acknowledgment SHALL NOT expose a Host rollout path, require internal
`RuntimeStartupMetadata`, or add a startup side API or duplicate AgentFS
metadata file.

Agent Runtime Service SHALL complete the abandoned-staging sweep and quarantine
recovery before exposing `/agent/rollouts` or `/mnt/agent-runtime/clone`.
Recovery SHALL reconcile duplicate staging aliases, complete valid final
entries that survived publication, and invalid or torn final entries according
to the publication recovery rules above. Sweep or recovery failure SHALL
prevent readiness. A Rollout quarantined during the current boot SHALL remain
hidden until the next service start; recovery SHALL NOT republish Rollouts
while launch handshakes are possible.

#### Scenario: Strict-durability launch creates its Rollout
- **WHEN** a caller pins the current boot identity, lists current Rollout IDs,
  and `/mnt/agent-runtime/clone` allocates a PID for an
  `AgentExecutableRequest` whose `runtime_overrides.durability_required` is
  `true`
- **AND** Agent Runtime Service completes the producing Rollout's file sync,
  atomic publication rename, and durable directory commit
- **THEN** `/agent/rollouts/<rollout-id>` exposes a new ID with first-record
  metadata whose `process_path` identifies that PID
- **AND** `/proc/host/boot_id` still matches the pinned value
- **AND** the caller may acknowledge durable background launch

#### Scenario: Power fails after durable launch acknowledgment
- **WHEN** the Host loses power immediately after the caller acknowledges a
  strict-durability launch
- **THEN** the producing Rollout's initial metadata and discoverable directory
  entry survive restart
- **AND** Agent Runtime Service reconstructs the same Rollout from System Store
  backing

#### Scenario: Cancellation wins before publication claim
- **WHEN** control or the terminal deadline fires after Rollout file creation
  but before the publisher claims its non-cancellable publication critical
  section
- **THEN** Agent Runtime Service irrevocably revokes publication and transfers
  the charged staging cleanup lease to its reaper
- **AND** a late Host open, write, or file-sync result cannot issue the
  publication rename or permit Agent Machine side effects
- **AND** Kernel may publish exit because the inode was never discoverable

#### Scenario: Cancellation arrives after publication rename
- **WHEN** publication has claimed its critical section and issued rename but
  cancellation arrives before durable directory commit completes
- **THEN** cancellation remains pending while the publication barrier resolves
- **AND** a successful barrier resolves the producing-Rollout context before
  cancellation is serviced, without an intervening Agent Machine side effect
- **AND** if terminal containment supersedes the stalled barrier, it advances
  the publication generation and every late completion suppresses producing-
  context resolution, discovery insertion, slot release, and Agent Machine
  effects
- **AND** containment fences the superseded publication owner before it
  excludes and durably quarantines every possible destination plus stale
  staging aliases
- **AND** successful durable destination containment releases the charged
  staging slot
- **AND** failure of that containment invokes the synchronously non-returning
  Host-fatal path

#### Scenario: Publication owner outlives the containment cutoff
- **WHEN** terminal containment cannot fence a superseded publication owner
  before the absolute deadline
- **THEN** it does not report successful quarantine or release Kernel
- **AND** it invokes the synchronously non-returning Host-fatal path
- **AND** a late publication completion cannot reinsert discovery or start
  Agent Machine work after Process exit

#### Scenario: Host restarts during the publication critical section
- **WHEN** the Host restarts after rename may have occurred but before the
  running service observes durable directory-commit success
- **THEN** startup recovery treats a complete valid surviving final entry as a
  committed retained Rollout and durably removes any duplicate staging alias
- **AND** it quarantines an invalid or torn final entry before readiness
- **AND** the recovered valid Rollout may appear as unterminated evidence of
  the Host interruption

#### Scenario: Backing-file creation outlives cancellation
- **WHEN** cancellation or the deadline fires while Host file creation is
  still pending and the open later completes
- **THEN** the reclaim-only completion owner closes and unlinks the staging file
- **AND** it releases the staging slot only after successful unlink
- **AND** no empty orphan appears in the discoverable Rollout subtree

#### Scenario: Staging reclamation cannot complete
- **WHEN** a late staging unlink fails
- **THEN** Agent Runtime Service emits a diagnostic and keeps its fixed-pool
  slot charged
- **AND** additional creation is rejected with resource exhaustion when the
  pool is full
- **AND** the next service startup sweeps the abandoned staging entry before
  exposing discovery or clone capability

#### Scenario: Startup staging sweep fails
- **WHEN** Agent Runtime Service cannot remove an abandoned staging entry
  during startup
- **THEN** it does not expose `/agent/rollouts` or
  `/mnt/agent-runtime/clone`
- **AND** another boot cannot accumulate new staging entries through this
  service instance

#### Scenario: Recovery precedes launch correlation
- **WHEN** Agent Runtime Service starts with quarantined Rollouts or abandoned
  staging entries
- **THEN** it finishes quarantine recovery and the staging sweep before
  exposing discovery or clone capability
- **AND** no recovered prior-boot Rollout can appear between a pre-spawn
  listing and launch acknowledgment
- **AND** current-boot quarantine waits for the next service start

#### Scenario: Launch is rejected before commit
- **WHEN** Agent Runtime Service rejects the clone request before committing
  the pending Process
- **THEN** the caller reports a definite launch failure
- **AND** no Agent Process begins execution

#### Scenario: Correlation is missing after commit
- **WHEN** commit succeeded or was ambiguous but the caller does not observe a
  valid new Rollout absent from the pre-spawn listing whose `process_path`
  matches the allocated PID
- **THEN** the caller reports an indeterminate launch outcome
- **AND** it does not automatically retry the request
- **AND** Agent Runtime Service still does not accept an in-memory Agent
  Machine as durable background work

#### Scenario: Host restart reuses a prior PID
- **WHEN** the pre-spawn listing contains an older retained Rollout whose
  `process_path` matches the newly allocated PID
- **THEN** the caller excludes that Rollout because its ID was already present
- **AND** it acknowledges only a matching Rollout created after the listing

#### Scenario: Host restarts during launch correlation
- **WHEN** `/proc/host/boot_id` differs from the value pinned before
  `/mnt/agent-runtime/clone`
- **THEN** the caller reports the committed launch outcome as indeterminate
- **AND** it does not associate any Rollout from the new boot with the prior
  dispatch
- **AND** it does not automatically retry the request
