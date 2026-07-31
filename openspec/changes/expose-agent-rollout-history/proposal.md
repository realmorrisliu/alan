## Why

Durable Rollouts survive Agent Process exit, but authorized Alan OS consumers
cannot discover them without scanning Agent Runtime Service System Store
backing. That prevents reliable review after Process exit or Alan OS Host
restart even though the execution evidence already exists.

## What Changes

- Attempt to append one `process_exit` record to the existing Rollout before
  clean runtime shutdown and Process exit publication. On success it carries
  the authoritative numeric exit code and, when available, the existing
  `AgentExecutableResult`; AgentFS unbinding follows exit publication, and no
  second terminal status model is introduced.
- Bound Agent terminal finalization with one internal absolute deadline while
  reserving its final interval for containment. Context-barrier, quiescence,
  writer-fence, terminal-persistence, and pre-exit runtime-shutdown work stops
  at the earlier containment cutoff. On error or timeout, cancel the logical
  writer and runtime owners without awaiting stuck Host I/O and use the
  reserved interval to atomically quarantine the backing inode before
  releasing finalization. If containment itself does not return by the
  absolute deadline, report the failure through the Alan OS Host lifecycle
  adapter. Calling that Host-owned adapter is synchronously non-returning: it
  stops attachment and work admission and enters fail-stop termination without
  awaiting the stuck operation. Agent Runtime Service does not own whole-Host
  shutdown. Recovery may republish only after ownership ends and validation
  succeeds.
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
  that still carries any live runtime owner and cleanup for AgentFS already
  bound. The ordinary run path hands that ownership to terminal finalization
  instead of shutting down or dropping it. Terminal finalization cancels
  startup or execution, awaits that barrier, quiesces every live Agent Machine,
  and, when a Rollout exists, fences its writers before appending
  `process_exit`. Thus no live runtime leaks, no producing Rollout is missed,
  and no later record can follow the terminal record. Alan Kernel publishes
  exit before invoking the returned AgentFS cleanup action.
- Apply the same executable-eligibility check during terminal preparation as
  during System Process dispatch. If any pre-dispatch path returns after a
  barrier was registered, resolve it explicitly as no producing Rollout so an
  exit such as missing executable or unavailable Agent Runtime cannot hang.
- Create Rollouts under internal staging, publish only after the initial
  metadata flush, and recover quarantine only at service startup before
  exposing discovery or clone capability.
- Expose each retained Rollout at the read-only
  `/agent/rollouts/<rollout-id>` file path. Each open captures a validated
  complete-prefix length and SHA-256 digest. Reads stream that exact prefix
  through bounded scratch space and return data only after the digest
  revalidates, so later or quarantined writes are never exposed. A fixed
  global and per-namespace-handle open-fid policy bounds memory without making
  large valid Rollouts unreadable. Ordinary Agent Process handles cannot consume
  the Local Entry Login Namespace reserve, and inherited handles share one
  account rather than multiplying capacity. Reopening observes a later
  validated prefix. The surface remains available after Process exit and Host
  restart.
- Include active, terminal, and valid unterminated Rollouts; presentation may
  prioritize terminal entries but discovery does not hide retained evidence.
- Add Agent Runtime Service-owned `/mnt/agent-runtime/clone` as the
  clone-via-open path for an attached local Shell or renderer to request a
  top-level Agent Process. Service Manager binds this capability only into the
  Local Entry Login Namespace; it is not published in `/srv` or retained by
  Agent Process namespaces. It uses the current Root Agent Process as the
  ordinary Process parent and crosses `/proc/clone`; it does not create another
  Process owner or launch identity.
- Add `durability_required` to Agent Process runtime spawn overrides and use
  the active Rollout's existing ID and `process_path` metadata as the
  file-visible acknowledgment for background dispatch, excluding IDs visible
  before spawn and rejecting any `/proc/host/boot_id` change.
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
- `local-entry-service`: Bind the top-level Agent Process launch capability
  only into the Login Namespace handed to an authorized local renderer.
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
