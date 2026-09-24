# Disposition — 2026-09-23

Implementation active for the first usable-Agent tracer bullet, selected from
the vertical route in
[`next-planning.md`](../add-cognitive-model-routing/next-planning.md).
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

This change owns only the minimal terminal Agent loop and its direct
verification. Reliability beyond turn cancellation, continued use, and
non-owning renderer exit must be re-planned from observed gaps before this
change is archived. The queued interaction change remains the owner for richer
presentation, not Agent launch or execution authority.
