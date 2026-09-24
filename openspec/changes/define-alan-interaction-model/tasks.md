## 1. Replan terminal experience

Status: queued behind the delivered first usable-agent slice; retained
proposal/design/deltas are inventory only. See [disposition.md](disposition.md) and the
[active roadmap](../add-cognitive-model-routing/next-planning.md).
The former 18-task desktop/background-dispatch plan is superseded, not completed.

- [ ] 1.1 Rewrite proposal/design/deltas around the real execution path in the [archived programmable-client change](../archive/2026-09-24-define-alan-programmable-client-surface/); remove native desktop obligations and renderer-owned launch assumptions.
- [ ] 1.2 Identify remaining presentation gaps after the first task works; do not duplicate the first slice's minimal input/output requirements.
- [ ] 1.3 Define inline output, progressive disclosure, result inspection and terminal lifecycle acceptance; retain Process/runtime ownership.

## 2. Terminal acceptance

- [ ] 2.1 Implement only observed presentation gaps over existing Process/AgentFS files.
- [ ] 2.2 Verify resize, narrow panes, Unicode, paste and scrollback in an ordinary terminal and Herdr using the current build.
- [ ] 2.3 Verify Ctrl-C, EOF, live Root Agent PID replacement, detach/reattach Process identity, output-offset continuity and visible retention-gap behavior against the shared lifecycle contract; presentation closure must not kill Host or replay a task.
- [ ] 2.4 Record visual/interactive evidence separately from execution logs; propose Herdr-specific notification or recognition only if a concrete gap remains.

## 3. Delivery

- [ ] 3.1 Run focused renderer tests, applicable quality checks and strict OpenSpec validation.
- [ ] 3.2 Complete current-head CI and Codex review/fix/resolve; merge and sync implemented deltas only.
- [ ] 3.3 Hand off any unfinished roadmap items before archive; history UI, background/event-driven modes and full editfs remain separate deferred scope.
