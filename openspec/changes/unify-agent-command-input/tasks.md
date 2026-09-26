# Tasks

## 1. Planning and contract reconciliation

- [x] 1.1 Record confirmed interview decisions, ADR draft and glossary; verify links and distinguish target behavior from the current implementation.
- [x] 1.2 Produce proposal/design and owning deltas, including old task-lease, bash-request and generation-only conflicts; verify complete modified requirement blocks preserve unrelated scenarios.
- [x] 1.3 Final shared-understanding confirmation received on 2026-09-24; ADR-0058 and disposition record accepted direction, with runtime implementation still pending.

- [x] 1.4 Record the user-approved KISS revision: mature Host shell, explicit aP access, shared Host Mount authority and scoped execution-path disclosure; supersede the earlier namespace-command grammar.

- [x] 1.5 Record the accepted internal-aP refinement: task-oriented alan9 command facade and shared project file/path identity across Agent editing and native commands.

## 2. Explicit command slice

- [x] 2.1 Define the versioned file record details for submission identity, prefix intent, queue controls and completion using existing AgentFS owners; verify protocol scheduling mode stays distinct from intent and two clients cannot consume each other's results.
- [x] 2.2 Implement shared prefix framing across TUI and redirected input; verify `!`, `:`, nested prefix data, empty payloads, slash controls, pending responses, stdin EOF and multiline boundaries.
- [ ] 2.3 Reuse the native shell adapter with unchanged script bodies, selected shell/environment and Host cwd; verify pipelines, redirection, quotes, multiline scripts, PATH lookup and partial failures without command/path rewriting. Define the bounded standalone user `cd` parser and explicit errors for unsupported cd forms.
- [ ] 2.4 Dispatch user and Agent commands through the same governed native Tool Process path; verify model-free explicit execution, no authority amplification, sandbox scope limited to the current cwd grant, switching grants only through explicit `!cd`, descendant cancellation and correlated Action evidence.

- [ ] 2.5 Implement Process-owned cwd and ordered ordinary input admission; verify explicit `cd` ordering across two clients and across delegated grants, failed/unsupported standalone `cd`, script-local `cd`, per-action cwd isolation and replacement of the old busy-client rejection without weakening correlation.
- [ ] 2.6 Implement interrupt and paused-queue continuation/discard through runtime controls; verify pre-start cancellation, active cancellation, no dispatch after cancellation, pending request precedence and preserved completed effects.
- [ ] 2.7 Project Alan-captured command results into shared evidence and bounded model input; verify shared-cwd-relative path projection for `pwd`, diagnostics and captured stdout/stderr within the active grant, including Markdown emphasis without matching underscore siblings, no raw Host root or `/mnt` alias in those outputs, truncation, readable references, retention gaps, exit status and a later Agent question without an automatic summary call. Also verify native `!pwd > cwd.txt` preserves shell redirection as ordinary project data, is not output-sanitized or copied into evidence, and grants no authority through the stored path string.
- [ ] 2.8 Persist recoverable queue/cwd state through existing rollout/checkpoint owners; verify reliable pending work restores paused, unknown effects are not replayed, invalid cwd requires explicit replacement and missing records are reported.
- [ ] 2.9 Present route/cwd and truthful outcomes; verify empty-input Ctrl-D detaches only with no pending Agent input and preserves accepted work, pending confirmation/structured input remains attached and available on Ctrl-D, redirected output is clean, and missing response channels fail without hidden terminal input or fabricated rollback.
- [ ] 2.10 Run focused boundary checks, ordinary-terminal and Herdr acceptance, and `just quality`; record explicit-prefix slice evidence while documenting that unprefixed input remains Agent-routed.

- [ ] 2.11 Keep grant-to-native cwd/path resolution within ephemeral Host-adapter spawn/sandbox context while preserving logical service records; verify grant IDs/path strings cannot authorize access, raw backing-path metadata stays out of Alan-owned path fields and execution-path references in evidence, ordinary content in an explicitly delegated Host file remains user data, undelegated/private backing stays hidden and shell/model context is not rewritten to aP aliases.
- [ ] 2.12 Reconcile existing Linux reification with native path identity and macOS sandbox projection; verify read-only grants, outside-grant writes, symlink escape, rejection of mount-local executables and opaque project-code dispatchers (Git aliases, package scripts, build/test/run/generation commands) on ProtectedOnly, human escalation on weaker fallback backends, virtual-only mounts, revocation before launch and truthful degraded-backend behavior without bypassing policy.
- [x] 2.13 Inventory existing internal control operations and executable packaging; select the smallest task-oriented alan9 commands needed for real Agent workflows, specify exact invocation/help and result schemas, and implement thin aP clients with caller-scoped authority. Verify explicit discovery/invocation, no ambient broader connection, no duplicate state owner, commit errors, asynchronous acceptance versus completion and no replay of unknown effects; native `cat`/`q` lookup must not silently switch meaning.
- [ ] 2.14 Align structured project read/edit/search path parameters with Host shell cwd and paths through existing Host adapters. Verify Agent edit → native read/git diff and native edit → Agent read for the active grant; verify other grants remain available to Agent file tools and become shell-visible only after explicit cwd switching, while one shell action cannot span disjoint grants. Cover grant-relative and shared-cwd-relative Agent paths plus native cwd-relative shell paths, pending-buffer/save failure, stale-content conflict, read-only grants, symlink containment and revocation; no mirror copies or shell-text rewriting.
- [ ] 2.15 Review normal user flows and Agent command help: ordinary work requires neither aP terminology nor internal mount/descriptor/commit knowledge, while explicit developer inspection remains available.

- [x] 2.16 Implement `alan: ` / `alan! ` as presentation of canonical one-shot intent; verify empty-entry typing/paste, literal embedded prefixes, explicit `:`, empty-body Backspace, accepted/reset versus rejected/preserved drafts, history recall, pending responses, multiline/resize cursor geometry and prompt-free redirected IO.

## 3. Typed intent routing and qualification — transferred

- [x] 3.1 Transfer original implementation task 3.1 to active successor [qualify-agent-input-routing, task 2.1](../qualify-agent-input-routing/tasks.md); typed integration is not implemented here.
- [x] 3.2 Transfer original corpus/budget task 3.2 to successor task 2.2; budgets and measurement remain unfinished.
- [x] 3.3 Transfer original shadow qualification task 3.3 to successor task 2.3; no provider qualification is claimed.
- [x] 3.4 Transfer original activation task 3.4 to successor task 2.4; automatic routing remains disabled and still requires explicit activation.

## 4. Review, merge and archive readiness

- [ ] 4.1 Run strict OpenSpec validation and applicable implementation checks, complete PR review and required current-head CI for each delivery slice; record the exact reviewed and merged commits.
- [ ] 4.2 Sync only implemented and merged requirements to canonical specs; verify no automatic-routing guarantee is synced merely because the explicit slice shipped.
- [ ] 4.3 Archive only after remaining work is delivered or explicitly handed to an active successor, with canonical specs synced; verify links, disposition and implementation evidence before archive.

## Incremental implementation evidence — 2026-09-26

- PR #937 remains the unmerged explicit-input delivery slice. ProtectedOnly now
  rejects uninspected executables rather than extending a default-allow runner
  denylist; descriptor redirection retains its native shell meaning.
- Command `next_turn` preserves the full submission in the existing Machine
  queue and releases it only on explicit Turn. Command `steer` uses the existing
  Tool boundary, skips undispatched stale calls, and preserves both command-result
  identity and the parent turn across approval or rejection. Idle steering fails.
- Local `cargo test -p alan-agent-engine -p alan-tools`: 1,191 engine unit tests,
  20 architecture checks and 137 Tool tests passed; one engine test remains ignored.
- These checks do not establish tasks 2.5–2.10 as complete: end-to-end multi-client
  command/cwd ordering, durable queue/cwd recovery and real terminal acceptance
  still require implementation and evidence. Task-oriented
  alan9 controls and project-file parity also remain open. No automatic routing
  has been activated or synchronized to canonical specs.

- Runtime UI errors now retain the failing submission ID across accepted in-band
  work and generation failure. Redirected clients match only that ID, including
  failure before Tape admission; unrelated and legacy uncorrelated errors are
  not attributed to the task. Interactive admission was still lease-gated at this
  checkpoint; the later client-admission evidence below supersedes that boundary.

- Machine input queues now share one Machine-owned state; the Process pump uses
  a handle, with no second copy of accepted inputs or active submission identity.
  Ordinary follow-ups use the FIFO outer queue, while steering/responses retain
  their in-turn semantics. `queue-v1 interrupt <submission_id>`, `continue` and
  `discard` are handled through `machine/ctl`; paused work is retained through
  turn reset and is not dequeued until explicitly resumed or discarded. Unknown
  targets leave the queue unchanged. These controls are currently in-memory;
  persistence remains incomplete; later client-control evidence below covers
  renderer wiring.
- Queue-owner/control verification: 1,194 engine unit tests and 20 architecture
  checks passed (one engine test remains ignored), including real AgentFS control
  writes and correlated discard errors. This is not terminal or restart acceptance.

- Activity v2 now publishes active submission identity/intent, ordered pending
  identities and queue-paused state from the Machine. The Process input pump
  serializes activity snapshots/events so transition writes cannot overwrite
  newer queue observations. Heartbeat retains the start timestamp. The later
  client-admission slice consumes these identities; the schema/projection alone
  does not establish multi-client terminal acceptance or durable recovery.
- Activity projection verification: 1,495 tests passed across engine, protocol,
  AgentFS and TUI (one engine test remains ignored), including v1 decoding,
  identity/intent projection, pause retention and serialized activity publication.

- Redirected activity consumers now match v2 active/pending IDs and preserve a
  task's settled state when a different client begins work. UI state alone never
  supplies a final result. Cancellation targets the accepted input UUID even
  before execution and reports request submission rather than claiming stopped
  effects. All 154 TUI tests pass, including queued cancellation through AgentFS.
  That checkpoint preceded the interactive client-admission slice below.

- Interactive clients now retain their submission ID for activity acceptance,
  targeted interruption and reconnect history. Identical prompt text from another
  client does not satisfy recovery. The shared activity reducer also stops at the
  submitted input's settled event instead of adopting subsequent clients' work.
- Removed the channel task lock and busy/paused Root Agent admission rejection
  from both interactive and redirected clients. New submissions require activity
  v2; retained v1 history remains readable, but an older running Host must restart
  before accepting new work. PID pinning and attach retries remain intact.
- Client-admission verification: all 156 TUI tests pass, including concurrent
  interactive/redirected writes through real AgentFS while another input runs,
  own-ID queued interruption, identical-prompt reconnect and Root PID races.
  `just quality` passes. These tests establish client admission and correlation,
  not full multi-client command/cwd ordering, durable recovery or terminal/Herdr
  acceptance; tasks 2.5, 2.6, 2.8–2.10 remain unchecked.

- Interactive `/continue` and `/discard` now write the existing versioned Machine
  controls. Completion/help and a persistent queue count expose paused work;
  active-input settling and pending requests take precedence. The renderer does
  not mutate queue state on successful writes or claim effects were rolled back.
  Correlated cancellation failures clear the renderer's pending interrupt target.
- A running Process-loop test submits three inputs through AgentFS, cancels the
  first during generation, then verifies both remaining IDs stay paused. Continue
  dispatches them in accepted order; discard emits per-ID errors without extra
  generation. This supplements the file-control and queue-owner unit tests and
  does not establish native command/cwd ordering, descendant process cancellation,
  restart recovery or real terminal/Herdr acceptance.
- Fixed the macOS CI failure observed on `7df09b62`: the bounded child-result
  collector now waits for its submitted ID and excludes idle admission snapshots.
  Delegated supervision waits for the Process result/exit instead of inferring
  successful completion from UI Idle. This prevents empty premature results in
  both readers of child completion evidence.
- Queue-control/child-result verification: 1,473 tests passed across engine,
  architecture, Service Manager and TUI; one engine test remains ignored.
  `just quality` and strict validation of this change pass.
  Regression coverage includes the original child-Process execution/cleanup test,
  idle admission not being terminal and live delegated Processes not completing
  merely because UI history contains Idle. Current-head remote CI and real
  terminal acceptance remain required before merge.

- Queue recovery now uses acknowledged, versioned records in the existing rollout
  writer before input acceptance and execution. Pending IDs, intent, payload and
  order recover paused; formerly active IDs report unknown outcomes without
  replay. Continue/discard are persisted before publishing their results. Missing
  records remain visible across repeated recovery; malformed records cannot
  silently resume pending work.
- The checkpoint includes the logical Process cwd. Successful standalone cd waits
  for that checkpoint before completing. Recovery revalidates the logical path
  against live Host Mount authority and retains an invalid path on failure, so
  cwd-dependent effects cannot fall back to the launch directory or another grant.
  Pending work without cwd evidence is rejected. Process bindings remain the live
  cwd owner; checkpoint fields are durable snapshots, not another mutable owner.
- Durability verification includes disk-backed repeated Machine recovery, a running
  Process that waits for explicit continuation before generating, and denied cwd
  recovery through the Tool Process authority boundary. Full engine regression
  passed (1,202 tests plus 20 architecture checks, one ignored); final recovery
  checks include the additional missing-cwd regression. Task 2.8 remains open for
  combined native-command restart acceptance and the remaining interruption edges.
- Remote checks for `9a742b17` all passed, including macOS Test Suite. This evidence
  predates the queue/cwd recovery commit; fresh current-head CI remains required.

- Added a real Host/Host Mount/native-shell integration check with two independent
  aP clients. It verifies standalone cd ordering, quoted pipeline output, multiline
  redirection/append, script-local cd isolation, failed cd preserving the shared
  directory, correlated failure evidence, native cancellation preserving completed
  writes, paused pending inputs and explicit continuation. All commands remain
  model-free after the initial mount-request turn.
- This check exposed an intermittent lock-progress failure: polling input execution
  inside the Process pump could suspend a service-lock holder while the pump waited
  for that same lock to publish activity. One abort-on-drop Tokio task now advances
  the accepted input independently; RuntimeLoopState returns to the Process pump
  on completion and the Machine remains the sole queue owner.
- Concurrent finite AgentFS reads could combine document versions (including invalid
  activity JSON). A fid now retains one finite-file read until offset zero starts
  another; streams keep their offset/live-edge semantics. A chunked-read regression
  proves concurrent snapshot updates cannot append a different document's tail.
- Native asynchronous shell execution now owns a process group, with cleanup on
  cancellation, timeout and completion. The cancellation regression checks that the
  shell and its sleeping descendant group disappear. Linux reified execution uses
  a different blocking runner and still needs its cancellation acceptance.
- Verification so far: 1,204 engine tests plus 20 architecture checks passed (one
  engine test ignored), all 69 AgentFS tests passed, and the previously intermittent
  real Host scenario passed ten consecutive runs after the fixes. Complete
  OS Host, Service Manager and TUI suites also passed. Current-head CI and the
  remaining acceptance cases are still required before merge.

- Linux reified runtime execution now uses the same cancellable native process
  group owner as other shell backends. Only trusted capability probes and setup
  preparation use a blocking worker; cancelling the runtime future cannot leave
  a user command executing on that worker. Synchronous probe APIs retain their
  existing timeout and error behavior.
- Real Linux acceptance exposed an older setup failure: anonymous stdout/stderr
  pipes cannot be bind-mounted at the reified standard-device paths. Runner
  captures now use files under its private (0700) temporary parent, preserving
  stdout/stderr and exit status without exposing Host `/proc` in the command view.
  Native commands also explicitly receive stdin EOF, matching their previous
  noninteractive capture behavior rather than inheriting the caller terminal.
- Cross-compiled engine tests with `cargo zigbuild -p alan-agent-engine --tests
  --target aarch64-unknown-linux-gnu` and ran the resulting executable in the
  existing Linux arm64 `postgres:18` container, with no network and the container
  seccomp filter disabled to permit unprivileged namespace syscalls. All ten
  `linux_runner_` tests passed without smoke-test skips. Evidence includes real
  mount/read-only/network isolation, timeout, cancellation after command start,
  no delayed descendant write, captured stdout/stderr and exit code 7. The common
  native process-group cancellation test also passed with Docker's init reaper.
- The macOS engine suite passed (1,204 tests plus 20 architecture checks, one
  ignored), and the real two-client Host integration passed again after the IO
  changes. `just quality` passed. Task 2.12 still needs its remaining full command
  corpus and policy/revocation acceptance; these checks do not establish every
  explicit-slice requirement or ordinary-terminal/Herdr acceptance.

- Project-file acceptance now exercises real structured write/edit followed by native
  cat and Git diff, native overwrite followed by structured read, and rejection of
  a stale structured edit without losing native changes. Both paths use the same
  shared cwd backing. Git diff explicitly disables external diff/textconv helpers,
  preserving the ProtectedOnly gate on opaque configured executables.
- Added the minimal `agent_work` executable and existing Tool-package metadata:
  status, submit, cancel, continue and discard delegate to caller-visible AgentFS.
  Boundary checks cover versioned submission receipts, read-only denial, invalid
  targets/IDs, unavailable targets and the mounted-executable dispatch gate.
  The real Host test discovers and invokes status through the governed model Tool
  Process path and verifies its correlated Action result. Host client attachments
  do not gain an additional Process launch surface.
- Tasks 2.13 and 2.14 remain open for uncertain-commit acceptance and remaining
  cross-grant, buffer/save, revocation and containment cases. These checks do not
  establish ordinary-terminal or Herdr acceptance.
- Verification for this slice: the Service Manager suite passed (93 unit tests and
  two integration checks before adding the new boundary regression); the added
  command boundary regression and expanded real Host scenario passed separately.
  Strict change validation and `just quality` passed. All remote checks passed for
  preceding commit `ce2d2793`; the new slice still requires current-head CI.

- Completed task 2.13 with an acknowledgment-loss regression: AgentFS commits the
  actual input, its clunk acknowledgment fails, and `agent_work` returns nonzero
  with the same submission ID and unknown-delivery diagnostic. Exactly one input
  reaches the stream; queue controls likewise do not retry the failed acknowledgment.
  Together with caller-namespace denial, executable discovery and real governed
  Tool invocation, this covers the chosen minimal work-command facade. Memory and
  service wrappers are not introduced without a concrete workflow.
- Recovery audit found that command steering temporarily replaced the parent input
  identity without including it in the queue checkpoint. The Machine now retains
  that suspended identity during the command boundary. Restart reports both IDs
  as unknown, never replays either, and a cancellation targeting the parent still
  reaches the active turn. Disk-backed recovery and existing steering continuation
  regressions cover this change; combined native-command restart remains open.

- Completed task 2.16 against the current composer/renderer/redirected-IO contracts:
  empty-entry keyboard/paste prefixes, literal embedded prefixes and explicit `:`,
  Backspace reset, accepted reset versus rejected draft/intent preservation,
  history recall, pending responses, wrapped multiline cursor geometry and resize,
  and redirected one-shot framing without prompts. All 160 TUI tests passed.
  This marks the presentation contract only; task 2.10 still owns actual terminal
  and Herdr acceptance.
- Current regression: 1,204 engine tests, 20 architecture checks, 95 Service Manager
  unit tests and its two integration checks passed; one engine test remains ignored.
  Herdr environment probe returned `NOT_IN_HERDR`; its skill prohibits controlling
  the focused session from outside Herdr, so that acceptance remains outstanding.
- `just quality` and strict change validation passed after the recovery/facade
  changes; the commit hook rechecks the final staged presentation assertions.
- Expanded the real native Host Mount adapter acceptance to prove structured file
  reads can access a second delegated read-only grant while native shell access is
  limited to the selected cwd grant. Selecting that grant permits native reads but
  still rejects writes. Structured writes also reject the read-only target.
- Undelegated symlink targets reject structured reads/writes and preserve original
  bytes. Saving over a directory reports failure and preserves the directory.
  After revocation, refreshed file access fails, and the real Tool Process launch
  boundary rejects the stale cwd rather than executing in the remaining writable
  grant. Existing native/structured round-trip and stale-edit tests remain the
  integration evidence for shared backing files. Task 2.14 still retains its
  remaining buffer/visibility audit instead of treating this boundary test as proof
  of a pending project-buffer save feature.
- All 38 OS Host unit/integration/architecture checks passed with the expanded
  boundary scenario, including the two-client native command/file round-trip.
  Strict change validation passed; the commit hook runs the full quality gate.
- Ordinary PTY acceptance ran the actual file-backed TUI against an isolated real
  Host/native mount and a mock provider (no existing user Host or credentials).
  It verified `alan!` command presentation, native write/read, `:!` remaining Agent
  text, Ctrl-C with a persistent paused queue, and Ctrl-D detachment while a native
  command continued and wrote its result afterward. Root PID remained unchanged
  after the fix below; explicit commands added no model calls. This verifies PTY
  interaction, not Terminal.app or Herdr visual acceptance.
- PTY acceptance exposed an intermittent real Root replacement. The supervisor
  interpreted a nonblocking Process-table lookup's `None` (also returned on lock
  contention) as completion, then could terminate a still-running Root. Removed
  that redundant inferred-exit path: only observed Process exit drives restart.
  Root shutdown waiting likewise requires positive exit evidence. A concurrent
  Process-reader regression failed before the fix (Root PID 8 became 0) and passed
  afterward; all 96 Service Manager unit tests and two integration checks passed.
- Terminal.app UI access was rejected by the computer-use tool. Herdr remains
  unavailable from this non-Herdr task context under its skill's entry requirement.
  Neither restriction was bypassed. Temporary PTY harness and logging dependency
  changes were removed from the repository after capturing evidence.
- Root-contention fix verification completed with `just quality`, strict change
  validation and the real PTY replay. The temporary Host exited after evidence
  capture; no acceptance Process remains running. Current-head CI and the remaining
  terminal/Herdr, native restart and full closure audit are still required.
- Cancelled explicit commands without a Tool result now retain a correlated Action
  with a user-facing interruption message and preserve completed effects. The real
  two-client Host regression checks this message alongside paused pending input,
  preserved early writes and absence of late writes.
- Root restart audit found that Service Manager currently discards runtime startup
  rollout metadata and relaunches the unchanged Root template without a recovery
  source. Engine recovery tests therefore do not establish Root or Host restart
  recovery. Task 2.8 remains open for wiring that source through the existing durable
  owner and verifying native-command restart end to end.
