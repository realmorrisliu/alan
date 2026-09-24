# Disposition — 2026-09-24

## Outcome

The shell-like inline TUI reliability slice is delivered and ready to archive.
PR #931 merged the renderer work at `cec382aa98bae8d030462ef333e4dae618bdd588`.
Its macOS test and coverage jobs exposed a test-only AgentFS submission race;
PR #932 fixed the synchronization and merged at
`bda9a82e6faa66e6c3ec21496e864f70aa753935`. The repaired head passed the full
required CI suite. Codex reported no major issues on the final PR #931 and #932
heads; all PR #931 review threads were resolved.

The delivered renderer presents an inline `alan >` transcript and prompt,
temporary completion candidates, visible errors, terminal restoration, and
non-owning detach behavior. It continues to use the existing Process and
AgentFS surfaces; it does not own launch, routing, execution, or recovery.

## Handoff

- Unified input and command routing now belong to
  [unify-agent-command-input](../../unify-agent-command-input/disposition.md).
  This TUI slice retains the current behavior: ordinary input and `!` go to the
  Agent, while slash commands remain local.
- Model discovery and selection remain a future Connection/Machine concern in
  [cognitive routing](../../add-cognitive-model-routing/tasks.md); this change
  does not implement a picker or model-list refresh.
- Stream retention, Process recovery, and external-effect reconciliation remain
  with their owning services. The renderer contract does not promise recovery
  from retained-stream gaps or duplicate task execution.

The archived handoff note
[`unified-input-exploration.md`](unified-input-exploration.md) points to the
receiving change's latest interview. Earlier desktop GUI plans remain cancelled;
the product retirement and source removal are recorded in their respective
ADRs and archived changes.

See the [current roadmap](../../add-cognitive-model-routing/next-planning.md)
for the ordered follow-up: finish the `alan9` terminology adoption, then
implement the explicit unified-input stage.
