# Disposition — 2026-09-27

## Accepted session lifetime revision

On 2026-09-27 the user explicitly selected one independent Alan invocation per
Herdr terminal session, with recovery only on explicit request. This replaces
the earlier channel-wide Root Agent attachment and independent background Host
assumptions for this delivery. Ordinary terminals use the same foreground model;
Herdr is a preferred terminal host, not an Alan runtime or authorization dependency.

The CLI owns the native application lifetime. Its existing alan9 composition
keeps Kernel, Service Manager and Agent Runtime Service responsibilities intact
inside that invocation; a logical Service does not require a separate background process.
`/agent/root` is local to that instance. Separate invocations do not implicitly
share input, cwd, Process identity or pending work. Multiple authorized clients
of one live Agent still use its ordered admission and correlated results.

A terminal host may keep the foreground process alive when its view detaches.
Actual Alan exit ends that instance and its owned work; Alan does not start a
replacement background process to preserve execution. Recovery selects explicit durable
records, validates current authority, and restores reliable pending work paused.
No automatic channel-wide latest-rollout selection or effect replay is authorized.

The CLI-only distribution and independent foreground startup, Root, runtime
endpoint and shutdown shipped in PR #1009 (`faf7ee8e6b71c9f6c460e45ac049485035ec4d3b`).
Current-head Codex review found no issues and required CI passed; integration
tests cover two simultaneous invocations and independent shutdown. Task 2.18.1
records this slice. Cross-invocation cwd/queue isolation and shared-store
concurrency remain under task 2.18.2. Explicit durable recovery selection,
authority validation and paused queue restoration remain under task 2.19.

This is a partial delivery. The canonical standalone distribution requirement
is synchronized in this change; the owning lifecycle specs and ADR-0056 still
need the focused implementation-status sync tracked by task 4.2.2. The remaining
lifecycle clauses in its deltas, ADR-0047/0054/0056/0058 and canonical specs also
require the cross-surface audit in task 2.17.4 and remaining sync in task 4.2
before this change can be archived. PR #981's automatic Root rollout selector and PR #982's
automatic Host-restart acceptance are not merge prerequisites for the revised
delivery; reusable evidence-recovery fixes may be extracted into focused PRs.

Delivery boundary updated 2026-09-26: automatic typed routing, qualification and
activation are owned by the active [qualify-agent-input-routing](../qualify-agent-input-routing/)
successor. Future-routing discussion below records accepted direction, not delivery
or a canonical-spec synchronization claim for this explicit-input change.

The original consolidated design was confirmed by the user on 2026-09-24;
lifecycle planning is reopened by the accepted revision above.
ADR-0058 is accepted direction. Runtime delivery is in progress through small PRs:
versioned input, governed explicit command execution, shared cwd/queue admission,
correlated completion and cancellation have landed through PR #970. The full
explicit-input contract and acceptance matrix remain incomplete; unchecked tasks
are not an assertion that no supporting code exists.

The `/bin/agent_work` facade is implemented in PR #973 with its command/schema
contract in design.md. Local Service Manager tests, schema validation and the
full repository quality gate pass after retaining the existing dependency boundary.
Its current-head review and CI merge gates remain pending. End-to-end Agent discovery,
commit-failure injection and unknown-outcome acceptance remain part of task 2.13;
this implementation is not an archive-readiness or canonical-spec sync claim.

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
new state owner. Host Command Plane compatibility is reconciled by an owning delta;
the first implemented command spelling and receipt schema are recorded in design.md.
Remaining acceptance and merge evidence are tracked in tasks.md.
