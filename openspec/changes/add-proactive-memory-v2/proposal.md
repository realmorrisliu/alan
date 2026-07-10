## Why

Alan should proactively retain durable user and workspace facts without turning
memory into hidden provider state. The previous proposal put durable mutation,
ledger authority, and review/revert APIs inside runtime and daemon surfaces;
under the Plan 9-like design those responsibilities belong to mounted Memory
Stores and their file-server owner.

## What Changes

- Keep model-mediated candidate planning for semantic judgment, but make it
  produce a bounded write proposal rather than mutate memory.
- Make the selected Memory Store the only authority that validates paths,
  commits durable memory and ledger records, applies redaction, and reverts its
  own writes.
- Expose write proposals, status, result, ledger records, recent-write events,
  and revert control as files under `/mnt/mem`; remove the proposed daemon memory
  endpoints.
- Preserve the current pure-text workspace memory layout as a compatibility
  backing tree while separating Personal, System-Continuity, App, and Workspace
  Memory Store authority.
- Make `[memory].enabled = false` a namespace/configuration decision that withholds
  writable Memory Store surfaces and suppresses proactive candidate planning.
- Keep recall, handoff, and generated memory surfaces bounded, source-linked,
  and unable to reintroduce reverted content.
- Keep `alan memory recent|show|revert` only as file-client convenience commands;
  they do not own memory policy or storage.

## Capabilities

### New Capabilities

- `memory-store-write-audit`: Defines proactive write proposals, Memory Store
  commit authority, file-backed ledger and recent-write streams, redaction,
  inspection, and precise revert semantics.

### Modified Capabilities

- `runtime-memory-contract`: Splits candidate planning from Memory Store commit
  authority; runtime no longer directly mutates durable memory files.
- `runtime-memory-surfaces`: Generated surfaces reference store-owned write and
  evidence paths and exclude reverted store content.

## Impact

- `alan-agent-engine` still schedules bounded candidate planning and consumes
  Memory Store results, but no longer owns durable memory mutation.
- `alan-memfs` owns write transactions, ledger records, redaction enforcement,
  revert, retention, and recent-write events under `/mnt/mem`.
- Existing `.alan/memory/` workspace files may remain a compatibility backend
  behind the Workspace Memory Store adapter; callers use namespace paths.
- CLI and future UI review surfaces become file clients; no new daemon endpoints,
  session scopes, or artifact-read APIs are introduced.
