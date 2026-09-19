## Context

See proposal.md and ../add-cognitive-model-routing/architecture-review.md.
The review was approved; runtime implementation is deliberately a subsequent
planning step on the merged main branch.

## Goals / Non-Goals

Close product decisions, suspend obsolete task execution, and leave a truthful,
validated baseline. Do not implement Jev, remove desktop source, replace Shell
IO, alter permissions, or claim those follow-ups complete.

## Decisions

1. Keep Alan OS ownership and a terminal-neutral Alan entry. Herdr is preferred
   presentation infrastructure, not Process or authorization authority.
2. Accept mixed Machine direction in ADR-0055; rewrite the proposed cognition
   artifacts, but keep unimplemented runtime deltas out of canonical specs.
3. Mark retained desktop contracts maintenance-only. Bulk deletion of those
   requirements now would discard safety constraints before their consumers
   have been audited; a scoped source-removal change must remove them together.
4. Record all thirteen previous changes in disposition.md. Parked means
   intentionally inactive, not ready to apply. Cancelled history is not a
   successful archive and does not sync unfinished deltas.
5. Correct the package path contract now because existing source implements it.
   Preserve omitted scenarios in the two invalid MODIFIED deltas.
6. Keep immutable prior archives unchanged. Update ADRs with dated pointers,
   never pretend the older decision had always been the new one.

## Risks / Trade-offs

- Retained legacy contracts may be mistaken for roadmap → put lifecycle notices
  directly on the affected contracts and planning artifacts.
- A decision-only merge may look like feature delivery → record all runtime
  gaps and entry gates in disposition.md and the cognition tasks.
- Existing CI may fail independently → require current-head evidence; never
  bypass required checks or silently delete guard coverage.

## Migration Plan

Merge documentation and validated deltas through a PR; only then update main
and remove verified merged task branches. Preserve the primary checkout.
Rollback is a normal Git revert; no runtime or external-state migration occurs.
