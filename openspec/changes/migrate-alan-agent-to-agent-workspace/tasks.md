## 1. Workspace Model

- [x] 1.1 Define Agent Workspace objects for compatibility sessions, Agent
  Process projections, root-agent-raised work, memory entries, evidence,
  artifacts, actions, requests, and plans.
- [x] 1.2 Define conversation, process list, request/action review, evidence,
  memory review, approval form, and command palette buffers/views needed for the
  first workspace slice.
- [x] 1.3 Define commands for submit input, answer request, approve/deny action,
  interrupt, compact, rollback, inspect evidence, promote Root Agent work, and
  open memory review.

## 2. Projection

- [x] 2.1 Map current session metadata into Agent Process workspace objects and
  native references.
- [x] 2.2 Map current `alan_protocol::EventEnvelope` values into workspace IO,
  request, action, evidence, and audit projections.
- [x] 2.3 Map child runs and delegated skills into child Agent Process
  projections.
- [x] 2.4 Map memory recall, promotion, and flush observations into memory
  review projections without changing memory ownership semantics.
- [x] 2.5 Map rollout artifacts, effects, checkpoints, and evidence into
  inspectable workspace evidence views.

## 3. Alan Shell Host Integration

- [x] 3.1 Render the first Agent Process conversation and task projections in
  Alan Shell behind a compatibility-first path.
- [x] 3.2 Preserve current compatibility creation/attach, hydration, reconnect,
  submission, resume, interrupt, compact, rollback, and pending-yield behavior.
- [x] 3.3 Add parity fixtures between current `crates/tui` reducer output and
  Agent Process workspace projections.

## 4. Verification

- [x] 4.1 Run focused Alan Agent workspace projection tests.
- [x] 4.2 Run affected Alan Shell tests.
- [x] 4.3 Run formatting and relevant workspace checks.
- [x] 4.4 Run `openspec validate migrate-alan-agent-to-agent-workspace --strict`.
- [x] 4.5 Run `openspec validate --all --strict`.
