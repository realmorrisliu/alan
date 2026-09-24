# Disposition — 2026-09-20

Queued for tracer bullet slice 2 after the first usable-agent tracer bullet,
delivered by merged PR #929 and recorded in the
[archived change](../archive/2026-09-24-define-alan-programmable-client-surface/).
This change owns re-planning of remaining inline presentation and terminal
acceptance over the same Process execution path.
See [the roadmap](../add-cognitive-model-routing/next-planning.md).
Old native-client obligations and renderer-only launch in D7 are rejected.
Native desktop GUI work is cancelled, not deferred. Retained draft references
to macOS UI are historical inventory only. Desktop source removal is complete
and recorded in [the archived change](../archive/2026-09-23-remove-retired-desktop-source/);
shared Rust platform adapters remain supported.
History UI, background/event-driven modes and full editfs are deferred.

Rewritten tasks supersede the former 18-task plan without marking it complete.
Retained proposal/design/deltas are inventory only and must be replaced before
implementation; they must not be synced as implemented canonical behavior.

## Tracer-bullet handoff — 2026-09-24

PR #929 adds automated Root Agent PID-replacement and transcript-reconciliation
coverage, but live reattachment of an already-attached TUI across an active
Root Agent replacement remains unverified. Durable output-offset continuity
and visible retention-gap behavior are also not claimed by the tracer bullet.
Task 2.3/2.4 own re-planning and ordinary-terminal/Herdr acceptance for those
renderer-visible cases, including pane closure without Host shutdown or task
replay. This change does not take ownership of Process lifecycle, Agent task
recovery, or external-effect reconciliation; any such implementation gap
requires a new change under the owning service before code is written.
