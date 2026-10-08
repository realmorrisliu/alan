# Disposition — 2026-09-20

## Current delivery — 2026-10-08

PR #1035 merged at `cb0ec7ec7746a8f5b2af77782b281bbce7a838f0`.
Finite-choice Connection evaluation, the TypeSafe adapter and invocation-scoped
Machine shadow advice are implemented. General mixed transitions remain unfinished.
The frozen v2 qualification failed; later reviewed source is unqualified.
Automatic routing remains disabled. Sync only the delivered operation, adapter,
read-only snapshot, shadow and invocation requirements; the broad
`cognitive-model-routing` delta is still future direction.
See [typed-entry-delivery.md](typed-entry-delivery.md) for exact delivery evidence.

## Historical entry decisions

Discussion update (2026-09-24):
[unify-agent-command-input](../unify-agent-command-input/disposition.md) owns
unified input and deterministic command operations. Its first slice uses explicit
prefixes. Automatic intent integration and qualification now belong to the active
[qualify-agent-input-routing](../qualify-agent-input-routing/disposition.md) successor. Generic typed
capability contracts remain here; Jev support is not claimed implemented.

Direction accepted. Typed evaluation is queued after the first usable-agent
tracer bullet, using the same real task; necessary recovery contracts may be
delivered earlier for reliability. This does not activate the old two-Process
plan. Read proposal and entry gates before applying tasks. Runtime changes
remain unimplemented.

See [next-planning.md](next-planning.md) for the ordered cross-change planning
checklist, ownership and activation gates on main 04753a2f. This change's
implementation checklist remains in [tasks.md](tasks.md).
Before this change is archived, its remaining cross-change roadmap must be
handed to an active successor with references updated, as required by task 3.3.

## 2026-10-07 bounded activation

The user authorized the finite-choice typed evaluation and no-effect shadow phase
following local lifecycle/model/UI acceptance. [entry-slice.md](entry-slice.md)
records the exact operation, lifecycle and ownership decisions. This activates
that bounded implementation scope; it did not activate automatic command routing,
claim a then-implemented provider adapter or authorize canonical sync before merge.
