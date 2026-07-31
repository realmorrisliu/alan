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
  exposed. Prove this acknowledgment guarantees Rollout creation but does not
  claim terminal outcome persistence.
- [ ] 1.7 Prove a retained Rollout with the same PID path from a prior Host
  boot is excluded by the pre-spawn Rollout-ID listing.
- [ ] 1.8 Pin and revalidate `/proc/host/boot_id`, and prove a Host restart
  during correlation rejects the launch acknowledgment.

## 2. Terminal Rollout Evidence

- [ ] 2.1 Extend the generic `ProcessRunner` bridge with default no-op
  terminal-finalizer preparation and execution hooks. Prepare one
  per-Process finalizer from the committed invocation before the Process is
  visible as running or accepts `ctl`, and invoke it with the intended numeric
  exit code.
- [ ] 2.2 Serialize runner completion, `/proc/<pid>/ctl`, and Host
  `record_exit` through one per-Process terminal claim; retain, invoke, and
  await the prepared finalizer exactly once before publishing exit and before
  aborting a controlled runner. Carry Process outcome only for a winning runner
  completion; control and Host winners carry none. Test result/control races,
  Host omission, immediate control, and generic no-op behavior.
- [ ] 2.3 Add the `process_exit` Rollout record using the existing numeric
  Process exit code, completion timestamp, and optional
  `AgentExecutableResult`, with serialization tests.
- [ ] 2.4 Immediately after Agent Machine creation succeeds, publish existing
  `RuntimeStartupMetadata` through an early `RuntimeController` channel before
  later initialization and readiness. During generic finalizer preparation,
  have System Process runner synchronously register a pending terminal-context
  barrier and startup cancellation path with Agent Runtime Service before
  Process control is reachable. Resolve the barrier exactly once on every
  startup exit with either the existing metadata plus ownership of the live
  `RuntimeController` (or equivalent runtime-task guard) and Process cleanup
  guard, or an explicit no-producing-Rollout outcome; a cloned
  `RuntimeHandle` alone is insufficient. Keep any eventual
  `AgentExecutableResult` only in the candidate runner outcome. Prove control
  during the delivery window and failure after Rollout creation but before
  readiness still finalize correctly. Create at an internal staging path,
  register an independently cancellable creation owner before awaiting open,
  register writer containment before the initial metadata flush, and atomically
  publish only after that flush succeeds. Test cancellation during open and
  control or deadline expiry while the initial flush stalls,
  without an Engine-to-Service dependency, file, Host API, durable identity, or
  absent-resolution fallback.
- [ ] 2.5 Apply the same committed-namespace executable eligibility check in
  System Process runner finalizer preparation as in `run`. Keep the generic
  no-op when `/bin/alan-agent` is not mounted, and explicitly resolve no
  producing Rollout on every pre-dispatch return after registration, including
  an unavailable Agent Runtime Service. Prove missing-image exit `127` and
  service-loss exit cannot wait on an orphaned barrier.
- [ ] 2.6 Have System Process runner route Agent Executable finalization to
  Agent Runtime Service; first signal startup or runtime cancellation and
  await the terminal-context barrier. For a producing Rollout, request Agent
  Machine quiescence that cancels or drains both ordinary transitions and
  deferred runtime actions, then await a writer fence covering every Rollout
  producer. The normal run path SHALL hand off its live runtime and cleanup
  ownership instead of calling `RuntimeController::shutdown`, dropping the
  task owner, or cleaning up before finalization. Append and flush terminal
  `process_exit`, then shut down and release the runtime task, perform AgentFS
  and Process cleanup, and consume the terminal context exactly once before
  runner abort or Kernel exit publication. Test immediate control before
  metadata delivery, explicit no-Rollout startup, normal result publication
  followed by successful finalization while the runtime remains live,
  controller-drop regression, active-transition cancellation,
  `/proc/<pid>/ctl` exit during an active deferred action, deadlock-free barrier
  and fence completion, no post-exit append, and control exit code `130`.
- [ ] 2.7 Bound Agent terminal finalization under one fixed absolute deadline
  while reserving its final interval for containment. Stop context barrier,
  quiescence, writer fence, and terminal append/flush work at the earlier
  persistence cutoff. On error or timeout, emit a structured
  PID/Rollout/exit-code/stage diagnostic, cancel logical writer and runtime
  owners without awaiting stuck Host I/O, remove the entry under the
  snapshot-open lock, atomically quarantine the backing inode within the
  reserved interval before Kernel may publish exit, and perform bounded
  cleanup. If containment errors or reaches the absolute
  deadline, stop accepting work and enter Host-fatal termination without
  awaiting the stuck operation or publishing Process exit. Recover only after
  ownership ends and validation succeeds under the same Rollout ID. Test
  an earlier stuck writer blocking the fence, disk-full/flush-error, timeout,
  ambiguous-flush no-retry, a complete record surviving an error, absent/torn
  outcomes, stale I/O affecting only quarantine, containment failure as
  bounded Host-fatal termination, cleanup, and Host-shutdown progress.
- [ ] 2.8 Preserve Rollouts without `process_exit` as unterminated evidence
  without fabricating a result. Treat any discovered complete valid
  `process_exit` as authoritative even when its original flush reported an
  error.

## 3. Rollout Discovery

- [ ] 3.1 Enumerate retained Rollouts from Agent Runtime Service's existing
  System Store subtree using the existing Rollout loader, with no persistent
  history index.
- [ ] 3.2 Reserve `/agent/rollouts` and expose each valid retained Rollout as
  one read-only `/agent/rollouts/<rollout-id>` JSONL file while preserving
  numeric PID entries and `/agent/root`. Materialize each open as a
  service-owned immutable snapshot of the validation-approved complete prefix;
  never retain the Host backing inode in an aP fid. Order snapshot open and
  discovery removal under one lock, and prove an existing fid cannot observe
  later active writes or quarantined stale I/O while reopen can observe later
  complete records.
- [ ] 3.3 Isolate malformed Rollouts with diagnostics, accept recoverable torn
  tails only after earlier complete records pass envelope validation, and
  require exactly one leading `AgentMachineMeta` with no later metadata record.
  Permit at most one `process_exit`, require it to be the final record with no
  later complete or torn bytes, and reject conflicting exits or post-exit
  records.
  Validate that its `rollout_id` is nonempty, neither `.` nor `..`, contains
  neither `/` nor NUL, and is unique in the listing. Omit every entry in an ID
  collision rather than choosing one or minting a replacement; prove empty,
  message-first, metadata-later, repeated-metadata, invalid-ID, and duplicate-ID
  Rollouts do not block unrelated valid entries.
- [ ] 3.4 Test discovery of active, terminal, and unterminated Rollouts across
  Process exit and Agent Runtime Service restart.
- [ ] 3.5 Complete quarantine recovery before exposing `/agent/rollouts` or
  `/mnt/agent-runtime/clone`; defer current-boot quarantine recovery until the
  next service start, and prove no recovered Rollout can appear during a
  launch-correlation handshake.
- [ ] 3.6 Test that `/agent` holders can read but not mutate Rollouts and that
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
