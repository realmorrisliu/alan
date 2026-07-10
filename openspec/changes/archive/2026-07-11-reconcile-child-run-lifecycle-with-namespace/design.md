## Context

`child-run-lifecycle` was merged in the daemon/DTO era. Since then the
namespace-native substrate made `/proc` parentage plus the agent overlay
(`children/`, aggregate `events`) the canonical child-visibility surface, and
the daemon-backed TUI path was deleted (`remove-daemon-backed-tui-compat`,
archived 2026-07-08). The consumer audit for this change found:

- Daemon child-run endpoints (`SessionChildRunsList/Get/Terminate`): registered
  and implemented, but called by no client. The macOS console's
  `AlanAPIClient.swift` has no child-run functions; the file-backed TUI has no
  `/agents` commands. Only `daemon_payload_contract_test.rs` exercises them.
- The runtime-side machinery (child-run registry, liveness classification,
  progress metadata, governed termination virtual tool) is alive and consumed
  by the delegated path in `virtual_tools.rs`.

So the contradiction resolves cleanly: the dead client/daemon surfaces go, the
live runtime behavior stays, and the registry is re-grounded as a projection
rather than a competing source of truth.

## Goals / Non-Goals

**Goals:**

- Remove spec requirements (and code) for surfaces with zero consumers.
- State the authority order: `/proc` and the agent overlay own process state;
  child-run records are a delegation-scoped projection.
- Purge raw-rollout-path vocabulary from the failed-handoff contract.

**Non-Goals:**

- Changing runtime delegation behavior (registration, liveness, progress,
  governed termination stay as implemented).
- Building file-backed TUI child rendering (re-propose separately if wanted).
- Session-level daemon API cleanup (sessions/events endpoints have a live
  consumer — the macOS console — and are out of scope).
- Deciding remote child visibility (belongs to `define-remote-access-service`).

## Decisions

### 1. Remove, don't freeze, the daemon child-run control plane

With zero consumers, keeping the endpoints "frozen for compatibility" preserves
dead weight and a standing spec contradiction. Remote child control, if ever
needed, will be namespace projection through the remote-access service, not
these DTO routes. The payload contract test shrinks accordingly.

Alternative considered: keep list/read as a debugging surface. Rejected:
`/agent/<pid>/children/` and `machine/ui` file surfaces are the debugging
story, and the CLI/TUI already reads files.

### 2. Registry becomes a projection by contract

The runtime registry remains as implementation (the legacy delegated path needs
its bookkeeping), but the spec states that process state in a child-run record
is derived from — and yields to — `/proc` parentage and the agent overlay.
This keeps the align-delegation change's launch-metadata delta meaningful
(launch metadata is exactly the projection's delegation-scoped payload) while
retiring the record's claim to be an independent process table.

Alternative considered: delete the registry requirement outright. Rejected:
the delegated handoff, liveness classification, and terminated/timed-out
distinction consume it today; deleting the contract without migrating the
virtual-tool path would leave live behavior unspecified.

### 3. Sequenced behind align-delegation-capability-with-namespace

Both changes touch `child-run-lifecycle`. This change deliberately does not
modify `Child Run Registration` (align's delta) and only removes/adds disjoint
requirements, but archive-time sync still orders them: align lands first, then
this change rebases its delta against the merged spec. Same discipline for
`delegated-result-handoff` versus `define-evidence-retention-and-projection`
(disjoint requirements, evidence change owns `output_ref` semantics).

## Risks / Trade-offs

- [Risk] An out-of-tree consumer of the child-run endpoints exists → the audit
  covered this repo only; the removal PR description must call the endpoints
  out, and the route-contract verification failing on any lingering reference
  provides a backstop.
- [Risk] Spec removal is read as removing runtime behavior → the proposal and
  delta explicitly keep registration/liveness/progress/termination; only
  transport surfaces and authority claims change.
- [Risk] Parallel active changes drift the same spec files → sequencing note in
  both proposal and tasks; sync order is align → evidence → this change.

## Open Questions

- Should the governed parent termination virtual tool be restated now as a
  `/proc/<child>/ctl` write (spawn-model form), or left until the
  delegate-executable migration? (Leaning: leave; this change only removes dead
  surfaces and fixes authority claims.)
