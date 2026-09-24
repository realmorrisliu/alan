# Disposition — completed 2026-09-24

Completed by PR #929, merged 2026-09-23, and archived after syncing its
implemented deltas to the canonical specs. This record is historical and
non-normative. The current route and follow-up ownership are in the
[`next-planning.md`](../../add-cognitive-model-routing/next-planning.md).

The delivered first usable-Agent tracer bullet was selected from that vertical
route.
The prior 33-task editable-buffer/run/package design is superseded inventory,
not an implementation contract.

The first slice attaches the terminal CLI to the existing Host-managed
`/agent/root` using the existing file-backed `alan-terminal-ui`. Ordinary TTY
text is an Agent task; a leading `!` explicitly requests the exact remainder
through the existing `bash` Tool, with normal governance intact. Non-TTY stdin
is one Agent task with answer on stdout, diagnostics on stderr, and task outcome
in the exit status. Existing Connection, Tool governance, Host Mount, sandbox,
Process and durable evidence owners remain unchanged. No new evaluator Process
or execution manager is authorized.

The old `editable-buffer-file-server` and `editable-buffer-interaction` deltas
are removed because their behavior is not part of this tracer bullet. Generic
packaging, `run`, editfs UI, scripts, typed evaluation and Jev remain outside
scope. No removed or unimplemented requirement may be synced as current
behavior.

## Handoff and remaining gaps

All tasks in this change are complete. PR #929 adds automated Root Agent
PID-replacement and transcript-reconciliation coverage, but live reattachment
of an already-attached TUI across an active Root Agent replacement remains
unverified. Durable output-offset continuity and visible retention-gap
behavior are also not claimed by this tracer bullet.

Those renderer-facing gaps were handed to the queued
[`define-alan-interaction-model` tasks](../2026-09-24-define-alan-interaction-model/tasks.md)
and Slice 2 of the roadmap. That change must rewrite its retained proposal,
design, and deltas before implementation; it owns presentation and terminal
acceptance, not Agent launch or Process/runtime authority. Any Process lifecycle,
Agent recovery, persistence, or external-effect gap needs a change under its
owning service before implementation. This archive does not claim those gaps
are complete or authorize additional work here.
