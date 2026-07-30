## Why

Durable Rollouts survive Agent Process exit, but authorized Alan OS consumers
cannot discover them without scanning Agent Runtime Service System Store
backing. That prevents reliable review after Process exit or Alan OS Host
restart even though the execution evidence already exists.

## What Changes

- Attempt to append one `process_exit` record to the existing Rollout before
  clean runtime cleanup. On success it carries the authoritative numeric exit
  code and, when available, the existing `AgentExecutableResult`; it does not
  introduce another terminal status model.
- Bound the terminal append-and-flush attempt with an internal deadline. On a
  persistence error or timeout, emit a structured diagnostic and release
  finalization so `/proc` can still publish the authoritative Process exit.
  Later discovery treats a complete valid `process_exit` as authoritative even
  after an ambiguous flush result; only absence or a torn tail is incomplete.
- Extend the generic Process runner bridge with a prepared terminal
  finalization hook. Alan Kernel asks the runner to prepare the per-Process
  hook before the committed Process becomes controllable, invokes it exactly
  once before any terminal transition is published, and, for control-driven
  exit, awaits it before aborting the runner. Other Process images use the
  default no-op.
- For `/bin/alan-agent`, prepare a pending Process-local terminal-context
  barrier before control becomes reachable. Resolve it on every startup path
  with either the existing Rollout metadata plus ownership of the runtime task
  and cleanup guard, or an explicit no-producing-Rollout outcome. The ordinary
  run path hands that ownership to terminal finalization instead of shutting
  down or dropping it. Terminal finalization cancels startup or execution,
  awaits that barrier, and quiesces every Rollout writer before appending
  `process_exit`, so no producing Rollout is missed and no later record can
  follow the terminal record.
- Apply the same executable-eligibility check during terminal preparation as
  during System Process dispatch. If any pre-dispatch path returns after a
  barrier was registered, resolve it explicitly as no producing Rollout so an
  exit such as missing executable or unavailable Agent Runtime cannot hang.
- Expose each retained Rollout as its existing JSONL record at the read-only
  `/agent/rollouts/<rollout-id>` file path. The surface remains available
  after Process exit and Host restart.
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
  Isolate empty, misordered, repeated-metadata, malformed, or colliding
  Rollouts with diagnostics during discovery without deleting or repairing
  their backing files or blocking valid entries.
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

## Impact

- Affects Rollout completion and Agent Runtime Service file serving over its
  durable System Store subtree.
- Adds no new durable identity, database, index, Kernel Process type, or
  renderer-owned state.
- Adds no new retention or deletion policy.
- Adds no namespace notification protocol.
- Adds no Host-private startup API or duplicate runtime-metadata file.
- Does not change Alan Kernel lifecycle authority, the aP wire surface, or
  Rollout evidence ownership.
- Must land before `define-alan-interaction-model` implements its durable
  review surface.
