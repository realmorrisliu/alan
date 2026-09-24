# Disposition — 2026-09-24

Planning complete and consolidated design confirmed by the user on 2026-09-24.
ADR-0058 is accepted direction. No runtime implementation has been performed by
this change; implementation tasks remain pending.

This change receives unified-input planning preserved in the
[archived TUI handoff](../archive/2026-09-24-define-alan-interaction-model/unified-input-exploration.md).
Generic typed evaluation remains
owned by `add-cognitive-model-routing`; this change owns command-specific input,
Machine transitions and shared state/evidence extensions.

The first delivery slice implements explicit `!`/`:` behavior with unprefixed
Agent fallback. Automatic routing follows capability delivery, shadow qualification
and explicit activation. Canonical specs remain the current implementation baseline;
only delivered and merged deltas may be synced or archived.

## Accepted KISS revision

The user approved updating ADR/OpenSpec to native Host shell execution on
2026-09-24. The earlier bounded alan9 command grammar is superseded. This change
also owns native-path sandbox reconciliation. Alan-managed path metadata and
path values projected from captured command output use public project paths or
opaque references. Ordinary files in explicitly delegated Host mounts preserve
shell semantics and may contain native path text as project data. These are
planned contracts, not shipped behavior.
Linux VM/container migration, FUSE, 9P gateways and custom kernel work are deferred;
the two research notes are historical alternatives, not implementation tasks.
See [decision report](decision-report.md) for the consolidated accepted direction.

## Accepted internal-protocol refinement

The user approved keeping aP internal behind task-oriented alan9 commands and
requested design updates. Normal user workflows do not require protocol clients.
Agent project tools and native commands share public paths and backing files.
This refines the KISS revision without introducing a separate command authority,
new state owner or runtime implementation claim. Host Command Plane compatibility
is reconciled by an owning delta; command spellings remain delivery tasks.
