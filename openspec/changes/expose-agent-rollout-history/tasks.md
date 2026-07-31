## 1. Durable Background Launch

- [ ] 1.1 Add clone-via-open `/mnt/agent-runtime/clone` to Agent Runtime
  Service. Pin the current `/agent/root` Process as the parent, allocate the
  pending Process through its `/proc/clone` context, accept one
  `AgentExecutableRequest`, and return the ordinary PID without adding another
  launch identity.
- [ ] 1.2 Give the Local Entry Shell Process an ordinary `/agent` handle and
  no `/mnt/agent-runtime`, then give the authorized renderer a distinct
  attachment view which overlays reserved `/agent` capacity and adds the
  launch capability. Keep `/proc/self/namespace` tied to the underlying Shell
  Process namespace; do not publish the capability in `/srv`, add it to
  `/agent`, or retain it in any child Process namespace. Prove the renderer can
  reach it while ordinary Shell children and delegated Agent Processes cannot.
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
  claim terminal outcome persistence. Prove a request rejected before commit
  is a definite failure, while missing correlation after a successful or
  ambiguous commit is reported as indeterminate and never automatically
  retried.
- [ ] 1.7 Prove a retained Rollout with the same PID path from a prior Host
  boot is excluded by the pre-spawn Rollout-ID listing.
- [ ] 1.8 Pin and revalidate `/proc/host/boot_id`, and prove a Host restart
  during post-commit correlation produces an indeterminate launch outcome,
  does not match a new-boot Rollout, and does not automatically retry.

## 2. Terminal Rollout Evidence

- [ ] 2.1 Extend the generic `ProcessRunner` bridge with default no-op
  terminal-finalizer preparation and execution hooks. Prepare one
  per-Process finalizer from the committed invocation before the Process is
  visible as running or accepts `ctl`, and invoke it with the intended numeric
  exit code. Allow the finalizer to return one opaque, default-absent post-exit
  cleanup action.
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
  `RuntimeController` (or equivalent runtime-task guard) and deferred AgentFS
  cleanup action, or an explicit no-producing-Rollout outcome that still
  carries any live runtime owner and cleanup for AgentFS already bound; a
  pre-dispatch outcome carries neither, and a cloned `RuntimeHandle` alone is
  insufficient. Keep any eventual `AgentExecutableResult` only in the
  candidate runner outcome. Prove control during the delivery window, a
  best-effort in-memory runtime after Rollout creation failure, and failure
  after Rollout creation but before readiness all finalize correctly. Create
  at an internal staging path,
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
  await the terminal-context barrier. For every outcome with a live runtime
  owner, request Agent Machine quiescence that cancels or drains both ordinary
  transitions and deferred runtime actions; for a producing Rollout, then
  await a writer fence covering every Rollout producer. The normal run path
  SHALL hand off its live runtime and cleanup ownership instead of calling
  `RuntimeController::shutdown`, dropping the task owner, or cleaning up before
  finalization. When a Rollout exists, append and flush terminal
  `process_exit`; then shut down and release any runtime task and return the
  deferred AgentFS cleanup action while consuming the terminal context exactly
  once. Have Kernel publish terminal `/proc` state before invoking that action
  to unbind `/agent/<pid>` and release Process-scoped AgentFS backing. Test
  immediate control before metadata delivery, explicit no-Rollout startup
  retaining AgentFS until exit publication, normal result publication followed
  by successful finalization while the runtime remains live,
  controller-drop regression, active-transition cancellation,
  `/proc/<pid>/ctl` exit during an active deferred action, deadlock-free barrier
  and fence completion, no post-exit append, and control exit code `130`.
- [ ] 2.7 Bound Agent terminal finalization under one fixed absolute deadline
  while reserving its final interval for containment. Stop context barrier,
  quiescence, writer fence, terminal append/flush, and pre-exit runtime shutdown
  work at the earlier containment cutoff. On error or timeout, emit a structured
  PID/Rollout/exit-code/stage diagnostic, cancel logical writer and runtime
  owners without awaiting stuck Host I/O, remove the entry under the
  discovery-table lock used by open, atomically quarantine the backing inode
  within the reserved interval before Kernel may publish exit, and finish
  non-blocking logical-owner release. If containment errors or reaches the
  absolute deadline, call the synchronously non-returning Alan OS Host
  lifecycle adapter without awaiting the stuck operation. Recover only after
  ownership ends and validation succeeds under the same Rollout ID. Test
  an earlier stuck writer blocking the fence, disk-full/flush-error, timeout,
  ambiguous-flush no-retry, a complete record surviving an error, absent/torn
  outcomes, stale I/O affecting only quarantine, containment failure as
  bounded Host-fatal termination, cleanup, and Host-shutdown progress.
- [ ] 2.8 Preserve Rollouts without `process_exit` as unterminated evidence
  without fabricating a result. Treat any discovered complete valid
  `process_exit` as authoritative even when its original flush reported an
  error.
- [ ] 2.9 Add an internal fatal-transition adapter owned by `alan-os-host`.
  Inject its synchronously non-returning call into Agent Runtime Service during
  Host boot. On invocation, atomically close in-memory readiness, attachment
  admission, and Service Manager new-work admission, request Service Manager
  shutdown, and enter immediate fail-stop termination without awaiting storage
  or shutdown completion. Abort within the adapter if its internal signal
  cannot be delivered. Prove the call never returns to terminal finalization or
  Kernel, existing and new attachments cannot submit more work, and no Host
  command, aP file, or renderer API exposes the adapter.

## 3. Rollout Discovery

- [ ] 3.1 Enumerate retained Rollouts from Agent Runtime Service's existing
  System Store subtree using the existing Rollout loader, with no persistent
  history index.
- [ ] 3.2 Reserve `/agent/rollouts` and expose each valid retained Rollout as
  one read-only `/agent/rollouts/<rollout-id>` JSONL file while preserving
  numeric PID entries and `/agent/root`. Enforce append-only published Rollout
  backing: writers and the Host adapter may append but never overwrite or
  truncate a published prefix. Build an in-memory discovery table by validating
  each retained source once at startup; advance a live source's approved length
  only after an owned complete append passes envelope validation. On open,
  capture its pinned read-only descriptor and approved length from that table
  without rescanning. Bind quota-scoped `/agent` handles with named fixed
  per-handle caps, a fixed ordinary pool for every Process namespace, and a
  separate authorized-renderer attachment pool whose capacity exceeds one
  handle cap. Make inherited delegation share its account. Reserve handle and
  pool slots, then capture source identity and approved length under the same
  discovery lock used by containment removal. Release slots on failure or
  clunk. Before allocating
  read scratch or result storage, non-blockingly acquire per-handle and
  corresponding-pool in-flight read permits; reject immediately rather than
  queue when either limit is reached, and release permits on success or error.
  On every permitted read, fetch only the protocol-bounded requested range
  through the pinned descriptor and never read beyond the approved length.
  Make storage work and scratch/result memory proportional to that range.
  Ignore later appends and fail if the descriptor is unreadable. Impose no
  Rollout-size limit or full-file allocation. Prove large valid Rollouts remain
  fully readable without whole-prefix work per range read, later active or
  quarantined appends are not exposed, reopen can observe later complete
  records, tiny and out-of-range reads do not trigger a complete-prefix scan,
  concurrent reads through one fid remain bounded, ordinary Processes cannot
  exhaust renderer capacity, ordinary Shell children inherit neither the
  reserve nor `/mnt/agent-runtime`, and excess opens or reads fail with
  resource exhaustion without changing evidence.
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

- [ ] 4.1 Run focused Alan OS Host lifecycle, Agent Runtime Service, AgentFS,
  Rollout, and namespace tests, then run `just quality`.
- [ ] 4.2 PR review confirms there is no new execution identity, persistent
  index, retention policy, notification protocol, Host API, or renderer-owned
  state.
- [ ] 4.3 After implementation merges, sync `agent-rollout-history` into
  `openspec/specs/` and move the change to
  `openspec/changes/archive/YYYY-MM-DD-expose-agent-rollout-history/`.
