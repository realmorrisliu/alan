# Proposal

## Why

Alan can generate answers and has governed project tools, but the current bare
CLI does not offer a complete project authorization and development workflow.
Herdr comparison with fx on 2026-09-29 exposed missing entry wiring, weak task
feedback and an interruption/recovery anomaly; a successful self-development
loop has not been demonstrated.

## What Changes

- Make project selection and explicit Host Mount approval usable within the
  foreground CLI, with truthful project/cwd/Connection status and actionable
  failure states. Launch cwd is a suggestion, never implicit authority.
- Connect existing file/Skill completion and channel-scoped composer history;
  make command submission and queued-input acknowledgement predictable.
- Refine the inline terminal hierarchy, spacing, semantic styles, bounded Tool
  summaries and on-demand details without replacing Ratatui or host scrollback.
- Expose the effective model and a Connection-owned model selection flow whose
  result is bound to the next eligible turn, with explicit failure feedback.
- Establish staged acceptance: usable project agent, supervised self-development,
  then repeated daily use. Record real source changes, tests, diffs and relaunch
  evidence before claiming Alan can develop itself.
- Coordinate existing input/recovery work through `unify-agent-command-input`;
  do not duplicate its runtime deltas. Transfer terminal model-picker planning
  from the cognitive roadmap here; typed evaluation and automatic routing stay
  outside this milestone. Parked history-browser work stays parked.

## Capabilities

### New Capabilities

None. Existing product and renderer contracts own this work.

### Modified Capabilities

- `alan-interaction-model`: project onboarding, visible submission disposition,
  effective-model interaction and evidence required for self-development readiness.
- `rust-inline-tui`: production completion wiring, predictable local-command
  interaction and semantic terminal presentation.
- `tool-result-presentation`: usable summary/detail navigation, actionable errors
  and faithful truncation boundaries.

## Impact

Expected implementation touches CLI composition, Host Mount native presentation,
AgentFS projections and the existing TUI composer/history/layout path. Runtime
queue/cancellation fixes remain in the existing input change and owners. No new
execution manager, grant registry, provider router, terminal emulator or desktop
app is introduced. No new dependency is presumed necessary.

This submission is planning only. It does not claim implementation, CI success,
archive readiness or reactivation of parked changes. See `assessment.md` for
live observations, `design.md` for interaction decisions and `tasks.md` for gates.
