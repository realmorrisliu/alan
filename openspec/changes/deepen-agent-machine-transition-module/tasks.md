## 1. Characterize The Transition Boundary

- [x] 1.1 Map every direct `AgentMachine`, `RuntimeLoopState`, and
  `NamespaceRuntimeEnvironment` access in the accepted-submission path and assign
  each value to Process-loop control, Agent Machine state, or a narrow
  transition dependency.
- [ ] 1.2 Add focused characterization checks for submission acceptance,
  generation completion, Yield/resume, Tool replay, compaction, deferred
  actions, persistence/recovery, cancellation, and failure evidence.
- [ ] 1.3 Confirm current AgentFS, `/proc`, rollout, and checkpoint observations
  before moving ownership.

## 2. Make Agent Machine The State Owner

- [ ] 2.1 Move current submission, turn state, pending Yield, Tool replay,
  active-task, and deferred action state behind Agent Machine semantic
  operations.
- [ ] 2.2 Make Tape, recorder, flags, and transition state private; remove the
  public Agent Machine re-export and replace direct field access.
- [ ] 2.3 Keep or delete the private `RuntimeLoopState` according to whether it
  still groups cohesive Machine state; do not preserve it as a shared field bag.
- [ ] 2.4 Add adjacent white-box tests for private Machine transitions and keep
  supported external tests at file boundaries.

## 3. Extract The Concrete Transition Owner

- [ ] 3.1 Move execution after `Submission` acceptance into one concrete
  transition module with a compact outcome for the outer loop.
- [ ] 3.2 Keep input polling, channel closure, shutdown, cancellation, and
  heartbeat in `engine.rs` and verify they do not become Machine state.
- [ ] 3.3 Restrict complete namespace-environment access to the transition
  boundary and pass narrow concrete inputs to generation, Tool, policy, memory,
  and evidence workflows.
- [ ] 3.4 Delete displaced forwarding helpers, broad environment parameters,
  direct Machine field access, and any temporary dual transition path.

## 4. Verify And Deliver The Stack

- [ ] 4.1 Run focused `alan-agent-engine` tests, workspace formatting, Clippy,
  the canonical repository quality gate, and strict OpenSpec validation.
- [ ] 4.2 Verify AgentFS, `/proc`, aP, Yield, Tool, compaction, memory,
  persistence, recovery, and child-process behavior remains unchanged.
- [ ] 4.3 Deliver focused stacked PRs in dependency order; each PR must move one
  complete owner and delete its old path rather than add scaffolding or a
  compatibility layer.
- [ ] 4.4 For every PR, resolve all actionable Codex Review comments, rerun CI on
  the current HEAD, wait through a follow-up review window, and merge only when
  no unresolved or new issue remains.

## 5. Sync And Archive

- [ ] 5.1 After all implementation PRs merge, sync the delta requirement into
  canonical `agent-namespace-runtime` and run strict OpenSpec validation.
- [ ] 5.2 Confirm the merged code and canonical spec contain no retired public
  Machine surface or dual transition owner.
- [ ] 5.3 Archive the change only after implementation, review, verification,
  and canonical spec sync are complete.
