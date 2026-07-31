## Context

Agent Runtime Service already stores durable Rollout JSONL files in its System
Store subtree. Each Rollout has a `rollout_id`, producer Process path, start
metadata, and append-only Agent Machine evidence. The live `/agent/<pid>` view
is unbound when its Process is released, and renderer hosts are forbidden from
reading Host-private System Store paths.

The existing `/agent` mount is the Agent Runtime Service file tree. Its root
currently contains numeric live Agent Process entries and the `root` alias.

## Goals / Non-Goals

**Goals:**

- Keep retained Rollouts discoverable after Process exit and Alan OS Host
  restart.
- Persist terminal completion in the Rollout that owns the execution evidence.
- Expose Rollouts through a read-only Alan OS namespace surface.
- Preserve `/proc` as live Process lifecycle truth.

**Non-Goals:**

- Any additional durable execution or interaction identity.
- A second durable history record, database, or persistent index.
- Grouping multiple Rollouts into one logical user task.
- Renderer access to System Store backing.

## Decisions

### D1: History is a view, not an entity

The only durable entity on the discovery surface is the existing Rollout. A
history listing is reconstructed from retained Rollouts and has no identifier
or lifecycle of its own. A separate durable record is rejected because no
current behavior requires one.

### D2: Each retained Rollout is exposed as one JSONL file

Agent Runtime Service SHALL reserve `/agent/rollouts` and expose each retained
Rollout directly as a read-only JSONL file at
`/agent/rollouts/<rollout-id>`. The file carries the existing ordered Rollout
records; there is no directory wrapper or parallel `meta`, `status`, `result`,
or `evidence` projection. Numeric `/agent/<pid>` entries remain live Agent
Process views, and `/agent/root` remains the Root Agent Process alias.
`/srv/agent-runtime` remains only the service-handle rendezvous and does not
become a state tree.

Published Rollout backing is append-only: its owning writer and Host backing
adapter may append, but may never overwrite or truncate an existing published
prefix. On service startup, Agent Runtime Service validates each retained
source once and records its source identity plus approved complete-prefix
length in a rebuildable in-memory discovery table. For a live Rollout, the
owning writer advances that length under the discovery lock only after a
complete owned append passes envelope validation. This table is cache, not
durable authority or another execution identity.

Opening a Rollout file reserves its open slots and, under the discovery lock,
captures a pinned read-only source descriptor plus the current approved length
from that table. It stores only those two values in the fid and performs no
whole-prefix scan. Containment removes the entry under the same lock, so an
open either captures the pre-removal prefix or fails after removal. Failed open
or clunk releases its slots.

Each read fetches only the protocol-bounded requested range from the pinned
descriptor and never reads beyond the approved length. Its storage work and
scratch/result memory are proportional to that requested range, not to the
Rollout length. Bytes appended beyond the captured length are ignored, and an
unreadable descriptor returns an error. Reopening may capture a later validated
prefix. Agent Runtime Service issues
quota-scoped `/agent` FileServer handles for namespace assembly. Agent Runtime
Service binds an ordinary handle into every Process namespace, including Agent
Processes and the Local Entry Shell Process. The Host hands the authorized
renderer a separate attachment view over the Shell Process namespace which
overlays `/agent` with a reserved handle. Each handle has a fixed cap, and
inherited delegation shares that account rather than minting more capacity.
Ordinary handles draw from a fixed ordinary pool. Renderer attachment handles
draw from a separate reserved pool whose capacity exceeds one handle's cap;
ordinary handles cannot consume it. Open slots are reserved before the
discovery-table capture and released on failure or clunk.

Before allocating read scratch or result storage, every history read must
non-blockingly acquire both a per-handle in-flight read permit and a permit
from the handle's ordinary or renderer-reserved pool. If either permit is
unavailable, the read fails immediately with resource exhaustion rather than
queuing inside Agent Runtime Service. Both permits are released on success or
error. Fixed open-slot and read-permit totals therefore bound retained
descriptor memory, simultaneous scratch and result memory, and range-read
bandwidth even when tagged aP requests concurrently read one fid. Per-handle
limits prevent one holder from exhausting its corresponding pool.

This representation keeps every valid Rollout readable independently of its
size without a snapshot store, lease, generation, revocation protocol,
full-file buffer, or caller identity inside Agent Runtime Service. The quota
account belongs to the mounted capability handle and is not durable state.
Startup rebuild pays one O(prefix length) scan per retained source; active
writers validate new records once; opens are constant-work captures; and reads
are proportional to their requested ranges.

### D3: Retained Rollouts remain the only durable discovery source

Agent Runtime Service reconstructs `/agent/rollouts` by enumerating and
validating its retained Rollout backing at startup. The rebuildable in-memory
table used by listing and open records source identity and current approved
prefix length but is not persisted. If enumeration later becomes measurably
too expensive, a durable index may be added as a rebuildable cache, never as
authority.

### D4: Terminal completion belongs in the existing Rollout

For an Agent Process with a producing Rollout, clean completion SHALL attempt
to append and durably sync one `process_exit` record to that Rollout before
runtime cleanup. A successfully persisted record contains:

- the authoritative numeric `/proc` exit code;
- a completion timestamp; and
- the existing `AgentExecutableResult` when one was published.

No new terminal status enum is introduced. `AgentExecutableResult` already
owns `completed`, `paused`, and `failed`. Kernel control currently maps both
`cancel` and `interrupt` to exit code `130`, so the Rollout preserves `130`
without inventing a distinction. If Host failure prevents `process_exit` from
being appended, the Rollout remains valid unterminated evidence.

The current Kernel control path aborts the runner before Agent Runtime Service
can perform cleanup, so cleanup ordering alone cannot cover code `130`.
`ProcessRunner` therefore gains generic terminal-finalizer preparation and
execution hooks with default no-ops. Before the committed Process becomes
visible as running or accepts `/proc/<pid>/ctl`, Alan Kernel asks the runner to
prepare one per-Process finalizer from the committed `ProcessInvocation` and
retains it with the runner task. For runner completion, `/proc/<pid>/ctl`, and
Host `record_exit`, Alan Kernel serializes competing terminal paths and invokes
the prepared finalizer exactly once with the winning claim source and numeric
exit code. Only a runner-completion winner carries its `ProcessOutcome`;
control and Host winners carry none. Kernel waits for finalization before
publishing exit, and a control path aborts the runner only afterward.

These hooks contain no Agent, Rollout, or storage semantics. System Process
runner dispatches them to Agent Runtime Service only for `/bin/alan-agent`;
Agent Runtime Service owns the existing Rollout writer. System Process runner
derives the optional `AgentExecutableResult` only from a winning runner
outcome. Control and Host claims omit the losing candidate result, so code
`130` cannot be persisted beside a contradictory completed result. Claim source
is transition-local synchronization, not a durable lifecycle state. Kernel
still owns the one terminal transition and numeric exit code.

During generic finalizer preparation, System Process runner synchronously asks
Agent Runtime Service to install a pending terminal-context barrier and startup
cancellation path for that PID before Process control becomes reachable. The
Agent startup owner must resolve the barrier exactly once on every exit path:
with a producing-Rollout context, or with an explicit no-producing-Rollout
outcome. Either outcome carries any live runtime owner, charged
prepublication cleanup lease, and deferred cleanup for AgentFS already bound;
a pre-dispatch outcome carries none. A drop guard or equivalent total-exit
mechanism prevents channel closure from being misread as the latter.

Rollout creation starts at an internal quarantine/staging path, never at the
discoverable path. Before awaiting Host file creation, Agent Runtime Service
reserves a slot from a fixed service-wide staging-creation pool and registers
the intended path plus an independently cancellable cleanup lease. The Host
open completion is delivered to that lease even after Process startup is
cancelled. After creation, the writer owner enters the pending terminal context
before the initial `AgentMachineMeta` write.

Publication is one Host-backed durable-store barrier: write the complete
initial metadata, durably sync file data and metadata, atomically rename the
same inode into the discoverable subtree, then durably commit every affected
directory entry (or use a store transaction with equivalent crash semantics).
Only after that barrier succeeds may Agent Runtime Service insert the source
into its discovery table, resolve the producing-Rollout terminal context,
release the staging slot, or allow the Agent Machine to run a transition,
spawn a Tool, or cause another external side effect. `AsyncWriteExt::flush`
alone does not satisfy this barrier.

Cancellation before publication irrevocably switches the lease to reclaim-only
mode. A late Host open completion is closed and unlinked from staging; the slot
is released only after publication or successful unlink. A failed unlink keeps
the slot charged and emits a diagnostic, so repeated failures cannot create
unbounded same-boot staging work. Exhausting the fixed pool rejects later
creation with resource exhaustion. On startup, Agent Runtime Service sweeps all
abandoned staging entries before exposing discovery or clone capability. A
failed sweep prevents service readiness. No staging name or cleanup lease is a
Rollout identity or discoverable evidence.

Every prepublication completion, including a late durable-store step, rechecks
the lease's publish permission under the same lock that revokes it. Once
reclaim-only wins, no completion can insert the source into discovery or
complete durable publication.

Preparation first applies the same committed-namespace executable eligibility
check as `SystemProcessRunner::run`. An invocation whose `/bin/alan-agent`
image is not mounted keeps the generic no-op finalizer and may return exit
`127` without creating a barrier. Once preparation has registered a barrier,
System Process runner owns its resolution until Agent Runtime Service accepts
the invocation: every pre-dispatch return, including loss of the weak Agent
Runtime Service reference, explicitly resolves no producing Rollout. This
keeps the finalizer total even when dispatch is rejected before an Agent
startup owner exists.

Immediately after `initialize_agent_machine` succeeds, Agent Execution Engine
publishes the existing `RuntimeStartupMetadata` through an early
`RuntimeController` channel before initializing later UI surfaces or signaling
readiness. Agent Runtime Service receives it through that caller-owned
controller boundary. The Agent Runtime Service terminal context takes
ownership of the live `RuntimeController` (or an equivalent owning runtime-task
guard), a deferred AgentFS cleanup action, and the existing metadata rather
than retaining only a cloned `RuntimeHandle`; it then resolves the pending
barrier to the producing-Rollout context without waiting for
`wait_until_ready`. The ordinary Agent Executable run path may use a borrowed
handle to await readiness and produce its `ProcessOutcome`, but it must hand
off ownership immediately after constructing the controller, before awaiting
runtime readiness. Control may already be pending at that point and therefore
awaits the pre-registered barrier. The run path must not call
`RuntimeController::shutdown`, drop its runtime task owner, or perform AgentFS
cleanup before terminal finalization. If startup exits before creating a
Rollout, it resolves the barrier explicitly as no producing Rollout while
retaining any live runtime owner and deferred cleanup for AgentFS already
bound. This includes best-effort fallback to an in-memory Agent Machine after
Rollout creation fails. If a Rollout was created and a later startup step
fails, finalization can therefore still terminate that Rollout. If
`/bin/alan-agent` produces an `AgentExecutableResult`, it remains in the
candidate runner outcome and reaches finalization only if runner completion
wins the terminal claim.

The finalizer first signals startup or runtime cancellation and waits for the
terminal-context barrier. Whenever the outcome has a live runtime owner, it
requests Agent Machine quiescence. That operation cancels or drains both
ordinary transitions and deferred runtime actions. For a producing Rollout it
then completes a writer fence that covers every Rollout producer. Normal runner
completion does not shut down the controller before this step: it publishes
its result, returns its `ProcessOutcome`, and leaves the runtime task and
deferred cleanup action owned by the terminal context. The finalizer waits for
the writer fence, appends and flushes `process_exit` through the owning Rollout
writer, then shuts down and releases the runtime task. It returns the deferred
AgentFS cleanup action while consuming the terminal context exactly once.
Kernel publishes the terminal `/proc` state before invoking that action; only
then may Agent Runtime Service unbind `/agent/<pid>` and release Process-scoped
AgentFS backing. In particular, control immediately
after commit cannot outrun terminal registration, normal completion cannot
leave the finalizer with a stopped runtime, and control during a deferred
action cannot leave the finalizer waiting on a producer that never received
cancellation. This context is bounded to the live Process and is neither
exposed as a file or Host API nor promoted into another execution identity.

The writer fence orders producers; it does not make storage infallible, and it
cannot overtake an earlier writer operation stuck in storage I/O. One fixed
internal absolute deadline bounds Agent terminal finalization, but persistence
does not own that whole budget. A fixed final interval is reserved for
containment, so startup/context-barrier cancellation, quiescence, the writer
fence, the single terminal append-and-durable-sync attempt, and pre-exit
runtime shutdown share an earlier containment cutoff. The finalizer does not
retry because an ambiguous durable-sync result could otherwise duplicate
`process_exit`.

On an error or containment-cutoff expiry at any pre-exit stage, Agent Runtime
Service emits a structured diagnostic containing the PID, available Rollout
ID, intended exit code, failed stage, and error or timeout. It closes runtime
and writer admission and force-aborts their logical owners without awaiting
stuck work.

Containment then follows the terminal context's owned storage state:

- For a published producing Rollout, it removes the entry from discovery under
  the discovery-table lock used by open, then uses the reserved interval to
  atomically rename the current backing inode out of the discoverable subtree
  before returning control to Kernel.
- For an unpublished pending-open or staging outcome, it irrevocably revokes
  publication and transfers the charged cleanup lease to the bounded staging
  reaper. This is successful non-storage containment and does not wait for the
  Host open, prepublication I/O, or unlink before Kernel may publish exit.
- For an explicit no-producing-Rollout outcome with no creation lease or
  backing inode, closing admission and force-aborting any live runtime owner is
  successful non-storage containment. It neither removes a discovery entry nor
  attempts a rename. A pre-dispatch outcome with no runtime owner completes
  immediately.

Already-submitted published Rollout I/O retains only the quarantined inode and
may append only beyond its prior published prefix. Existing history fids retain
their pinned read-only descriptor and fixed prefix length, so those later
appends remain outside their readable range. Quarantine has no user-visible
identity or status and is not listed. Recovery waits until no writer owner
remains, revalidates the complete envelope, and may atomically republish the
same Rollout ID.

Only published-Rollout inode containment can invoke the fatal path. If that
operation errors or has not returned when the absolute deadline expires, Agent
Runtime Service reports a fatal storage-integrity failure through an injected
Alan OS Host lifecycle adapter and does not wait for the rename. The Host owner
atomically closes readiness, attachment
admission, and new-work admission, requests Service Manager shutdown, and
enters immediate fail-stop Host termination. The adapter call is synchronously
non-returning whether its internal shutdown signal succeeds or it must abort
the process; it never yields control back to Agent terminal finalization,
Kernel, or the caller. Kernel therefore cannot publish that Process exit and
the Host cannot continue with a discoverable stale writer. Thus the ordinary
successful-containment path can publish exit and later complete bounded AgentFS
cleanup, while containment failure has a Host-owned bounded fatal outcome
rather than an unbounded rename. This adds no second execution identity or
Host command surface.

### D5: Read authority follows the existing `/agent` capability

The history surface does not add per-interaction ACLs, renderer-only APIs, or
credential checks inside a shared FileServer handle. A Process whose namespace
contains readable `/agent` may read `/agent/rollouts`; a Process without that
mount cannot reach it. The Rollout files themselves reject writes even when
the enclosing `/agent` mount is read-write.

This matches current `/agent` visibility, where holders can already inspect
other live Agent Process entries. If Alan later requires confidentiality
between Agent Processes, that work must narrow `/agent` namespace delegation
as a whole rather than special-case only retained Rollouts.

### D6: Discovery follows owning-service retention

`/agent/rollouts` lists whatever valid Rollouts Agent Runtime Service currently
retains. This change does not promise permanent storage and does not add TTL,
quota, pin, archive, delete, or garbage-collection controls. Current product
behavior retains Rollouts indefinitely because no garbage-collection policy
exists.

A future Agent Runtime Service retention change may remove Rollouts, but it
must preserve existing evidence-retention guarantees and structured expiry
behavior. Renderers do not keep deleted Rollouts alive as ghost entries.

### D7: Discovery includes every valid retained Rollout

`/agent/rollouts` includes active, terminal, and valid unterminated Rollouts.
It is an evidence-discovery surface, not a completed-results index. A renderer
may prioritize Rollouts with `process_exit` and label those without it as
unfinished, but presentation filtering does not change discovery authority.

### D8: Discovery has no notification protocol

This change adds no event file, watch stream, or subscription protocol.
Consumers refresh `/agent/rollouts` when opening or reactivating a history
view and after lifecycle changes they already observe. Agent Runtime Service
may add native change notification later only if measured refresh behavior is
insufficient.

### D9: Discovery validates records and path-safe Rollout IDs

Discovery first uses the existing Rollout loader's record-validity rules and
then enforces the Rollout envelope required by existing writers and launch
correlation. A nonempty Rollout must contain exactly one `AgentMachineMeta`, it
must be the first complete record, and no later metadata record is permitted.
The ID and `process_path` used by discovery and launch correlation come only
from that leading record. An empty file, a non-metadata first record, missing
metadata, or repeated metadata excludes the Rollout. The envelope permits at
most one `process_exit`; when present it must be the final complete record and
no complete or torn record bytes may follow it. Conflicting terminal records or
any post-terminal bytes exclude the Rollout rather than forcing discovery to
choose an outcome. A torn trailing record is ignored only when the earlier
complete records satisfy this envelope and contain no `process_exit`. Any other
malformed record excludes that Rollout from `/agent/rollouts`.

The loader currently treats `rollout_id` as an unconstrained string, while the
discovery surface must use it as one child name. Discovery therefore adds only
the projection invariant required by that file surface: the ID must be
nonempty, must be neither `.` nor `..`, and must contain neither `/` nor NUL.
It must also be unique among the retained Rollouts in the same listing. If
multiple backing Rollouts claim one ID, discovery omits every colliding entry
rather than choosing an arbitrary authority or minting another identity.
Invalid and duplicate IDs emit diagnostics without blocking unrelated valid
entries. Discovery neither deletes nor repairs backing files. Internal
stale-writer quarantine is exceptional service-owned containment, not a
discoverable Rollout, status, or execution identity.

### D10: A producing Rollout is the durable launch acknowledgment

`SpawnRuntimeOverrides` gains the optional `durability_required` field and
Service Manager applies it to the existing Agent Runtime strict-durability
setting. Strict durability uses the publication barrier above: successful
file sync, atomic rename, and durable directory commit precede discovery,
producing-context resolution, Agent Machine transitions, and acknowledgment.

A renderer-attached Shell Process is not an Agent Process and therefore cannot
be the parent of `/bin/alan-agent` through its own `/proc/clone` view: Agent
Runtime Service has no parent runtime template to inherit from it. Agent
Runtime Service therefore exposes a dedicated clone-via-open launch capability
tree. The Local Entry Shell Process receives an ordinary `/agent` handle and
does not receive this launch tree. The Host hands the authorized renderer an
attachment view over that Process namespace which overlays `/agent` with the
reserved renderer handle and adds the launch tree at `/mnt/agent-runtime`.
This overlay does not alter the Shell Process namespace described by
`/proc/self/namespace`. Commands spawned by the Shell therefore inherit the
ordinary `/agent` handle and cannot inherit `/mnt/agent-runtime`. Service
Manager does not publish the launch tree in `/srv`, add it to `/agent`, or
retain it while assembling any child Process namespace. Reachability of
`/mnt/agent-runtime/clone` is the authority, so the aP FileServer needs no
caller identity and a restricted Process cannot acquire Root Agent
capabilities through this path.

Opening `/mnt/agent-runtime/clone` pins the current `/agent/root` Process as
parent, allocates the ordinary pending Process slot through `/proc/clone`, and
returns that PID. The caller writes one existing `AgentExecutableRequest`;
clunk commits the request. Agent Runtime Service derives the launch from the
Root Agent Process's registered runtime template and rejects the commit if that
parent is no longer current or the request would amplify its capabilities.
Agent Processes continue to launch their own children directly through their
Process-bound `/proc/clone`.

A caller requesting durable background work first reads
`/proc/host/boot_id` and lists the currently discoverable `rollout_id` values,
then opens `/mnt/agent-runtime/clone`, reads its allocated PID, writes a
request whose `runtime_overrides.durability_required` is true, and waits until
`/agent/rollouts/<rollout-id>` exposes valid first-record `AgentMachineMeta`
whose ID was absent from the pre-spawn listing and whose `process_path` is
`/proc/<pid>`. Before acknowledgment, it reads `/proc/host/boot_id` again and
requires the same value.

Agent Runtime Service completes the abandoned-staging sweep and prior-boot
quarantine recovery before exposing either `/agent/rollouts` or
`/mnt/agent-runtime/clone`. A failed staging sweep prevents readiness. A
Rollout quarantined during the current boot is not republished online; it waits
for the next service start. Recovery therefore never inserts a hidden Rollout
during a launch handshake. This exceptional delay is preferred to another
generation field or acknowledgment protocol.

That existing Rollout metadata is the file-visible acknowledgment: no Host
rollout path, internal `RuntimeStartupMetadata`, acknowledgment side API, or
duplicate AgentFS metadata file is exposed. A clone request rejected before
commit is a definite failure because no Process starts. Once commit succeeds,
the Process may execute; if the matching Rollout is not observed, the Process
exits first, or boot identity changes, the caller reports an indeterminate
launch outcome and MUST NOT automatically retry. The pre-spawn listing is
transition-local comparison state, not a durable index or identity; it prevents
a PID reused after Host restart from matching an older retained Rollout but
cannot prove non-execution after commit.

This acknowledgment guarantees that durable execution evidence has begun. It
does not promise that a later terminal append cannot fail. Only a successfully
discovered complete `process_exit` makes the completed outcome reconstructible
after Process exit or Host restart. A successful terminal durable sync makes
that record crash-stable. An ambiguous append or sync error does not override a
complete record later found by discovery; only a missing or torn terminal
record leaves the retained Rollout unterminated or incomplete.

## Risks / Trade-offs

- **Startup or listing cost grows linearly with retained Rollouts.** → Start
  with direct enumeration; add a rebuildable cache only after measurement.
- **Older or interrupted Rollouts may lack terminal completion.** → Preserve
  them as unterminated evidence; absence of `process_exit` is the fact and no
  successful result is fabricated.
- **Adding a reserved `/agent` child changes root dispatch.** → Reserve only
  the non-numeric name `rollouts`; keep PID entries numeric and test collision
  behavior. The launch capability stays under `/mnt`.
- **Every current `/agent` holder can read retained Rollouts.** → Treat this as
  the existing system-wide `/agent` capability boundary; address future
  inter-Agent confidentiality by narrowing the whole mount.
- **Current retention is unbounded.** → Keep this change policy-neutral and add
  an owning-service retention policy only when measured storage needs justify
  one.
- **Stuck Host creation or failed staging unlink can exhaust the fixed creation
  pool.** → Reject later creation instead of accumulating unbounded pending
  work or files; the completion reaper and startup sweep reclaim capacity.
- **`/mnt/agent-runtime/clone` allocates an ordinary `/proc` PID before
  runtime readiness.** → Correlate that PID to a newly discovered active
  Rollout's already-durable first record under one unchanged boot identity
  before acknowledging background dispatch. After commit, missing correlation
  is indeterminate and must not trigger automatic retry because execution may
  already have produced side effects.
