# Disposition — 2026-09-24

Delivery boundary updated 2026-09-26: automatic typed routing, qualification and
activation are owned by the active [qualify-agent-input-routing](../qualify-agent-input-routing/)
successor. Future-routing discussion below records accepted direction, not delivery
or a canonical-spec synchronization claim for this explicit-input change.

Planning complete and consolidated design confirmed by the user on 2026-09-24.
ADR-0058 is accepted direction. Runtime implementation is in progress in PR #937;
the first explicit-input slice is not yet merged and the remaining implementation
tasks are pending.

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

## Delivery activation — 2026-09-26

The user authorized completing explicit unified input through review, merge,
implemented-spec synchronization and archival. Finish the current governed command
slice, then ordered multi-client admission/cwd/cancellation/recovery and the
project-file/control-command/terminal acceptance loop. Keep unprefixed input
Agent-routed. Before archiving, explicitly transfer automatic routing and its
qualification/activation contracts to an active successor and update references;
this authorization does not activate automatic execution or parked product work.
