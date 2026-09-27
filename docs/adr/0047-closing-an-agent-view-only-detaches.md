# Closing an Agent view only detaches

Lifetime direction revised 2026-09-27: the user selected independent foreground
Alan invocations and explicitly selected recovery. Survival after actual
application/process exit below is superseded as target direction by
[the active OpenSpec change](../../openspec/changes/unify-agent-command-input/disposition.md).
Renderer-only detach remains valid: closing a Herdr view while the native Alan
process stays alive does not shut down that instance or its Agent execution.
This does not claim the foreground runtime or canonical spec sync is complete.

Status: accepted

Closing an Agent ContentInstance, Pane, Tab, window, or the macOS app releases
only that renderer's Agent Attachment. It never infers Process ownership from
visibility or attachment counts and therefore does not terminate the Agent
Process. Stopping execution is a separate explicit Alan OS command written to
`/proc/<pid>/ctl`, with lineage effects determined by Process policy; a close UI
may offer that choice but defaults to closing the view.
