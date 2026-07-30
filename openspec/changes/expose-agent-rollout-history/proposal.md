## Why

Durable Rollouts survive Agent Process exit, but authorized Alan OS consumers
cannot discover them without scanning Agent Runtime Service System Store
backing. That prevents reliable review after Process exit or Alan OS Host
restart even though the execution evidence already exists.

## What Changes

- Append one `process_exit` record to the existing Rollout before clean runtime
  cleanup. It carries the authoritative numeric exit code and, when available,
  the existing `AgentExecutableResult`; it does not introduce another terminal
  status model.
- Expose each retained Rollout as its existing JSONL record at the read-only
  `/agent/rollouts/<rollout-id>` file path. The surface remains available
  after Process exit and Host restart.
- Include active, terminal, and valid unterminated Rollouts; presentation may
  prioritize terminal entries but discovery does not hide retained evidence.
- Add `durability_required` to Agent Process runtime spawn overrides and use
  the active Rollout's existing ID and `process_path` metadata as the
  file-visible acknowledgment for background dispatch, excluding IDs visible
  before spawn.
- Reconstruct the discovery view by enumerating retained Rollouts; do not add a
  separate durable history record or persistent index.
- Follow Agent Runtime Service's existing retention; add no TTL, quota, pin,
  archive, delete, or garbage-collection control in this change.
- Require consumers to refresh the directory when needed; add no change-event,
  watch, or subscription protocol.
- Isolate malformed Rollouts during discovery without deleting or repairing
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

None.

## Impact

- Affects Rollout completion and Agent Runtime Service file serving over its
  durable System Store subtree.
- Adds no new durable identity, database, index, Kernel Process type, or
  renderer-owned state.
- Adds no new retention or deletion policy.
- Adds no namespace notification protocol.
- Adds no startup side API or duplicate runtime-metadata file.
- Does not change Alan Kernel Process lifecycle, aP, or rollout evidence
  ownership.
- Must land before `define-alan-interaction-model` implements its durable
  review surface.
