## 1. Terminal Rollout Evidence

- [ ] 1.1 Add the `process_exit` Rollout record using the existing numeric
  Process exit code, completion timestamp, and optional
  `AgentExecutableResult`, with serialization tests.
- [ ] 1.2 For Agent Processes with a producing Rollout, append and flush
  `process_exit` before clean Agent Runtime Service cleanup, and test normal
  result publication plus generic exit code `130`.
- [ ] 1.3 Preserve Rollouts without `process_exit` as unterminated evidence
  without fabricating a result.

## 2. Rollout Discovery

- [ ] 2.1 Enumerate retained Rollouts from Agent Runtime Service's existing
  System Store subtree using the existing Rollout loader, with no persistent
  history index.
- [ ] 2.2 Reserve `/agent/rollouts` and expose each valid retained Rollout as
  one read-only `/agent/rollouts/<rollout-id>` JSONL file while preserving
  numeric PID entries and `/agent/root`.
- [ ] 2.3 Isolate malformed Rollouts with diagnostics, accept recoverable torn
  tails, and prove one bad file does not block valid entries.
- [ ] 2.4 Test discovery of active, terminal, and unterminated Rollouts across
  Process exit and Agent Runtime Service restart.
- [ ] 2.5 Test that `/agent` holders can read but not mutate Rollouts and that
  Processes without the `/agent` mount have no fallback access.

## 3. Verification And Archive Readiness

- [ ] 3.1 Run focused Agent Runtime Service, AgentFS, Rollout, and namespace
  tests, then run `just quality`.
- [ ] 3.2 PR review confirms there is no new execution identity, persistent
  index, retention policy, notification protocol, Host API, or renderer-owned
  state.
- [ ] 3.3 After implementation merges, sync `agent-rollout-history` into
  `openspec/specs/` and move the change to
  `openspec/changes/archive/YYYY-MM-DD-expose-agent-rollout-history/`.
