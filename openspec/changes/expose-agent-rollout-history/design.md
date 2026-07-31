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

### D3: Retained Rollouts remain the only durable discovery source

Agent Runtime Service reconstructs `/agent/rollouts` by enumerating and
validating its retained Rollout backing at startup. It does not persist a
parallel history index. If enumeration later becomes measurably too expensive,
an index may be added as a rebuildable cache, never as authority.

### D4: Terminal completion belongs in the existing Rollout

For an Agent Process with a producing Rollout, clean completion SHALL attempt
to append and flush one `process_exit` record to that Rollout before runtime
cleanup. A successfully persisted record contains:

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
outcome. A drop guard or equivalent total-exit mechanism prevents channel
closure from being misread as the latter.

Rollout creation transfers the backing inode and writer containment owner into
that pending context immediately after file creation and before the initial
`AgentMachineMeta` flush. Metadata and runtime ownership resolve later, but
control or deadline expiry during the first flush can already close and
quarantine the inode.

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
guard), the Process cleanup guard, and the existing metadata rather than
retaining only a cloned `RuntimeHandle`; it then resolves the pending barrier
to the producing-Rollout context without waiting for `wait_until_ready`. The
ordinary Agent Executable run path may use a borrowed handle to await readiness
and produce its `ProcessOutcome`, but it must hand off ownership immediately
after constructing the controller, before awaiting runtime readiness. Control
may already be pending at that point and therefore awaits the pre-registered
barrier. The run path must not call `RuntimeController::shutdown`, drop its
runtime task owner, or perform Process cleanup before terminal finalization. If
startup exits before creating a Rollout, it resolves the barrier explicitly as
no producing Rollout. If a Rollout was created and a later startup step fails,
finalization can therefore still terminate that Rollout. If `/bin/alan-agent`
produces an `AgentExecutableResult`, it remains in the candidate runner outcome
and reaches finalization only if runner completion wins the terminal claim.

The finalizer first signals startup or runtime cancellation and waits for the
terminal-context barrier. For a producing Rollout, it uses the retained live
runtime owner to request Agent Machine quiescence. That operation cancels or
drains both ordinary transitions and deferred runtime actions before
completing a writer fence that covers every Rollout producer. Normal runner
completion does not shut down the controller before this step: it publishes
its result, returns its `ProcessOutcome`, and leaves the runtime task and
cleanup guard owned by the terminal context. The finalizer waits for the
writer fence, appends and flushes `process_exit` through the owning Rollout
writer, then shuts down and releases the runtime task and performs Process
cleanup before consuming the terminal context exactly once. Only after that
may Kernel publish exit or abort the runner. In particular, control immediately
after commit cannot outrun terminal registration, normal completion cannot
leave the finalizer with a stopped runtime, and control during a deferred
action cannot leave the finalizer waiting on a producer that never received
cancellation. This context is bounded to the live Process and is neither
exposed as a file or Host API nor promoted into another execution identity.

The writer fence orders producers; it does not make storage infallible, and it
cannot overtake an earlier writer operation stuck in storage I/O. One fixed
internal deadline therefore bounds the entire Agent terminal finalization from
startup/context-barrier cancellation through quiescence, writer fence, and the
single terminal append-and-flush attempt. The finalizer does not retry because
an ambiguous flush result could otherwise duplicate `process_exit`. On an
append error, flush error, or deadline expiry at any stage, Agent Runtime
Service emits a structured diagnostic containing the PID, available Rollout
ID, intended exit code, failed stage, and storage error or timeout. It forcibly
aborts and closes the writer and runtime owners and atomically renames the
current backing inode out of the discoverable subtree before returning control
to Kernel. Already-submitted Host I/O retains only the quarantined inode.
Quarantine has no user-visible identity or status and is not listed. Recovery
waits until no writer owner remains, revalidates the complete envelope, and may
atomically republish the same Rollout ID. Failure to contain the inode is a
Host-fatal storage-integrity failure, not permission to publish exit with a
discoverable stale writer. This adds no second execution identity.

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
setting.

A renderer-attached Shell Process is not an Agent Process and therefore cannot
be the parent of `/bin/alan-agent` through its own `/proc/clone` view: Agent
Runtime Service has no parent runtime template to inherit from it. Agent
Runtime Service therefore exposes a dedicated clone-via-open launch capability
tree. Service Manager binds that tree only into the Local Entry Login Namespace
at `/mnt/agent-runtime`; it does not publish the tree in `/srv`, add it to
`/agent`, or retain it while assembling Agent Process namespaces. Reachability
of `/mnt/agent-runtime/clone` is the authority, so the aP FileServer needs no
caller identity and a restricted Agent Process cannot acquire Root Agent
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

Agent Runtime Service completes quarantine recovery before exposing either
`/agent/rollouts` or `/mnt/agent-runtime/clone`. It does not republish recovered
Rollouts while launch handshakes are possible, so a hidden prior-boot Rollout
cannot appear between the pre-spawn snapshot and acknowledgment.

That existing Rollout metadata is the file-visible acknowledgment: no Host
rollout path, internal `RuntimeStartupMetadata`, acknowledgment side API, or
duplicate AgentFS metadata file is exposed. If the Process exits before a
matching Rollout is discoverable, launch did not establish durable background
work. The pre-spawn listing is transition-local comparison state, not a
durable index or identity; it prevents a PID reused after Host restart from
matching an older retained Rollout. Revalidating the existing boot identity
rejects a Host restart during the handshake.

This acknowledgment guarantees that durable execution evidence has begun. It
does not promise that a later terminal append cannot fail. Only a successfully
discovered complete `process_exit` makes the completed outcome reconstructible
after Process exit or Host restart. An append or flush error does not override
a complete record that reached storage; only a missing or torn terminal record
leaves the retained Rollout unterminated or incomplete.

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
- **`/mnt/agent-runtime/clone` allocates an ordinary `/proc` PID before
  runtime readiness.** → Correlate that PID to a newly discovered active
  Rollout's already-durable first record under one unchanged boot identity
  before acknowledging background dispatch.
