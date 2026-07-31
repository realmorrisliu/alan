## Why

Durable Rollouts survive Agent Process exit, but authorized Alan OS consumers
cannot discover them without scanning Agent Runtime Service System Store
backing. That prevents reliable review after Process exit or Alan OS Host
restart even though the execution evidence already exists.

## What Changes

- Attempt to append and durably sync one `process_exit` record to the existing
  Rollout before clean runtime shutdown and Process exit publication. On
  success it carries the authoritative numeric exit code and, when available,
  the existing `AgentExecutableResult`; AgentFS unbinding follows exit
  publication, and no second terminal status model is introduced.
- Bound Agent terminal finalization with one internal absolute deadline while
  reserving its final interval for containment. Context-barrier, quiescence,
  writer-fence, terminal-persistence, and pre-exit runtime-shutdown work stops
  at the earlier containment cutoff. On error or timeout, cancel logical owners
  without awaiting stuck Host I/O. A published Rollout requires atomic inode
  quarantine in the reserved interval; failure invokes the synchronously
  non-returning Alan OS Host lifecycle adapter. An explicit no-Rollout outcome
  instead force-aborts its runtime owner and completes without a storage
  operation. An unpublished staging creation revokes publication and transfers
  its cleanup lease to a bounded service reaper. Agent Runtime Service does not
  own whole-Host shutdown. Recovery may republish only after ownership ends and
  validation succeeds.
- Treat clean no-Rollout completion as an ordinary non-storage disposition:
  quiesce and shut down its runtime owner, transfer any charged pending-open or
  staging lease to the bounded reaper, and return deferred AgentFS cleanup so
  Kernel can publish exit without fabricating terminal evidence.
- Extend the generic Process runner bridge with a prepared terminal
  finalization hook. Alan Kernel asks the runner to prepare the per-Process
  hook before the committed Process becomes controllable, invokes it exactly
  once with the winning terminal claim before any transition is published, and
  awaits it before aborting the runner. Only runner completion carries its
  Process outcome; control and Host winners carry none. Other Process images
  use the default no-op.
- For `/bin/alan-agent`, prepare a pending Process-local terminal-context
  barrier before control becomes reachable. Resolve it on every startup path
  with either the existing Rollout metadata plus ownership of the runtime task
  and deferred AgentFS cleanup, or an explicit no-producing-Rollout outcome
  that still carries any live runtime owner, charged prepublication cleanup
  lease, and cleanup for AgentFS already bound. The ordinary run path hands
  that ownership to terminal finalization instead of shutting down or dropping
  it. Terminal finalization cancels
  startup or execution, awaits that barrier, quiesces every live Agent Machine,
  and, when a Rollout exists, fences its writers before appending
  `process_exit`. Thus no live runtime leaks, no producing Rollout is missed,
  and no later record can follow the terminal record. Alan Kernel publishes
  exit before invoking the returned AgentFS cleanup action.
- Apply the same executable-eligibility check during terminal preparation as
  during System Process dispatch. If any pre-dispatch path returns after a
  barrier was registered, resolve it explicitly as no producing Rollout so an
  exit such as missing executable or unavailable Agent Runtime cannot hang.
- Create Rollouts under fixed-capacity internal staging. Durably sync initial
  metadata, atomically rename the inode, and durably commit affected directory
  entries before publishing it into discovery, resolving a producing context,
  or allowing Agent Machine side effects. Claim a non-cancellable publication
  critical section before rename so cancellation can win only while the inode
  is still staging. After rename, treat failed or ambiguous publication as
  destination-claimed storage and durably quarantine every possible
  destination instead of using staging cleanup. Terminal containment advances
  a transition-local publication generation so late completions cannot publish,
  then fences the superseded publication owner before quarantine; failure to
  fence by the absolute deadline is Host-fatal and never releases Kernel.
  Retain the cleanup lease until the complete barrier, successful staging
  unlink, or successful destination quarantine. Startup reconciles staging
  aliases and surviving final entries before exposing discovery or clone
  capability.
- Expose each retained Rollout at the read-only
  `/agent/rollouts/<rollout-id>` file path. Agent Runtime Service validates
  retained prefixes once while rebuilding discovery and advances active
  prefixes incrementally after owned appends. Each open captures that approved
  length on a pinned read-only source descriptor. Because the owning Rollout
  writer may only append and never overwrite or truncate a published prefix,
  reads fetch only the requested range within that fixed length; later or
  quarantined appends are never exposed. A fixed
  global and per-namespace-handle open-fid policy plus non-queuing read permits
  bounds memory and validation bandwidth without making large valid Rollouts
  unreadable. Ordinary Process handles cannot consume the authorized renderer
  attachment reserve, and inherited handles share one account rather than
  multiplying capacity. Reopening observes a later validated prefix. The
  surface remains available after Process exit and Host restart.
- Include active, terminal, and valid unterminated Rollouts; presentation may
  prioritize terminal entries but discovery does not hide retained evidence.
- Add Agent Runtime Service-owned `/mnt/agent-runtime/clone` as the
  clone-via-open path for an attached local Shell or renderer to request a
  top-level Agent Process. Service Manager binds this capability only into the
  authorized renderer's attachment view over a Local Entry Shell Process
  namespace; it is absent from the underlying Shell Process namespace, not
  published in `/srv`, and not retained by child Process namespaces. It uses
  the current Root Agent Process as the ordinary Process parent and crosses
  `/proc/clone`; it does not create another Process owner or launch identity.
- Add `durability_required` to Agent Process runtime spawn overrides and use
  the active Rollout's existing ID and `process_path` metadata as the
  file-visible acknowledgment for background dispatch, excluding IDs visible
  before spawn. A request rejected before clone commit is a definite failure;
  after commit, a missing correlation or `/proc/host/boot_id` change is
  indeterminate and MUST NOT trigger automatic retry.
- Reconstruct the discovery view by enumerating retained Rollouts; do not add a
  separate durable history record or persistent index.
- Follow Agent Runtime Service's existing retention; add no TTL, quota, pin,
  archive, delete, or garbage-collection control in this change.
- Require consumers to refresh the directory when needed; add no change-event,
  watch, or subscription protocol.
- Require each discovered `rollout_id` to be a nonempty, unique, safe single
  path component sourced from exactly one leading `AgentMachineMeta` record.
  Require at most one `process_exit`, as the final record with no trailing
  record bytes. Isolate empty, misordered, repeated-metadata, conflicting-exit,
  post-exit, malformed, or colliding Rollouts with diagnostics during discovery
  without deleting or repairing their backing files or blocking valid entries.
- Keep `/proc` authoritative for live Process lifecycle and each Rollout
  authoritative for its durable execution evidence.
- Require authorized consumers to use the Alan OS file surface rather than
  System Store paths or a Host-private API.
- Reuse the existing `/agent` namespace capability: a Process that can read
  `/agent` can read retained Rollouts, while a Process without that mount has
  no Rollout-history authority.

## Capabilities

### New Capabilities

- `agent-rollout-history`: Terminal completion and read-only namespace
  discovery for retained Agent Process Rollouts.

### Modified Capabilities

- `agent-namespace-runtime`: Define the Agent Runtime Service-owned top-level
  Agent Process launch path for callers such as Alan Shell renderer hosts.
- `local-entry-service`: Keep renderer-only mounts out of the Shell Process
  namespace and bind them only into its authorized renderer attachment view.
- `plan9-kernel-substrate`: Serialize terminal Process transitions through a
  generic pre-exit runner finalization hook without adding agent semantics to
  Alan Kernel.
- `alan-os-host-lifecycle`: Own fatal storage-integrity admission closure and
  fail-stop Host termination requested through an injected adapter.

## Impact

- Affects Rollout completion and Agent Runtime Service file serving over its
  durable System Store subtree.
- Adds no new durable identity, database, index, Kernel Process type, or
  renderer-owned state.
- Adds no new retention or deletion policy.
- Adds no namespace notification protocol.
- Adds no Host-private startup API or duplicate runtime-metadata file.
- Adds one internal Agent Runtime Service-to-Host fatal-transition adapter; it
  does not add a Host command or file surface.
- Does not change Alan Kernel Process lifecycle authority, the aP wire surface,
  or Rollout evidence ownership.
- Must land before `define-alan-interaction-model` implements its durable
  review surface.
