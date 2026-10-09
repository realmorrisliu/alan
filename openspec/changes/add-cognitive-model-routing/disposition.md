# Disposition — 2026-09-20

## Current delivery — 2026-10-09

PR #1035 merged at `cb0ec7ec7746a8f5b2af77782b281bbce7a838f0`.
Finite-choice Connection evaluation, the TypeSafe adapter and invocation-scoped
Machine shadow advice are implemented. PR #1039 merged the bounded read-only source-owner Machine program at
`90a05fe5e26efdba6186518dd58295619095bf01` from reviewed head
`c69cf3ebfda8ebd52c71ec5ac46bf2de32d548eb`. It composes deterministic lookup,
one finite-choice evaluation, an explicitly budget-gated generation fallback and
owned wait/resume. PR #1040 synchronized the five delivered source-owner and
Machine-file requirements at `498f97e9f3604bfbc4462ea5a93155736d2591de`.
General mixed transitions remain unfinished. Frozen input-routing v1–v4
qualification failed; later reader corrections remain unqualified.
Automatic routing remains disabled. The delivered operation, adapter, read-only
snapshot, shadow, invocation and bounded source-owner requirements have canonical
owners; the broad `cognitive-model-routing` delta remains future direction.
See [typed-entry-delivery.md](typed-entry-delivery.md) and
[mixed-task-acceptance.md](mixed-task-acceptance.md) for exact delivery evidence.

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
