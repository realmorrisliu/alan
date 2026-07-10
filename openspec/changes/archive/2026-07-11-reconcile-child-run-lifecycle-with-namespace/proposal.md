## Why

`openspec/specs/child-run-lifecycle/spec.md` and
`openspec/specs/agent-file-layout-contract/spec.md` contradict each other: the
former mandates a child-run registry, a daemon child-run control plane
(list/read/terminate APIs), and TUI `/agents` commands, while the latter rules
that `children/` is derived from `/proc` parentage and "does not duplicate
process state owned by `/proc`" (no second source of truth). A consumer audit
(2026-07-10) shows the conflicting surfaces are dead: the file-backed TUI has
no `/agents` commands, and the macOS client's daemon API client
(`clients/apple/alan-macos/Services/Daemon/AlanAPIClient.swift`) covers
sessions/events/connections/skills but never calls the child-run endpoints —
their only consumer is the daemon's own payload contract test.

## What Changes

- **BREAKING** Remove the `Daemon Child-Run Control Plane` requirement and its
  endpoints (`/api/v1/sessions/{id}/child_runs*` in
  `crates/alan/src/daemon/api_contract.rs` / `routes.rs`); no client consumes
  them. Child visibility moves to the agent overlay (`/agent/<pid>/children/`,
  aggregate `events`); termination is `/proc/<pid>/ctl`.
- **BREAKING** Remove the `TUI Child-Agent Commands` requirement (`/agents`,
  `/agent terminate|kill`); it was never carried into the file-backed TUI. A
  file-surface-backed rendering can be re-proposed separately if wanted.
- Add a projection requirement: child-run records are a delegation-scoped
  projection over `/proc` parentage plus launch/handoff metadata; where record
  and process state disagree, `/proc` and the agent overlay are authoritative.
- Keep (unchanged): child-run registration for the delegated path, liveness /
  timeout classification, progress metadata, and the governed parent
  termination virtual tool — all transport-independent runtime behavior with
  live consumers in `crates/agent-engine/src/runtime/virtual_tools.rs`.
- Replace the residual raw-rollout-path vocabulary in
  `delegated-result-handoff`'s failed-handoff requirement with namespace-path
  references, matching `define-evidence-retention-and-projection`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `child-run-lifecycle`: remove the daemon control-plane and TUI command
  requirements (dead surfaces); add the projection-not-source-of-truth
  requirement. Does NOT touch `Child Run Registration`, which the active
  `align-delegation-capability-with-namespace` change already modifies — this
  change must land after (or rebase on) that one.
- `delegated-result-handoff`: `Failed Child Handoff Metadata` references the
  child's home/namespace path instead of a raw rollout path. Does NOT touch the
  two requirements modified by the active
  `define-evidence-retention-and-projection` change.

## Impact

- Deleted code: child-run route handlers in `crates/alan/src/daemon/routes.rs`,
  endpoint ids/constants/builders in `api_contract.rs`, child-run coverage in
  `daemon_payload_contract_test.rs`.
- Unchanged code: `agent-engine` child-run registry, liveness, progress, and
  governed termination (spec re-grounds their authority; behavior stays).
- Sequencing: depends on `align-delegation-capability-with-namespace` (same
  spec file) landing first; complementary to
  `define-evidence-retention-and-projection` (disjoint requirements in the same
  spec file).
