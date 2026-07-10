## Why

`add-content-addressed-knowledge` made checkpoint forks cheap, but branching
execution is still only an implied payoff. The next useful Ring 3 slice is a
headless file server that makes speculative branches inspectable and selectable
through Alan OS file operations before a scheduler starts driving them.

## What Changes

- Add `alan-branchfs`, a user-space aP file server for branch planning over
  content-addressed checkpoint roots.
- Expose a branch tree with `ctl`, `branches/`, `selected`, and `events` files.
- Support explicit `ctl` commands to fork a branch from a base root, score a
  branch, select a branch, and discard a branch.
- Store branch roots as `alan-knowledge` Merkle forks so unchanged state is
  shared and branch state remains tamper-evident.
- Emit JSON-line branch lifecycle records to a retained blocking-read `events`
  stream.
- Keep this slice headless: no automatic model calls, no daemon session fork
  replacement, no ranking strategy, and no native UI.

## Capabilities

### New Capabilities

- `branching-execution-file-server`: Defines the headless file-server contract
  for creating, observing, scoring, selecting, and discarding speculative
  checkpoint branches over the content-addressed knowledge store.

### Modified Capabilities

- None.

## Impact

- Adds a new workspace crate, `alan-branchfs`, depending on `alan-ap` and
  `alan-knowledge`.
- Adds focused aP integration tests for branch creation, cheap fork sharing,
  selected-branch publication, discard behavior, and event observation.
- Does not change `alan-kernel`, daemon `/api/v1/sessions/{id}/fork`, the agent
  runtime scheduler, macOS UI, or existing AgentFS surfaces.
