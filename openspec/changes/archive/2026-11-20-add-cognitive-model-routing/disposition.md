# Disposition — 2026-09-20

## Planning handoff — 2026-10-14

The delegate-project-autonomy change reuses existing Connection capabilities for Guardian qualification. It does not require general mixed-Machine completion, new cognitive Process types, or automatic input routing.

See [the accepted roadmap](../../qualify-personal-daily-development/roadmap.md).

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
[unify-agent-command-input](../2026-11-19-unify-agent-command-input/disposition.md) owns
unified input and deterministic command operations. Its first slice uses explicit
prefixes. Automatic intent integration and qualification now belong to the parked
[qualify-agent-input-routing](../../qualify-agent-input-routing/disposition.md) successor. Generic typed
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

## Frozen as deferred R&D — 2026-11-19

The delivered finite-choice evaluation, TypeSafe adapter and no-effect Machine
shadow slice remain canonical. All remaining scope (general mixed transitions,
broad `cognitive-model-routing` and any provider-specific evaluator) is frozen as
deferred research. It is not on the M1–M6 execution order in the
[personal daily-use roadmap](../../qualify-personal-daily-development/roadmap.md), is
not authorized for implementation, and requires explicit reactivation with a new
planning decision. Automatic routing stays disabled. Terminal-host acceptance
steps in the retained evidence documents are historical; Alan does not depend on a
specific terminal host (ADR-0061).

## Archived without syncing deferred research — 2026-11-20

Archived under `openspec/changes/archive/2026-11-20-add-cognitive-model-routing/`
without synchronizing deferred research deltas into canonical `openspec/specs/`.
Only the previously merged finite-choice Connection evaluation, TypeSafe adapter,
no-effect Machine shadow slice (`#1035`), and bounded read-only source-owner
Machine program (`#1039`/`#1040`) are canonical. The unfinished `cognitive-model-routing`,
broad `alan-shell`, and `provider-request-controls` research artifacts remain
historical, non-normative deferred R&D; reactivating general mixed-Machine work
requires a new OpenSpec change, explicit planning decision, and standard-terminal
(POSIX/VT) acceptance (`ADR-0061`).
