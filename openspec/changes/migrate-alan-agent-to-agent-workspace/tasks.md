## 1. Workspace Model

- [x] 1.1 Define Agent Workspace objects for compatibility sessions, bounded
  Agent Runs, supervisor-raised tasks, memory entries, evidence, artifacts, and
  plans.
- [x] 1.2 Define conversation, task tree, evidence, memory review, approval
  form, and command palette buffers/views needed for the first workspace slice.
- [x] 1.3 Define commands for submit turn, resume yield, approve/deny command,
  interrupt, compact, rollback, inspect evidence, promote supervisor task, and
  open memory review.

## 2. Projection

- [x] 2.1 Map current session metadata into Agent Workspace objects and native
  references.
- [x] 2.2 Map current `alan_protocol::EventEnvelope` values into workspace
  conversation, task, form, evidence, and audit projections.
- [x] 2.3 Map child runs and delegated skills into child Agent Run/task
  projections.
- [x] 2.4 Map memory recall, promotion, and flush observations into memory
  review projections without changing memory ownership semantics.
- [x] 2.5 Map rollout artifacts, effects, checkpoints, and evidence into
  inspectable workspace evidence views.

## 3. TUI Host Integration

- [x] 3.1 Render the first Agent Workspace conversation and task projections in
  Alan TUI behind a compatibility-first path.
- [x] 3.2 Preserve current daemon creation/attach, hydration, reconnect,
  submission, resume, interrupt, compact, rollback, and pending-yield behavior.
- [x] 3.3 Add parity fixtures between current TUI reducer output and Agent
  Workspace semantic projections.

## 4. Verification

- [x] 4.1 Run focused Alan Agent workspace projection tests.
- [x] 4.2 Run affected Alan TUI tests.
- [x] 4.3 Run formatting and relevant workspace checks.
- [x] 4.4 Run `openspec validate migrate-alan-agent-to-agent-workspace --strict`.
- [x] 4.5 Run `openspec validate --all --strict`.
