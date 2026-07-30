## 1. Durable Background Launch

- [ ] 1.1 Add clone-via-open `/mnt/agent-runtime/clone` to Agent Runtime
  Service. Pin the current `/agent/root` Process as the parent, allocate the
  pending Process through its `/proc/clone` context, accept one
  `AgentExecutableRequest`, and return the ordinary PID without adding another
  launch identity.
- [ ] 1.2 Bind the launch capability only into Local Entry Login Namespaces;
  do not publish it in `/srv`, add it to `/agent`, or retain it in Agent Process
  namespaces. Prove an attached Shell/renderer can reach it and a delegated
  Agent Process with read-write `/agent` cannot.
- [ ] 1.3 Reject `/mnt/agent-runtime/clone` commit when Root Agent is
  unavailable, replaced after open, or the request would amplify its
  capabilities; prove the Shell Process cannot launch `/bin/alan-agent`
  through its own Process-bound `/proc/clone`.
- [ ] 1.4 Add optional `durability_required` to
  `SpawnRuntimeOverrides`, preserve default best-effort behavior, and add wire
  round-trip and unknown-field tests.
- [ ] 1.5 Apply the spawn override to the existing Agent Runtime
  strict-durability setting and prove Rollout creation failure does not fall
  back to an in-memory Agent Machine.
- [ ] 1.6 Prove a strict-durability `/mnt/agent-runtime/clone` spawn can be
  acknowledged through the newly discovered active Rollout's ID and
  first-record `process_path`, with no Host path or internal runtime metadata
  exposed.
- [ ] 1.7 Prove a retained Rollout with the same PID path from a prior Host
  boot is excluded by the pre-spawn Rollout-ID listing.
- [ ] 1.8 Pin and revalidate `/proc/host/boot_id`, and prove a Host restart
  during correlation rejects the launch acknowledgment.

## 2. Terminal Rollout Evidence

- [ ] 2.1 Extend the generic `ProcessRunner` bridge with a default no-op
  terminal finalization hook that receives the committed invocation and
  intended numeric exit code.
- [ ] 2.2 Serialize runner completion, `/proc/<pid>/ctl`, and Host
  `record_exit` through one per-Process terminal claim; invoke and await the
  hook exactly once before publishing exit and before aborting a controlled
  runner, with race and generic no-op tests.
- [ ] 2.3 Add the `process_exit` Rollout record using the existing numeric
  Process exit code, completion timestamp, and optional
  `AgentExecutableResult`, with serialization tests.
- [ ] 2.4 Immediately after Agent Machine creation succeeds, publish existing
  `RuntimeStartupMetadata` through an early `RuntimeController` channel before
  later initialization and readiness; have Agent Runtime Service retain it
  with the existing `RuntimeHandle` and any eventual
  `AgentExecutableResult` in one internal Process-local terminal context.
  Prove failure after Rollout creation but before readiness still finalizes
  the producing Rollout, without an Engine-to-Service dependency, file, Host
  API, or durable identity.
- [ ] 2.5 Have System Process runner route Agent Executable finalization to
  Agent Runtime Service; first request Agent Machine quiescence and await a
  writer fence, then consume the terminal context exactly once and, for a
  producing Rollout, append and flush terminal `process_exit` before AgentFS
  cleanup, runner abort, and Kernel exit publication. Test normal result
  publication, active-transition cancellation, no post-exit append, and
  control exit code `130`.
- [ ] 2.6 Preserve Rollouts without `process_exit` as unterminated evidence
  without fabricating a result.

## 3. Rollout Discovery

- [ ] 3.1 Enumerate retained Rollouts from Agent Runtime Service's existing
  System Store subtree using the existing Rollout loader, with no persistent
  history index.
- [ ] 3.2 Reserve `/agent/rollouts` and expose each valid retained Rollout as
  one read-only `/agent/rollouts/<rollout-id>` JSONL file while preserving
  numeric PID entries and `/agent/root`.
- [ ] 3.3 Isolate malformed Rollouts with diagnostics, accept recoverable torn
  tails, and prove one bad file does not block valid entries.
- [ ] 3.4 Test discovery of active, terminal, and unterminated Rollouts across
  Process exit and Agent Runtime Service restart.
- [ ] 3.5 Test that `/agent` holders can read but not mutate Rollouts and that
  Processes without the `/agent` mount have no fallback access.

## 4. Verification And Archive Readiness

- [ ] 4.1 Run focused Agent Runtime Service, AgentFS, Rollout, and namespace
  tests, then run `just quality`.
- [ ] 4.2 PR review confirms there is no new execution identity, persistent
  index, retention policy, notification protocol, Host API, or renderer-owned
  state.
- [ ] 4.3 After implementation merges, sync `agent-rollout-history` into
  `openspec/specs/` and move the change to
  `openspec/changes/archive/YYYY-MM-DD-expose-agent-rollout-history/`.
