# Tasks

Implementation is in progress in the local checkout and is not shipped. Each
slice still requires focused checks, user-facing help, and current-head
review/CI evidence before it can be called shipped. Existing runtime
requirements remain in `unify-agent-command-input`; linked dependency work is
not a second implementation owner.

## Current status — 2026-09-30

- G1 live acceptance now passes on the diagnostic-free candidate with SHA-256
  `3a8bc9d68f96d52c75d05d289e390af3975724f279152b4e1db0f1c1e6ea945d`.
  Both `/tmp/alan-g1-herdr-full-20260930.py` and
  `/tmp/alan-g1-pty-full-20260930.py` completed fresh bare startup, visible
  read-write project authorization, actual Agent `read_file`/`edit_file`, exact
  `grep -Fx` check, real `git diff`, approved bounded sleep, acknowledged Ctrl+C
  cancellation with child termination, explicit `/continue`, successful `pwd`
  correction, and natural exit. External file and Git checks agree. No runtime
  directory, hidden project grant or sandbox bypass was injected. Unknown sleep
  capability required ordinary explicit approval. Stage ANSI artifacts are
  `/tmp/alan-g1-{herdr,pty}-{agent-edit,focused-check,real-diff}-20260930.ansi`;
  the PTY whole transcript is `/tmp/alan-g1-pty-20260930.ansi`.
- Correlated AgentFS probes established the shared scheduling cause: Engine task
  `Id(12)` left its execution walk queued on the FIFO state mutex, selected an
  observer branch, then awaited another walk on the same mutex without polling
  the execution again. Evidence is `/tmp/alan-herdr-agent-task-77724.log`;
  earlier path-stage evidence is `/tmp/alan-herdr-correlated-75900.log`.
  Execution and its control/heartbeat observer now run concurrently in both
  ordinary and deferred execution. Admitted observer writes settle before final
  UI publication. All probes are removed. A deterministic FIFO-contention
  regression passes, all 24 Engine loop tests pass, Agent Engine library tests
  pass (1222 passed, one ignored), and `just quality` passes including standalone
  distribution. Herdr interruption acceptance additionally passed five fresh
  consecutive runs after correcting transient/literal harness assertions.
- G1 and G2 live behavior are qualified locally; G3, full baseline review, merge
  and current-head CI remain unpassed. The historical failed attempts below remain
  evidence of the previous candidate, not the status of this new candidate.
- G2 is running in the explicitly approved Herdr checkout
  `/Users/morris/.herdr/worktrees/alan/self-development-g2-20260930`, starting
  from local snapshot `bb305940e9a44ca09b28a077976b5ffb741ff325` with the G1
  candidate retained. Alan authored the neutral cancellation presentation in
  the shared live/reattach path; no operator supplied or edited its source patch.
  Independent Standards/Spec review found test placement, a removed negative
  correlation observation and helper-only reconnect coverage gaps. Alan restored
  the observation and extracted the full existing suite to `interrupt_tests.rs`;
  its focused interrupt run passed eight tests. Real reconnect coverage now
  exercises `reattach_to_current_agent` and passes. An independent generated copy retains the exact starting
  production prefix and Alan's exact test file (SHA-256
  `fd18a20b35190c87208baf0d9b2bbd1d928b8f52190a1c2b488d4d85188d711f`):
  the matching-completion test actually fails its Rendered assertion on the old
  implementation, exit 101, rather than failing compilation. Evidence is
  `/tmp/alan-g2-independent-red-20260930.log`; the same check passes on the fixed
  tree in `/tmp/alan-g2-independent-green-20260930.log`. Alan ran all 154 TUI tests,
  formatting and diff checks successfully. Both independent review axes have no
  remaining findings, pinned to tracked diff SHA-256
  `42ab69b9ad3cb1685ba40e12928db5279c8680355234bd5ef1583951b49f42df`
  and new test file `2899a6ae1f85dec0244a53058f96de6ed476e00d686585a0b2053bb397df4998`.
  `just quality` passes including standalone distribution; log is
  `/tmp/alan-g2-quality-20260930.log`. Its fresh release candidate SHA-256 is
  `22a484b4c01880011c7c5964a3b1f76ab4d1bd02ea8b51ce5ad96a94856aef69`.
  Herdr PID 74594 visibly authorized this checkout, started real sleep child
  74708, acknowledged Ctrl+C with ordinary `Input cancelled` and child termination,
  then explicitly resumed, returned actual project cwd with exit zero and exited
  naturally to fish. ANSI evidence is `/tmp/alan-g2-relaunch-{cancel,correction}-20260930.ansi`.
  The harness first matched a stale ready row after sending correction; a separate
  exact-result read verified the same invocation before natural exit, without
  restarting or rescuing it. Alan's reviewed patch is committed locally as
  `ce6bc0e9f3c23ee6ac8c1160d7c7d89a9e05dae7`. G2 passes locally;
  G3, current-head CI and merge do not.
- At the user's explicit request, the dev `chatgpt-main` Connection model now
  selects `gpt-6.1-sol`; its other settings and credential binding are preserved.
  Subsequent supervised runs explicitly set
  `ALAN_CONFIG_PATH=/Users/morris/.config/alan/self-development-agent.toml`, which
  selects that profile and `model_reasoning_effort = "medium"`. A fresh Herdr
  invocation returned `MODEL_OK` and its durable Machine metadata at
  `rollout-20260930-113629-8f32b38e-ca4c-4da1-805b-a4b3328ce2d3.jsonl` records
  model `gpt-6.1-sol` and effort `medium`. G1/G2 evidence retains its original
  gpt-6-luna identity; this setting change does not retroactively requalify it.
- Full-candidate independent review is pinned to
  `ce6bc0e9f3c23ee6ac8c1160d7c7d89a9e05dae7` against original baseline
  `660253fa1ff0557dc1f2ef40fb0acefdcb5d1ae2`, diff SHA-256
  `dd15ef51fa405fe455e523197a7831dd170b7eaf3fd2ac2711597807e2872d285`.
  Five findings remain before delivery: Ctrl+C must consider admitted pending
  submissions before Running arrives; externally revoked project grants must
  invalidate project/candidate UI; provider bootstrap failure must not label an
  unbound configured model effective; `/project` remediation belongs in the
  interactive adapter rather than shared Engine errors; the enlarged Host Mount
  inline suite must be extracted intact. Alan is addressing the keyboard routing
  finding first. Normal-path live G1/G2 evidence does not qualify these boundaries.

- Corrected the live-test executable selection: the September 30 build outputs
  `target/quality-gate/debug/alan`; the explicit-target executable under
  `target/quality-gate/aarch64-apple-darwin/debug/alan` was a September 29 build.
  Earlier runs of that path do not qualify the newer source.
- Removed all temporary Root Agent/ProcFS lock guards and walk/response traces.
  The clean diagnostic-free candidate builds successfully and has SHA-256
  `3cffb78f2399309447d35396a4096013d94577db11750df27b69e67fc4a40909`.
  The ordinary PTY harness at `/tmp/alan-g1-pty-20260930.py` passed fresh bare
  launch, explicit read-write project authorization, cwd setup, tool approval,
  real `sleep 30` child startup, Ctrl+C cancellation and child termination,
  explicit `/continue`, and a successful `pwd` correction returning ready.
  ANSI evidence is `/tmp/alan-g1-pty-20260930.ansi`. This covers the interruption
  subset, not the complete ordinary-terminal G1 read/edit/check/diff scenario.
- The same candidate in Herdr 0.9.1 pane `w58:pA` (PID 57629) accepted project
  selection and reached the tool approval request. Numeric approval input
  visibly changed the UI to `sending response`, but no sleep child started;
  Ctrl+C did not reach an acknowledged paused state within eight seconds.
  The process was sampled and then stopped with TERM. Evidence is
  `/tmp/alan-herdr-g1-clean-20260930.sample` and `.ansi`. Herdr G1 remains failed.
- Targeted request-tag probes subsequently confirmed intermittent server-side
  unreturned walks at `/agent/8/machine/ctl` (tag 1380),
  `/agent/root/requests/r0/kind` (tag 253), `/agent/root/requests` (tags 184/252),
  and `/agent/8/actions/a0/name` (tag 122). The Host received those requests while
  unrelated Service Manager PID reads continued. Keyboard delivery is therefore
  not sufficient to explain all failures. One instrumented Herdr run completed
  approval, cancellation, child termination, explicit resume, correction and
  natural CLI exit; subsequent identical runs failed. This is intermittent
  evidence, not a qualified Herdr pass.
- A single-variable experiment replacing only the AgentRoot registry mutex
  with a standard mutex still failed, so it was reverted. A standalone Unix aP
  live-tail/request-refresh/control prototype passed 64 rounds and does not
  reproduce the production failure; it is retained only at
  `/tmp/alan-ap-live-request-prototype-20260930.rs`, not as a claimed regression.
  All temporary tag/walk probes and lock experiments have been removed again.
  Preserved traces include `/tmp/alan-herdr-ap-tags-failed-65947.log`,
  `-66607.log`, `-68355.log`, `-71011.log`, and the exact PID 71011 sample at
  `/tmp/alan-herdr-root-mutex-experiment-71011.sample`.
- After cleanup, `cargo build --locked -p alan --target-dir target/quality-gate`,
  `cargo fmt --all -- --check`, and `git diff --check` pass. The final rebuilt
  clean executable has SHA-256
  `2db33ac013eabf070e0e8008909beef24137403fc033c939d0a3a182cb2efd8f`;
  it has not yet undergone the complete live G1 scenarios. All 47 Alan aP tests
  pass, which does not qualify the intermittent production walk behavior.
- G1, G2 and G3 remain unpassed. Self-development remains gated on the complete
  ordinary-terminal and Herdr G1 scenarios, independent review and CI.

## Earlier status — 2026-09-29

- Source HEAD is `660253fa1ff0557dc1f2ef40fb0acefdcb5d1ae2`; the checkout has
  uncommitted Alan Shell, Host, and OpenSpec changes. The TUI, foreground Host,
  and project-mount modules have been split to satisfy the source-size policy.
- The earlier full `just quality` run passed after those splits, as did focused
  terminal UI, Host lifecycle, Agent Engine cwd-binding, and Alan CLI tests.
  After moving Root Agent PID polling out of the event loop,
  `cargo test --locked -p alan-terminal-ui` passed (153 tests) and `just quality`
  passed again. The fresh candidate SHA-256 is
  `6de169e8d4c0b9fcac505745418e6c02b60dbded8b4432b6ec70e6158d5f2f0c`.
- A same-source ordinary-terminal PTY attempt reached explicit project
  selection and a tool approval prompt. After approval, the UI remained at
  “sending response”; no bounded `sleep` child started. Ctrl+C was parsed and
  dispatched to `/agent/8/machine/ctl` with submission
  `2814a5ac-d87a-4529-a119-33e1a06166d7`, but the control write did not return.
  The exact test Alan process was terminated; its temporary runtime directory
  was removed. This is a current G1 blocker, not a passed cancellation test.
- A fresh run of that candidate showed the correct `gpt-6-luna` status and bare
  `: ` / `! ` prompts. Its bounded `sleep 60` approval remained at “sending
  response” with no child process. A sample showed the main event loop available
  but a Root Agent PID `Shell.cat` waiting in an aP Unix-socket write; another
  call waited on the same writer. The exact blocked request is not identified.
  Printable input reached the composer; Ctrl+C did not complete or visibly
  acknowledge cancellation. The candidate was stopped after sampling. This is
  unresolved transport evidence, not a response-write or cancellation pass.
- The same fresh candidate in Herdr displayed `model gpt-6-luna`. `! sleep 60`
  reached approval; pressing `1` showed “sending response”, while rollout
  `ccc4171d-70fa-47a2-b6d4-d02fd0c32c0e` remained at `escalation_required`,
  with no confirmation, engine activity, or sleep child. Ctrl+C had no visible
  acknowledgement. A sample found Host writing an aP response frame over the
  local socket but could not correlate that frame to approval. The exact test
  PID was stopped with TERM and the pane returned to fish. G1 remains unpassed.
- G1, G2, and G3 remain unpassed. No independent slice review, merge, or
  current-head CI evidence is recorded. Do not use Alan for self-development
  until the ordinary-terminal write/approval and cancellation path is fixed and
  both required G1 host runs pass.
- A repeat in Herdr passed explicit read-write project approval, exact fixture
  read/edit, fixed exact-line grep, and real diff. Its bounded `sleep 60` never
  started: pasted `approve` remained a draft, and Herdr `pane send-keys` for
  printable input, Ctrl+U, Esc, and Ctrl+C had no observable effect even while
  the pane was focused. Only that test PID was stopped with TERM. This is not a
  passed cancellation/correction case; verify key delivery in ordinary PTY and
  Herdr before G1.
- The two-line status now receives the Root Agent model selected after
  Connection profile metadata resolution and retains explicit `unknown` when
  no model is supplied. Focused TUI and Service Manager tests pass; do not mark
  task 2.4 complete until the fresh candidate is checked at required widths and
  through Herdr.

### Second self-authored task — keyboard interruption follow-up

- Alan's three-file patch against `ce6bc0e9` routes Ctrl+C using the existing
  pending submission queue before the Running UI event arrives. The regression
  exercises real keyboard dispatch, targeted control, chooser cancellation and
  completion back to idle. Independent TUI verification passed 155 tests;
  `/tmp/alan-g3-independent-tui-20260930.log` retains the result. Both review
  axes examined diff SHA-256
  `f4a483a5fdfdc2472e50a382424935c21c7a9a2951dbcafb262b9b839f1d6419`;
  the before-Running interruption finding is closed. Four whole-candidate
  findings remain open. `just quality` also passed, including standalone
  distribution verification; `/tmp/alan-g3-quality-20260930.log` retains the
  complete gate output. The one-line queue dispatch helper received a
  non-blocking simplification suggestion, not a standards violation.
- Fresh build/relaunch, recovery without replay and CI remain pending; this is
  not a G3 pass. On continuation the operator environment lacked
  `HERDR_ENV=1`, so Herdr control was stopped under its skill contract. The
  existing Alan process and its authored patch were preserved, with independent
  local validation continuing.
- The user subsequently confirmed the Herdr-managed pane and explicitly asked
  to continue after the environment-check explanation. The operator resumed
  only the recorded `w5E:p1`, verified its cwd/process identity, observed Alan's
  final ready report and unchanged reviewed diff, and requested normal `/quit`.
  Herdr confirmed the original fish shell returned; no process rescue occurred.
- The reviewed keyboard patch was committed as
  `28f16b3bb736e1ced490c0ee750679f69049ae67`; its commit hook passed the full
  quality gate. The freshly built release SHA-256 is
  `670b1fbec8b3ad0061294f7f99bb70c58bf05c0278cf168a0a4bfe83c767395d`.
  A new bare invocation in the same pane (PID 18457) required fresh visible
  read-write project selection and executed `!pwd` with exit 0. The effective
  model repair and the user's cursor report are subsequent separate tasks.
- Alan's model repair changed only Service Manager, its adjacent boot tests,
  OS Host and CLI projection. Both review axes closed configured-but-unbound
  model on four-file diff SHA-256
  `d6d7db2070cb35e15270fda59a9c8130af154d2432f71fa83c6537fd971e31c3`.
  Its real red returned configured `gpt-5.4` instead of null; the repaired
  unavailable and successful-profile checks passed. Independent Service Manager
  tests passed 119 unit and both integration tests; independent OS Host tests
  passed 31 unit plus their integration suites. Alan's own two socket-creating
  OS Host tests were denied by its sandbox; preserve that failed execution as
  an operator-support limitation rather than count independent checks as Alan's
  self-executed success. No sandbox broadening or test suppression was used.
- The user reported cursor misplacement during this continuation. Alan reproduced
  two real `draw` failures with a nonzero native Ratatui viewport origin, then
  changed only the two cursor-coordinate expressions to add that origin. Both
  checks and all 157 TUI tests passed; independent TUI verification also passed
  (`/tmp/alan-cursor-independent-tui-20260930.log`). Independent PTY verification
  of the old binary at 40/60/80/120 columns reproduced eight incorrect cursor
  positions, including Chinese/emoji multiline drafts
  (`/tmp/alan-cursor-pty-red-20260930.log`). The PTY reports a nonzero initial
  cursor row through the normal terminal query; it does not supply a runtime
  directory, grant or provider bypass. Two preliminary harness runs failed to
  recognize the narrow prompt and are not red evidence; the final run checked
  actual cursor positions and all four invocations exited normally. Fresh
  candidate PTY green and Herdr relaunch remain pending.
- Final model review SHA-256 is
  `eb23d5e211ed24a02284447cc9d886ebb010e168783e112a1359563795ff9eea`;
  final cursor review SHA-256 is
  `1a182a16913c02c2a201bfd2b2ec2d93f18c24b60094d7b22aaae61e3f05680a`.
  Alan shortened its own unavailable-model assertion to direct `None`, preserving
  all original tests and the recorded real red, to meet the unchanged 1000-line
  source-size gate. A later model-only hook was terminated before a successful
  result; the clean, reviewed six-file slice then passed the complete hook and
  was committed as `b0e11c0f5d82ec46564e9f8b9b5d8bd760c288be`.
  Release binary SHA-256 is
  `fe8157aca85ac077659570a6215aee52c54b83d36d79d6c9d83a611f7389b062`.
  The identical native PTY checks passed all eight cases
  (`/tmp/alan-cursor-pty-green-20260930.log`); all four invocations exited
  normally. Fresh Herdr invocation PID 9075 displayed and cleared a Chinese/emoji
  draft. Raw `pane send-text` LF did not represent multiline paste; the actual
  bracketed-paste multiline proof is the PTY evidence, not that raw-key probe.
- Live G3 recovery acceptance remains failed/unqualified. Invocation 9075
  visibly authorized the project, executed the one-marker bounded command,
  started shell 11388/sleep 11389, cancelled them with Ctrl+C, displayed paused,
  and exited via `/quit`. A follow-up submitted while paused did not execute
  or show a receipt; its authoritative admission is not yet proven. Explicit
  `--resume` invocation 12679 selected the recorded rollout in a fresh instance,
  showed ready/no-project, and did not repeat the effect (marker count remained
  one; queued marker remained absent). It did not expose previous paused/pending
  state. Fresh visible authorization was needed again. Preserve these artifacts
  in `/tmp/alan-g3-recovery-{running,paused,resumed}-20260930.ansi` and the selected
  filename in `/tmp/alan-g3-recovery-selected-20260930.txt`; do not infer pending
  input preservation or G3 from no-replay alone. Alan is diagnosing admission,
  durable queue recovery and UI ownership without modifying code yet.
- Draft delivery is PR #1026 (`https://github.com/realmorrisliu/alan/pull/1026`).
  The exact remote head `5189ad3590c79507567f42e6628bbbd718aeafd4` passed all
  16 reported checks, including Linux/macOS quality, test/release and blocking
  harness workflows. The PR remains draft; three original review findings,
  G3 recovery and P3 work are unfinished. This CI evidence does not close them.
- Alan's recovery diagnosis confirmed live in-memory queue scheduling without
  durable admission/paused-queue reconstruction. It classified the particular
  follow-up's admission as unknown, not proven loss. The implementation attempt
  delegated to an ungranted worker and made no code changes; ordinary Ctrl+C
  settled that attempt before `/continue` steering Root to work directly.
  Root then failed LLM request document writes for g23 and g28. Independent
  Connection test passed; ordinary `/compact` reported success but did not repair
  the subsequent request. The underlying provider/file error is not exposed
  by the current terminal message, so no root cause is claimed. Preserve
  `/tmp/alan-g3-recovery-llm-failure-20260930.ansi` and
  `/tmp/alan-connection-test-g3-20260930.log`. With a clean source tree, the
  invocation exited normally via `/quit`; a fresh task is being prepared rather
  than treating this failed long-task workflow as G3 success. No process was
  killed or observation timeout used as a reason to restart.

## 1. P0 — Make project entry usable

- [ ] 1.1 Connect `/project` and Agent mount requests to a host-local chooser using the current invocation and existing Host Mount Service; verify approve/read-only/read-write/cancel/revoke in the bare CLI without a manually supplied runtime directory, and document the visible flow.
- [ ] 1.2 Select the Process cwd after approval and project truthful project-relative status; verify fresh launch without approval cannot access the project, revoked grants fail closed, and `!pwd` plus Agent `read_file` agree on an authorized disposable project.
- [ ] 1.3 Replace missing-project internal errors with an actionable authorization summary; verify both explicit commands and Agent Tools reach the same explanation and retain diagnostics in details.
- [ ] 1.4 Reproduce the recorded cancel → queued follow-up → continue → unresponsive-control sequence against exact HEAD, preserving submission/Action identifiers and process evidence; if reproduced, fix the shared cause under the unified-input owner and add the smallest regression that fails before the fix. If not reproduced, retain the uncertainty and run the G1 interruption cases rather than claiming it fixed.
- [ ] 1.5 Review the P0 slice, resolve findings, pass focused Host/CLI tests and required current-head CI; record actual ordinary-terminal and Herdr results here.

## 2. P1 — Complete the everyday task loop and G1

- [ ] 2.1 Render immediate authoritative admission receipts, queued previews/counts and paused choices; verify no lost or duplicate input across normal admission, rejection, cancellation and Tape reconciliation. Keep scheduling changes with the unified-input owner.
- [ ] 2.2 Wire existing channel history, authorized file candidates and installed/descriptor Skill sources at the real entrypoint; verify recall after restart, dev/stable separation, correct recalled intent and candidate removal on revocation.
- [ ] 2.3 Implement one-Enter slash execution, Tab insertion and idle Ctrl+C draft clearing with help text; place temporary candidates below the composer and keep the input/cursor anchor stable across candidate count/wrapping changes. Verify paste/Enter, multiline input, active interruption, rejected drafts, UTF-8 cursor editing, bounded menu at terminal edges and per-key anchor stability at 40/60/73/80/120 columns through focused composer tests and Herdr.
- [ ] 2.4 Add the two-line Agent prompt with actual project/cwd, model or unknown, and distinct ready/working/approval/paused/failure states; verify 40/60/80/120-column layouts, Chinese/emoji input and no fixed bottom panel or lost host scrollback.
- [x] 2.5 Pass G1 through ordinary terminal and Herdr on the same build: explicitly approve a disposable project, have Alan read/edit a small fixture, run a focused check, show the real diff, cancel a bounded task and successfully submit a correction; record toolchain/cache/network restrictions and resolve them through existing scoped policy rather than disabling sandboxing.
- [ ] 2.6 Review and merge the P1 slice after focused checks, `just quality` and required current-head CI; record G1 as passed only with the complete live transcript and no hidden operator wiring.

## 3. P2 — Demonstrate supervised self-development

- [x] 3.1 After G1 passes, prepare an isolated checkout at a recorded Alan SHA and a dev candidate executable; select one small remaining accepted defect with a clear failing check, verify relevant instructions/dispositions, and bound Alan's writable scope to the checkout.
- [x] 3.2 Through Herdr, ask Alan to inspect its own source, identify the shared cause and produce the patch plus regression check; preserve its prompt/Tool evidence and confirm no operator supplied or silently edited its patch.
- [x] 3.3 Have Alan run the focused check and inspect its diff; independently verify failing-before/passing-after, review scope and actual command exit statuses, and ensure the patch complies with repository architecture and test placement.
- [x] 3.4 Build the reviewed candidate through the governed workflow and freshly launch it in Herdr; demonstrate the corrected interaction and clean exit, retaining the known-good binary. Record source/binary identity and mark G2 passed only after this loop succeeds.
- [ ] 3.5 Complete normal review/CI/merge for the actual self-authored change under its owning OpenSpec scope; on any failed step, retain the failed-gate evidence and keep self-development unqualified.

## 4. P3 — Refine terminal presentation and repeatability

- [ ] 4.1 Preserve existing typed Tool titles/presentations through Action file projection for both user commands and Agent Tools; verify read/edit/bash results use semantic summaries rather than escaped JSON and add one end-to-end projection regression.
- [ ] 4.2 Bound summaries by physical rows/bytes and implement retained Action details with Ctrl+O, action selection and return-to-draft; verify long one-line output, true evidence truncation, missing detail and no duplicate scrollback. Update help alongside the feature.
- [ ] 4.3 Carry semantic spans through the existing history/layout path and apply the design's spacing and color roles; verify headings/lists/code/diffs, copyable indentation, light/dark/low-color terminals and resize snapshots at 40/60/80/120 columns.
- [ ] 4.4 Wire `/status` and `/model` to Connection-owned catalog/effective binding, adding any required owning provider/Connection delta before implementation; verify successful next-input binding, unavailable catalog, selection failure and unchanged in-flight bindings without hardcoded model choices.
- [ ] 4.5 Verify chosen durable recovery and paused/no-replay behavior with the active unified-input owner; keep the parked history-browser project inactive. Run a second independent Alan-authored task through Herdr and record G3, including whether any manual process rescue was needed.
- [ ] 4.6 Re-run the fixed fx/Alan comparison scenarios with exact versions and matched viewport widths; preserve visual/ANSI evidence and separate model-dependent timing from interaction quality. Review, pass `just quality` and required current-head CI for each delivered P3 slice.
- [ ] 4.7 Preserve a bounded safe provider startup reason through the existing LLMFS terminal error event and namespace/Engine failure projection. Reuse typed auth and HTTP/transport classification behind provider adapters; never publish unrestricted HTTP bodies, credentials, URLs or account identifiers. Verify startup cause propagation and unknown-sensitive-error fallback through existing failure suites. Distinguish local connection configuration validation from a real successful provider request. Cover both direct startup failures and background streaming adapters through existing terminal StreamChunk conventions. Preserve failed commit ErrorCode::Io and abort/single-terminal semantics; after failed data commit, read only the bounded terminal cause from the already-created generation instead of formatting the write error chain. Verify real local HTTP status/transport stubs through LLMFS and namespace projection; no new error framework is needed.

## 5. Delivery and archive readiness

- [ ] 5.1 Confirm every shipped slice has current-head review and passing required CI; queued/skipped checks do not count, and the plan's existence does not mark implementation complete.
- [ ] 5.2 Sync only merged behavior into its canonical spec, reconcile shared requirement owners and update user-facing instructions; verify strict OpenSpec validation and current-surface guards.
- [ ] 5.3 Archive only after owned tasks are complete or explicitly handed to an active successor with preserved status; verify G1/G2/G3 claims match live artifacts and leave any unfinished runtime lifecycle acceptance with its existing owner.


### 2026-09-30 recovery repair WIP and preliminary review

After ordinary `/quit` of the failed recovery-authoring invocation, a fresh bare
Alan invocation (PID 44196, owned Herdr pane `w5E:p1`) received visible read-write
project authorization and authored the recovery patch directly. No operator Rust
patch or delegated worker grant was supplied. The first owning interpretation
regression genuinely failed with exit 101 (empty recovered pending queue), then
passed; the Machine recovery suite passed 12 tests and runtime suite passed 24.
The interpretation test uses synthetic admission/dispatch events and does not
qualify actual intake, completed effects, or G3. Evidence: `/tmp/alan-recovery-interpretation-red-20260930.ansi`,
`/tmp/alan-recovery-interpretation-green-20260930.ansi` and
`/tmp/alan-recovery-first-task-20260930.txt`.

The first full Engine run inside Alan's governed Tool failed with exit 101 and
`sandbox-exec: sandbox_apply: Operation not permitted`; that failure is retained.
The independent operator run of the same first candidate passed 1223 unit tests
(1 ignored), 20 integration tests and doc tests with exit 0, recorded in
`/tmp/alan-recovery-independent-engine-first-20260930.log`. It does not certify
subsequent edits or bypass the failed Tool sandbox.

Preliminary code-review used the fixed base `5189ad3590c79507567f42e6628bbbd718aeafd4`
and snapshot `/tmp/alan-recovery-wip-20260930.patch`, SHA-256
`28dc82a6bc0ac5a431603f3409326b28175618732f5340f25ff3d72db00b54fb`.
Spec review found four actionable issues: directly consumed steering lacks durable
non-replay disposition; admission precedes Steer-to-FollowUp normalization; rejected
discard can leave partial durable tombstones; failed admission persistence silently
consumes input. Standards review confirmed no hard documented violation and noted
error/data-loss handling plus duplicated Machine/observer admission implementation.
All findings were returned to Alan for direct repair with regressions. The actual
namespace/API intake, normal shutdown and explicit recovery boundary test is still
being authored. Final immutable-patch review, current-head CI, live G3, delivery
and the other unchecked UX tasks remain outstanding.


### 2026-09-30 actual recovery intake boundary follow-up

Alan added `runtime_admission_shutdown_and_explicit_recovery_preserve_paused_inputs`
through its visible read-write project Tools. It uses actual API and framed
namespace intake with the existing gated mock provider, observes acknowledged
admission, interrupts, normally shuts down and explicitly restarts from the
selected rollout. Both pending IDs and their order remain paused; unauthorized
continue creates no new generation request. A second recovery after the rejected
continue still retains those IDs/order and pause. The final observed focused run
passed 1 test with exit 0; this is runtime-boundary evidence, not native effect
completion or live G3 qualification. Evidence:
`/tmp/alan-recovery-runtime-boundary-first-green-20260930.ansi` and
`/tmp/alan-recovery-runtime-boundary-extended-green-20260930.ansi`.

The independent source-size check failed with exit 1 at 1042 lines in
`engine_input_order_tests.rs`; evidence is
`/tmp/alan-recovery-source-size-first-20260930.log`. Alan was instructed to extract
the new recovery suite into an adjacent private-access test file, preserve all
assertions and existing tests, keep the 1000-line ratchet and remove its own
incorporated scratch file. No operator Rust patch was supplied. The four
preliminary Spec findings and shared admission duplication/failure handling remain
open until author repair, final review and affected checks. This follow-up changes
no task completion checkbox, G3 status or delivery status.

### 2026-09-30 recovery extraction and persistence review follow-up

Alan extracted five actual recovery-boundary tests into adjacent
`engine_recovery_boundary_tests.rs`, moved their shared provider fixture into the
existing parent test module, and extracted its test-only rejecting writer probe
from `rollout.rs`. It reported passing source-size, 27 Engine and 38 rollout
checks. An independent `cargo test -p alan-agent-engine recovery_boundary --
--nocapture` run passed all five boundary tests (exit 0). The author remains
active; these checks do not qualify later edits or final delivery. A second
independent boundary run also passed all five tests; the independent 1000-line
source-size ratchet passed. Logs:
`/tmp/alan-recovery-independent-boundary-20260930.log` and
`/tmp/alan-recovery-size-after-extraction-20260930.log`. Narrow Standards review
confirmed that extracted existing tests/assertions and the shared fixture were
preserved, with no production visibility widened solely for tests; it does not
qualify the complete changing patch.

The follow-up Spec review found the partial discard finding still open:
`persist_batch` writes separate JSONL records sequentially and flushes afterward.
A later failure can leave earlier complete removal records on disk while the
caller reports rejection and retains the entire live queue. The rejecting probe
fails before writing, so its passing test does not establish atomicity. Alan
received this concrete writer evidence and a request for an actual partial-write
or write-then-flush-failure regression, truthful admission uncertainty, and the
smallest shared disposition fix. Completed-only recovery, startup pause
projection, pending Tool cancellation reconciliation and remaining test placement
work are also still being authored. G3 and all delivery checkboxes remain open.

### 2026-09-30 restored startup activity boundary

Alan changed recovery so only reliable pending inputs restore a paused queue;
runtime startup now publishes the existing paused activity snapshot/event before
readiness when that queue is paused. The actual intake/shutdown/restart test now
asserts `/agent/2/machine/ui/activity` is paused and the UI event stream contains
the paused activity. An independent focused run passed with exit 0:
`/tmp/alan-recovery-startup-paused-independent-20260930.log`. This proves the
file-backed startup boundary, not yet a fresh candidate's live terminal rendering.
The completed-history/new-question regression is still being authored, and the
partial-persistence finding, final review/build and live G3 remain outstanding.

### 2026-09-30 active clock rendering evidence

Two live Herdr samples and a nine-sample quiet-period series from the owned
invocation show the working clock/spinner remaining at 572 seconds for more than
eight seconds, then jumping to 622 seconds when a Tool result arrives. Evidence:
`/tmp/alan-working-clock-samples-20260930.json` and
`/tmp/alan-working-clock-quiet-samples-20260930.json`. The existing frame timer
draws only when `dirty`; the layout computes elapsed time only during a draw.
Alan received this concrete task under 2.4, after its queued recovery correctness
work: update the existing active refresh boundary, preserve quiet idle behavior,
draft and scrollback, and add a regression without inventing another renderer.
The issue is confirmed, not yet fixed or qualified.

The new completed/legacy-history test compiled after the author corrected its
fixture/API errors, but the independent run failed at its empty Action-directory
assertion (exit 101):
`/tmp/alan-recovery-completed-history-independent-fixed-20260930.log`.
AgentFS always lists structural `clone`, `events` and `help` entries in `actions`;
an empty directory assertion cannot establish no Tool replay. This failure is
not evidence of a replayed effect. The no-replay check must inspect actual Action
records/effect identities while retaining the real fresh-question intake and
completion assertions. Alan is still repairing the test; no green claim is made.

The author subsequently corrected the structural-directory assertion while
preserving real file-native intake, completion, exactly one fresh generation,
and retained Applied/Unknown effect identities without replay. The independent
completed/legacy-history boundary run then passed (1 test, exit 0):
`/tmp/alan-recovery-completed-history-independent-green-20260930.log`.
Both startup-pause and empty-history usability boundaries now have independent
runtime evidence. These mock-provider checks do not qualify native side effects,
final patch review, live terminal acceptance or G3; the author is still active.

### User-authorized parallel tracks

The user authorized parallel subagents for recovery, self-development acceptance
and UI/UX. Recovery review remains read-only alongside the existing direct Alan
author in owned Herdr pane `w5E:p1`; the G3 track prepares native-PTY/Herdr
acceptance artifacts without fabricating results. A sibling pane `w5E:p2` was
created without focus changes in the same checkout for a separate Alan UI author.
That invocation visibly received read-write project authorization and initially
performs read-only diagnosis. Its writable implementation scope is `crates/tui`;
UI editing waits for the primary author's queued handoff to avoid conflicts.
The root operator remains the sole task-status writer and final delivery owner.

Acceptance preparation found a concrete recovery-entry dependency: `/project`
rejects `turn_active()`, including paused queues, while recovered pending work
inherits no grant and `/continue` requires current explicit project authority.
The UI track must fix settled-pause project entry while preserving running and
pending-interaction restrictions; it must also check whether mount-to-cwd
selection itself queues. G3 must use an exclusive fresh-candidate acceptance
window because concurrent author invocations change the dev latest-rollout
selector; no selector is silently rewritten.

Recovery review also found dispatch-checkpoint failure after successful admission
returns from `accepted_submission` before correlated `InputCompleted`, clearing
Machine identity while the TUI input can remain pending. The existing rejected
admission test does not cover this stage. The direct Alan author received the
finding and a request for a stage-specific failure regression using the existing
settlement/uncertainty owner. All these findings remain open pending fixes and
immutable-source verification.

Parallel recovery review passed 32 focused tests on unchanged source hashes:
`/tmp/alan-recovery-review-focused-20261001-r2.log`, end manifest SHA-256
`fade50ee16ac2356e8eb509d5d38510628f0e104e24220531e177102cb767ac8`.
The root independently ran `cargo test --locked --offline -p alan-agent-engine`:
1232 unit tests passed (1 ignored), 20 integration tests passed, doc tests passed,
exit 0. All 231 Engine source-file hashes remained identical across this run;
evidence is `/tmp/alan-recovery-full-engine-current-20260930.log` and
`/tmp/alan-recovery-full-engine-{start,end}-20260930.json`. This is broad regression
evidence for that WIP snapshot, not closure of the unexercised partial-write and
dispatch-failure findings or qualification of subsequent author edits.

The paused-directory boundary is now documented under the existing unified-input
owner and supplied to the direct Alan author: file-native directory selection
delegates current grant/path validation and Process binding to the existing
runtime cwd path, publishes correlated actual cwd, keeps the queue paused and
retains explicit continuation. UI clock-only authoring started independently in
`crates/tui`; the primary recovery author still touches no UI source. G3 native
PTY/evidence-inspector/Herdr artifacts are prepared under `/tmp`, with execution
still NOT_RUN. The completed-only repeat-recovery expectation is being reconciled
to ready/idle, while first reliable-pending recovery must remain paused.

### G3 harness audit before execution

Read-only review found acceptance gaps in the prepared native helper. Operator
corrections removed all signal-kill cleanup and generic automatic approval.
Failure retains the same native process/PTY for explicit normal observation,
interrupt or quit and remains FAIL. Cleanup self-checks retain a fake live handle
without launching a child. The helper now captures the exact owned child PID/start
identity across reparenting, records E and Q IDs/non-replay dispositions across
recovery and rejects every old nonce admission in a fresh invocation. Preparation
self-check and Python compilation passed; no native G3 run was performed.
A temporary read-only operator inspector reuses the existing processless
LocalAttachment and Shell APIs from immutable reviewed-preview source. It checks
expected native PID and attachment boot identity before reading, pins Root identity,
and bounds listing/read sizes plus total time. Wrong-PID rejection and actual
preview idle/correlated cwd reads passed; evidence is
`/tmp/alan-g3-owner-inspector-checks-20261001.json` and
`/tmp/alan-g3-owner-inspector-preview-20261001.json`. The native runner now requires
its pinned binary and records owner snapshots at each phase. It also checks
fresh/recovered invocation grants are inactive, correlates one new successful
UUID cwd Action with one fresh active RW namespace grant, and confirms directory
selection neither dispatches nor removes Q. Valid and invalid owner/control/grant
correlation self-checks passed. Temporary inspector source review closed the
pre-read attachment identity and actual bounded-read findings; adding bounded
grant reads is a further observer-only slice. Frozen-candidate
activity/cwd/input correlation, chooser behavior and the second independent
authored task remain required before execution/qualification.
Artifacts remain `/tmp/alan-g3-native-pty-20260930.py` and
`/tmp/alan-g3-herdr-acceptance-20260930.md`.

### Pending mount cancellation regression

Alan authored an actual namespace-input/request-mount/interrupt regression. Its
first independent run failed at a five-second correction timeout because local
interruption pauses scheduling; it had not explicitly continued the queue. That
failure is retained at `/tmp/alan-mount-cancellation-independent-red-20260930.log`
and is not treated as proof of missing Tool response pairing.

Before changing production, Alan moved the pairing assertion directly after
correlated cancellation and added explicit `queue-v1 continue` before correction
intake. The independent run then failed at the intended assertion, in 0.03s:
`interrupt must close the pending mount Tool request before accepting correction`.
Evidence: `/tmp/alan-mount-cancellation-independent-pairing-red-20260930.log`.
This reproduces a missing durable terminal Tool response through real runtime
intake/cancellation, not an HTTP provider-error diagnosis. Shared cancellation
repair and passing-after evidence remain pending.

The independent UI author also observed a genuine clock assertion failure and
then 158 passing TUI tests for its initial active repaint fix. Evidence:
`/tmp/alan-ui-clock-red-20260930.ansi` and
`/tmp/alan-ui-clock-first-green-20260930.ansi`. That first patch repaints on every
active frame tick; the author is refining it to the displayed second cadence
while retaining immediate event/input redraw and quiet idle/paused behavior.
The first patch is preserved at `/tmp/alan-ui-clock-first-20260930.patch`, SHA-256
`d73a53d7e2512b984f897510ba983f99e505f72245dfd3c4f95df9b95f2f2211`;
final immutable UI review, fresh build and native terminal acceptance remain open.

Alan subsequently authored a shared Host Mount terminal-response helper and
wired cancellation through truthful service terminal results, the existing
acknowledged recorder flush, and per-request pending removal before turn reset.
The independent pairing test passed (1 test, exit 0), including explicit queue
continuation, successful new-question intake, exactly one recovered correlated
Tool response and that response in the next provider request:
`/tmp/alan-mount-cancellation-independent-green-20260930.log`.
An intermediate compiler failure from attempting `?` on the non-failing public
`flush()` wrapper is preserved at
`/tmp/alan-mount-cancellation-independent-first-green-20260930.log`; the corrected
implementation reuses the existing `flush_recorder()` result-bearing API.
Warnings and shared normal-terminal handling are still being cleaned up by the
author. Final review must cover write-failure retry and Approved/cancel races;
this passing focused case does not prove the earlier live HTTP failure's cause
or qualify G3/final delivery.

The subsequent narrow review confirms normal terminal Resume now reuses the
shared helper, and an Approved result winning the cancel race retains its actual
grant. One concrete retry finding remains: the response is appended to live Tape
before flush acknowledgement, while a failed flush retains the pending request;
retry can append a second response with the same Tool ID. The writer does not
poison itself on flush failure. Alan received the finding and a request for a
flush-failure/retry regression and scoped idempotent terminal persistence, without
a new global registry or automatic effect retry. Reviewed `turn_support.rs`
SHA-256: `d459d105279307fe792a7f3baa22836acf2ef00d403ea115dd4d4214223a1731`.
This finding remains open despite the happy-path pairing test passing.

### Parallel UI rendering delivery checkpoint

The quiet Running clock slice authored by Alan in Herdr w5E:p2 is frozen at
patch SHA256 `5cecac1999e87d3d2e734049b2ff8b5da79ed65297cf7b6bca849fd816f524f4`.
The operator independently ran `cargo test --locked --offline -p alan-terminal-ui`:
158 tests passed, no failures, documentation tests passed. Independent correctness
and standards reviews found no actionable findings on the same three-file patch.
Fresh candidate native relaunch acceptance remains outstanding; this does not
complete task 2.4.

The user explicitly requested visible Tool, Markdown/diff and model/status work
to proceed now. The separate UI author is authorized to implement task 4.3 through
the existing history/layout owners, then 4.2 and 2.4, without waiting for G3.
Runtime semantic Action metadata and authoritative Connection catalog/binding
remain owning dependencies; no hardcoded model catalog or TUI Tool-argument
inference is authorized. Primary w5E:p1 owns Engine changes; w5E:p2 owns TUI.
No P3 checkbox is marked complete based on dispatch or preparation alone.

### Aggregate removal independent boundary check

The operator independently passed
`removal_ack_failure_is_uncertain_and_recovery_never_removes_a_prefix`
against the current Alan-authored source. Single-record full-ID removal excludes
either the complete set or none; failure retains live input and reports uncertain
disposition without claiming cancellation settlement. The G3 evidence parser was
updated for `machine_inputs_removed_v1`; artifact self-check and Python compilation
passed without launching Alan (G3 remains NOT_RUN).

Independent review identified a remaining shared writer defect: a partial JSON
write followed by another accepted record can corrupt a non-tail line and make
later acknowledged admissions unrecoverable. This finding was submitted to the
primary Alan author for a minimal owning writer fix and actual partial-byte
failure/next-persist regression. Complete-write/failed-ack coverage alone does not
qualify the partial-byte boundary. Current candidate remains uncommitted and
unqualified for G3 pending this and earlier outstanding recovery findings.

### Semantic rendering and Action producer handoff

The UI Alan author reproduced the existing heading projection failure in the real
terminal buffer at width 40: the old renderer displayed `#` instead of the first
heading character. This is assertion-output evidence; the enclosing logging
command masked the cargo exit status, so no exit-code claim is made. Typed spans
are now being implemented through the existing history/layout/scrollback paths.
Partial-drain, resize, continuation and Tape reconciliation remain acceptance
requirements; this is not completed Markdown/diff delivery.

Read-only runtime tracing identified the existing title/presentation/preview
mapping and the shared Action redaction/durability writer. The primary Alan author
received task 4.1: supply mapped optional metadata before terminal Action
persistence for Agent Tools and user commands, preserve raw/redacted evidence and
restore metadata with existing durable Action records. The later completion Event
is not itself an Action file producer.

The operator independently passed all 9 extracted `runtime::turn_input` tests and
confirmed original extracted-module function names remain present. This verifies
the focused extraction/build behavior, not full assertion equivalence or G3.
The supplemental G3 reader was corrected to accept complete final JSON without
LF and reject corrupt non-EOF records. Self-check and compilation passed; current
harness SHA256 is `c0984b514d8cad6d30e0d379b834be96738b91f28116361db376ba2b9dc5f556`.
No Alan was launched by these artifact checks; G3 remains NOT_RUN.

### Parallel ownership reset and model contract delivery

The primary author began an earlier queued clock task and edited TUI files while
the sibling author owned rendering. The operator used normal Ctrl+C and observed
paused state, then `/discard` acknowledged 8 pending inputs discarded without
execution. No process rescue, source rollback or operator Rust edit occurred.
The same primary Process received one Engine-only durability repair task; the
sibling remains the sole TUI writer and owns reconciliation of the unreviewed
clock edits. Remaining Tool metadata, paused cwd, model selection and review fixes
remain required; discarding stale prompts does not discard these plan obligations.

The operator independently passed the production-buffer Markdown width-grid test
on its then-current snapshot. A later full TUI snapshot failed compilation; the
author subsequently compiled and reported a separate partial-drain assertion
failure. No full TUI or native-rendering qualification is claimed.

Owning provider/Connection and request-control model-selection deltas now exist,
with independent Spec review and strict validation passed. They preserve captured
bindings for already-admitted queued work through recovery and require confirmed
installation before next-input success. Runtime coordination is unchecked task
2.20 in `unify-agent-command-input`; no model-picker implementation is claimed.
Current-code preparation confirms reusable `capture_connection`/immutable LLMFS
snapshot and canonical model/request-control resolvers. Admission currently
captures only raw Submission; queued/recovered work has no callable binding and
dispatch uses Process-wide config. Therefore an idle-only picker cannot satisfy
the accepted target. The owning implementation must capture non-secret binding
identity and resolved controls at admission, restore the same callable or fail
closed, and confirm installation before publishing next-input selection. A real
Runtime/FileServer test must distinguish providers A/B before and after selection
and recovery; missing A must fail without invoking B. Steering must retain its
admitted callable and controls: compatible steering may join the same active
binding; incompatible steering must settle Failed by exact ID through existing
durable removal before any join side effect, preserving active work, Tape,
pending Tools and ordinary queue state. It must not silently remap, convert to
future work or resume paused work. Both pre-selection compatible steering and
post-selection incompatible steering require actual Runtime/provider regressions.

### Independent rendering checkpoint and author continuation

The operator independently ran all 161 TUI tests plus documentation tests with
exit 0 and unchanged source hashes during the run. Artifacts are
`/tmp/alan-ui-markdown-independent-161-20261001.log` and its JSON manifest.
Two clock-related warnings and preliminary reviews of second drain/resize/Tape
reconciliation, preserved tabs and duplicate Diff projection remain outstanding;
this is not final task 4.3 or native candidate acceptance.

The UI author subsequently failed at namespace generation g268. Full historical
Tool request/response counts and the inferred compacted tail are balanced; no
pairing cause is proven. LLMFS discards the provider startup reason, so a local
connection configuration check cannot establish provider health. Task 4.7 tracks
the observed safe diagnostic gap. Failure/correlation/source evidence was saved
before normal `/quit`; fresh bare author continuation is PID 53200 in the same
owned pane and checkout, using the previous verified dev binary. Project RW and
cwd were explicitly reauthorized. It is author continuation, not durable G3
recovery or candidate relaunch qualification.

Read-only review also identified that existing compaction recovery restores all
historical messages plus summary instead of the live retained Tape. A focused
compaction/recovery test and owning fix if reproduced were dispatched to the
Engine author; this separate gap is not claimed as the cause of g268.


### Independent Phase R checkpoint and remaining recovery defects

Root independently ran `cargo test --locked --offline -p alan-agent-engine`:
1239 unit tests and 20 integration tests passed, with one unit test ignored.
All Engine Rust source hashes were unchanged during the run. Evidence:
`/tmp/alan-engine-phase-r-independent-20261001.log` and its adjacent JSON manifest.
This checkpoint does not close the subsequent R3/R5 findings or G3.

Independent review found the R3 host-mount terminal event is flushed before its
correlated Tool result. A flush failure can therefore suppress the recovered
wait while leaving no durable response. The existing local retry regression
permitted this incomplete durable state. Alan received the shared-owner fix and
the requirement to recover actual evidence and assert either an intact resumable
wait or one paired terminal response. Alan strengthened the existing real-flush
retry regression, and Root independently reproduced its durable recovery failure
with cargo exit 101 and unchanged reviewed-file hashes. Evidence:
`/tmp/alan-mount-durable-recovery-independent-20261001.log` and JSON manifest.
The assertion failure demonstrates the missing recovered wait/response; it is
not a compilation failure or merely a simulated local Tape count. After Alan
reordered the shared owner to record the response before terminal evidence,
Root independently verified this same regression passed with unchanged reviewed
files (`/tmp/alan-mount-durable-recovery-independent-green-20261001.log` and JSON).
Existing-response retry still emits missing terminal evidence without adding a
second Tool message. Alan subsequently extended the real regression through
first recovery, normal terminal completion, and a second actual recovery. Root
independently passed the final frozen Engine suite: 1244 unit tests, 20 integration
tests, and one ignored test, with all Engine Rust hashes unchanged. Evidence:
`/tmp/alan-engine-final-phase-r-independent-20261001.log` and JSON manifest.
Scoped R3/R5 spec review found no actionable findings; frozen Engine standards
review found no hard violations. A possible repeated durable Tool projection is
non-blocking and has no observed behavior difference for request_mount; no
optional abstraction is being added. All reviewed source matched the independent
test manifest. Frozen inclusive Engine patch SHA-256 is
`c84414636a197f5db220e20d2a70b67e3650e73cb3acf6013fa5d7fb5d2903da`
(`/tmp/alan-engine-phase-r-frozen-20261001.patch` and JSON). Candidate
build/relaunch and G3 remain outstanding. The ready primary Alan received one
Engine-only task 4.1 for runtime-owned Action title/preview/presentation before
terminal publication, including durable restore and truthful failures. No
project-control or model task was added to its queue. Alan's first real namespace
Tool regression reproduced the metadata gap with cargo exit 101: completed
Action status was visible but its `result.title` was Null rather than `Read
sample.txt`. Evidence: `/tmp/alan-action-metadata-author-red-20261001.ansi`.
The regression invokes the existing Tool execution boundary and reads actual
Action files; later recovery assertions are not yet reached by this red run.
After Alan added authoritative arguments to the existing evidence and mapped
metadata before Action writing, the same regression passed with cargo exit 0,
including Action restoration to a new AgentFS. Evidence:
`/tmp/alan-action-metadata-author-first-green-20261001.ansi`. This is author-only
focused evidence; failed Tool presentation, special explicit commands, redaction,
full frozen tests and independent review remain acceptance requirements.
Root's intermediate full run passed 1245 unit tests and 20 integration tests,
with one ignored, but the author changed `action_metadata.inc.rs` during the
run; it is not frozen qualification. Evidence:
`/tmp/alan-engine-action-metadata-intermediate-independent-20261001.log` and JSON.
An intermediate spec review identified failed edit/write Actions still receiving
argument-derived Diff from the unchanged unconditional shared mapper. The Event
mapper defect predates this patch; publishing it in Action metadata extends the
misleading result. The shared mapper must gate change Diff on actual success.
Standalone cd/rejected-command metadata and new explicit-command/failure/redaction/
failed-persist regressions also remain incomplete in this intermediate snapshot.
After the author returned ready, Root independently passed the fixed first
checkpoint: 1245 unit tests and 20 integration tests, one ignored, and all Engine
Rust hashes unchanged. Evidence:
`/tmp/alan-engine-action-metadata-first-checkpoint-independent-20261001.log` and
JSON manifest. This confirms test/environment state, not missing semantic coverage.
Root then submitted one Engine-only correction for the concrete failed-Diff and
special-command gaps plus actual redaction/durability checks. No extra FIFO work
was added. The author had used `agent_work submit root` to send its report back
to its own invocation, then settled normally; the next prompt explicitly requires
terminal reporting and forbids recursive self-report submission. No interrupt,
restart or process rescue was performed. The correction's real namespace
mutation regression failed with cargo exit 101: a failed edit Action contained
permission-denied diagnostics and the requested change Diff simultaneously.
Evidence: `/tmp/alan-failed-mutation-diff-author-red-20261001.ansi`. Alan then
added a guard at the existing shared presentation mapper for edit/write. Root
independently passed `failed_namespace_mutations_preserve_diagnostics_without_diff`
for both failed mutations with all Engine Rust hashes unchanged during execution.
Evidence: `/tmp/alan-failed-mutation-diff-independent-20261001.log` and JSON.
This focused green does not qualify later edits, special commands or the full
4.1 slice; those checks remain in progress.

Standards review found single-input cancellation still bypassed the shared
acknowledged-removal owner. Alan routed it through `persist_input_removals` and
added the existing adjacent removal-boundary regression for complete-write and
before-write failures. Root independently verified both local/recovered queue
IDs and correlated UI settlement through that test; the stable run passed with
all Engine Rust source hashes unchanged. Evidence:
`/tmp/alan-targeted-cancellation-independent-stable-20261001.log` and JSON.
An earlier Root run passed while source changed and is not frozen qualification.
Final combined review/build and G3 remain open.

Alan's actual R5 compaction-fidelity regression failed with exit 101: recovered
Tape held 12 messages while live compacted Tape held 8. The removed turn was
resurrected. Alan is repairing the existing rich compaction/rollout boundary and
explicit legacy behavior; this is a reproduced recovery gap, not an established
cause of the earlier provider startup failure. Root independently verified the
two new compaction-fidelity/legacy tests pass with the four reviewed files
unchanged during execution (`/tmp/alan-compaction-fidelity-independent-20261001.log`
and JSON manifest). This is not final R5 clearance: raw retained live Tool payloads
must pass through the existing durable redaction/truncation owner rather than
being cloned into the new snapshot. That concrete review correction and its
sensitive/large-payload regression were dispatched to Alan. The final frozen
Engine suite above includes the three fidelity/legacy/sensitive-payload tests
and passes independently. The earlier attempt at a focused sanitization run
hit a compile error during an active R3 edit; it is not a behavior red or frozen
qualification. The frozen scoped reviews above cover this correction. UI semantic repairs continue in
the separate owned pane; no new candidate has been built or live-qualified.


### Frozen Markdown/diff independent verification

The separate Alan UI author froze the 10-file semantic rendering patch, including
adjacent `semantic_tests.rs`. Inclusive patch SHA-256:
`d16a6b0b091ec15afd1d10c2e567fc5138ac269934d4ca166de76981e55c9880`.
Root independently ran the complete TUI suite: 164 tests passed, no failures,
and all TUI Rust source hashes remained unchanged throughout. Evidence:
`/tmp/alan-ui-final-independent-20261001.log` and its JSON manifest.
Standards review verified the frozen manifest and found no remaining findings.
Spec review reproduced one remaining production character-loss defect: after
one rendered-row drain at width 40, closing streamed emphasis changes geometry
and old row cuts lose two previously uncommitted letters (76 remain from 78).
The public HistoryCell probe failed with exit 101; evidence:
`/tmp/alan-semantic-stream-cutoff-probe-20261001.log`. Alan independently authored
`crates/tui/tests/semantic_cutoff.rs`; its actual regression failed with exit 101
(retained 38 letters instead of 40), recorded in
`/tmp/alan-ui-late-delimiter-author-red-20261001.ansi`. The author is replacing
unstable row-cut replay at the existing owner. The late-emphasis regression
then passed independently with all TUI Rust hashes unchanged during execution
(`/tmp/alan-ui-late-delimiter-independent-green-20261001.log` and JSON manifest).
This is a focused result only: the full suite exposed an existing scrollback
drain regression after source-position migration, which Alan is fixing before
another frozen checkpoint. Regression/full checks and both review axes remain
required for that checkpoint. Fresh native/theme/viewport acceptance remains pending.
Task 4.3 and delivery checkboxes stay open. The currently running author binaries
still do not include this patch. Task 4.2 preparation traced retained Action
file reads and existing evidence markers. Existing AgentFS Action listing must
serve the detail catalog even after inline cells are pruned; per-field read
failures must remain distinct from genuinely empty evidence. The detail modal
must intercept keyboard and paste before composer mutation and leave draft,
cursor, intent and transcript intact. Identical completed Action snapshots after
physical drain need a real no-duplicate regression in task 4.2. No details
implementation is claimed by this read-only preparation.


### Source-position rendering checkpoint and final correctness work

Alan froze the source-position rendering correction, inclusive patch SHA-256
`ada8cbaef85117bc29c7ec5c6282f1f5839220ad66393fd5efa7c98e5c93fb1c`.
Root independently passed 165 TUI unit tests and 3 public integration tests with
all TUI Rust source hashes unchanged during execution. Evidence:
`/tmp/alan-ui-source-cutoff-independent-20261001.log` and JSON manifest.
Spec review closed the original character-loss defect and verified the frozen
manifest. The precise EOF-Tab public probe passed; the earlier list-prefix
quadratic traversal was already removed in this frozen patch.

Standards review identified a separate remaining repeated-suffix allocation in
wrapped plain paragraphs. Alan received a bounded existing-owner byte-cursor fix.
The same task first tests the pre-existing whole-streamed-cell drain boundary
through actual App geometry and full Tape hydration. Alan's actual App regression
reproduced loss of open-fence source/context after the whole streaming cell was
pruned; cargo failed with exit 101. Evidence:
`/tmp/alan-ui-whole-cell-author-red-20261001.ansi`. Alan froze the repair plus
borrowed-suffix correction, patch SHA-256
`4b5dc6938051cc2f432c4a065b743537e050963186303a85554b53e74e4d4f2b`.
Root independently passed 166 unit tests and 4 public integration tests with
all TUI Rust hashes unchanged. Evidence:
`/tmp/alan-ui-whole-cell-independent-20261001.log` and JSON manifest.
This whole-cell boundary was pre-existing, not attributed as an introduced
source-cutoff regression. Read-only tracing found a subsequent actual production
path where ActionsChanged inserts a Tool after the retained empty AssistantTail
and the next prune breaks at that zero-row Tail, preventing later Tool rows from
draining. A bounded actual-App test-first task is authorized to verify and fix
that shared pruning boundary. The actual production-App test failed with cargo
exit 101: expected completed Tool rows but the second drain returned no rows.
Evidence: `/tmp/alan-ui-interposed-action-author-red-20261001.ansi`. Alan is
repairing the shared owner. The final correction froze as inclusive patch
SHA-256 `f28ae876af5d54adfe607136506d88c372acc47185b91e9399943fabc5407067`
(`/tmp/alan-ui-interposed-action-frozen-20261001.patch` and JSON manifest). Root
independently passed 167 unit tests and 4 public integration tests, all TUI
Rust source hashes unchanged, exit 0. Evidence:
`/tmp/alan-ui-interposed-action-independent-20261001.log` and JSON. Standards
review verified the final manifest with zero drift and no actionable findings.
Scoped spec review likewise found no actionable findings and verified all 12
frozen paths. The UI controller is authorized to submit one task 4.2 to ready
p2: retained Action detail catalog/modal, three-physical-row/byte-bounded
summaries, typed metadata consumption, honest evidence availability, draft
preservation, and no duplicate scrollback. Primary p1 concurrently owns 4.1
Action metadata production. The actual Action-file and post-drain snapshot
regressions compiled and failed with cargo exit 101 (0 passed, 2 failed): at
40 columns the long output rendered 3335 physical rows instead of at most three,
and an identical completed snapshot replayed already committed output. Evidence:
`/tmp/alan-ui-action-details-author-red-20261001.ansi` (controller) and
`/tmp/alan-ui-action-summary-author-red-20261001.ansi` (Root capture). These are
behavior failures, not compile failures or zero-selected tests. Alan is repairing
the existing shared summary/synchronization owners and implementing details.
After natural compaction, the UI author submitted a follow-up to its own Root
(`d833e65d-d184-48af-9953-0079ce7a8b0b`) that describes only the bounded-summary
slice. This is author-created queued work, not an operator dispatch or external
handoff. The complete 4.2 objective remains unchanged: a partial cached-details
prototype cannot replace retained AgentFS catalog/original evidence/error truth.
A second own-Root report (`5c28e29a-4042-4111-b70a-99a862d05ea5`) followed; the
author reports 171 unit/4 integration tests for the partial summary/cache
implementation, not complete detail acceptance. The existing Tool HELP owner
clarification is now tracked under unified-input task 2.15, preserving legitimate
self-scheduling and all routing behavior. All three own-Root notes were naturally consumed. The partial summary/cache
slice froze with author checks of 172 unit and 4 integration tests; it is not
complete 4.2 acceptance. The controller submitted one remaining full-detail
task to the same PID 53200, observed working, and added no duplicate FIFO work.
Its scope includes retained catalog/asynchronous per-field IO, selection, modal
input isolation, and truthful evidence availability. No process restart or
manual queue discard occurred.
Native/theme acceptance remains required; task 4.3 and candidate delivery remain
unqualified.


### User-requested current UI preview

The user requested a visible pane. Root captured consistent author-owned Rust
source into an isolated preview worktree, built successfully, opened/focused
`w5E:p3`, and observed fresh bare Dev PID 54966 displaying gpt-6.1-sol/ready and
a Tool-free Markdown/list/diff sample at 73×22. Binary SHA-256:
`bce0df9497f346fda7d236cbff5eb392f0a6130cf0919e6993818cb81df27a2a`.
Source manifest/build log live in
`/Users/morris/Library/Caches/Alan/ui-previews/20260930-132231/`; visible ANSI:
`/tmp/alan-user-ui-preview-20260930.ansi`. This contains WIP Tool-summary/cache
detail code and is user inspection, not full 4.2/native/theme or G3 qualification.
The two author panes were not interrupted. Subsequent live inspection found
`w5E:p3` absent from the explicit workspace pane inventory and PID 54966 missing;
the preview stderr was empty. This establishes that it is no longer live, not
its exit cause or a detach/recovery acceptance. No automatic restart occurred.

### Current review and preview feedback

Root independently passed the final rejection-path correction: 1250 unit tests
and 20 integration tests, 1 ignored, exit 0, all 255 Engine Rust hashes unchanged.
Evidence: `/tmp/alan-engine-action-metadata-independent-20261001.log` and JSON.
Standards review of the earlier metadata slice found no actionable ownership
issues. Spec review found the remaining `write_rejected_command` bypass; Alan
fixed it through the shared mapper after an actual handler regression failed
with title Null instead of Bash (cargo exit 101). The real two-branch regression
checks correlation/outcome, diagnostic preview and absent executed Command or
Process evidence. Spec re-review confirms the finding is fixed. Author p1 is
ready/frozen; reviewed patch SHA-256
`c632e69fe4e54d7518429616f942a2dacbec0b4aeda522b84b99b7ec80f77cc0`,
manifest `/tmp/alan-engine-action-metadata-reviewed-20260930.json`. Final native
semantic projection remains coordinated with 4.2, so 4.1 stays open. One tiny
existing HELP/manifest wording task completed under unified-input 2.15; Root
review confirmed description-only production changes with routing/schema
unchanged, and independently passed all 4 agent_work focused tests with all
Service Manager Rust hashes stable. Evidence:
`/tmp/alan-agent-work-help-independent-20260930.log` and JSON. Author p1
returned ready/frozen. Root then submitted one bounded paused-directory-control
task under unified-input 2.19.2 to existing protocol/AgentFS/Engine owners,
preserving current-authority validation and queue/no-replay semantics. This
is implementation in progress, not G3 qualification.

The user observed visible repaint on each slash-command character in p3.
Source tracing confirms that candidate count/wrapping changes live-region
height; `set_inline_height` clears from the old cursor down and rebuilds the
Terminal, losing its diff buffer, outside a synchronized frame transaction.
An independent fresh bare invocation of the same preview binary at 73×22
confirmed the path: `/`, `p`, `r` each emitted one clear-from-cursor-down and
one cursor-position query without a synchronized frame; `o`, `j`, `e`, `c`,
`t` kept the same height and emitted no clear. No Tools or authorization were
used; normal `/quit` returned exit 0, and the user pane was untouched. Runnable
probe: `/tmp/alan-slash-redraw-probe-20260930.py`; captures and metrics:
`/tmp/alan-slash-redraw-before-20260930/`. A matched 40/60/80/120-column
baseline grid at 22 rows likewise emitted three unprotected clears at each
width; all four fresh bare invocations naturally quit with exit 0 and used no
Tools/grants. Evidence: `/tmp/alan-slash-width-grid-before-20261001/summary.json`
and per-key ANSI. Alan subsequently fixed the shared TerminalSession frame
entry point using existing Crossterm synchronized updates around scrollback,
height adjustment and draw, including the failure cleanup path. Independent
TUI verification passed 189 unit and 4 integration tests with stable source;
both review axes reported no actionable findings. The rebuilt preview binary
SHA256 is `bb117d07b47dd7f4ddc7c833bcaa0206ae87a428e883fa7e20d4d3ec9c49acc5`.
Matched 40/60/73/80/120-column native runs each naturally quit with exit 0:
three clears, eight matched synchronization pairs, zero clears outside a
synchronized key frame. Evidence:
`/tmp/alan-slash-width-grid-after-20261001/summary.json` and per-key ANSI;
`/tmp/alan-slash-synchronized-frame-final-independent-20261001.log`.
The user subsequently confirmed slash input no longer flickers in the focused
73×22 Herdr preview. This qualifies that actual host view, alongside the
five-width native protocol evidence; other visual sizes/themes remain untested.
Direct
Ghostty GUI reading was rejected by the computer-use safety restriction;
no alternative screen-control technology was used. Fresh preview `w5E:p4`
was focused for the successful user visual acceptance. Its read-only disposable fixture
also produced real CD and Bash summaries and readable retained stdout via
Ctrl+O; opening/closing details preserved an unsubmitted operator draft.
The draft was cleared before handoff. Do not overwrite subsequent user input.
Current owning-crate tests passed (Engine 1256+20, Protocol 55+3, AgentFS 1+68,
TUI 189+4; one existing Engine ignored test). After Alan corrected test-module
formatting, canonical `just quality` passed with all 509 Rust files stable:
`/tmp/alan-reviewed-candidate-quality-after-format-20261001.log` and JSON.
Backend cleanup reviews inspected current source and matched the manifest;
they do not claim reconstructed byte-level old/new equivalence from earlier
hash-only checkpoints. Broader visual acceptance and the remaining G3/model
work remain open.

The remaining 4.2 acceptance regressions compiled and failed behaviorally: two
failures (0 passed, 176 filtered) expose a readable range wrongly rejected by
the whole-field display cap and late typed Diff lines losing styling. Evidence:
`/tmp/alan-ui-detail-range-author-red-20261001.ansi` and original durable Tool
record `/tmp/alan-ui-detail-range-author-red-record-20261001.json`. The outer
Tool exit was 1 after a following search; the cargo exit was not separately
saved, so this evidence does not establish cargo exit 101. Alan is repairing
descriptor-range reading and the existing full-detail semantic projection.
Root then independently passed the two actual focused acceptance regressions
(exit 0, 2 passed, 176 filtered), with all TUI Rust hashes unchanged during the
run. Evidence: `/tmp/alan-ui-ranged-detail-intermediate-independent-20260930.log`
and JSON. This proves those two corrections at the captured intermediate
snapshot; ongoing asynchronous/retention work and final 4.2 remain unqualified.
A later whole-TUI independent run passed 184 unit and 4 public integration
tests, exit 0, all TUI Rust hashes stable during the run. Evidence:
`/tmp/alan-ui-tool-details-independent-20261001.log` and JSON. The author
subsequently edited reference reading while finishing gates; this is an
intermediate checked snapshot, not the final frozen slice. Final qualification
must match the frozen bytes and be repeated if they drift. Final author
checkpoint froze at inclusive 23 paths, patch SHA-256
`ef3bd46784d7fd6f2225b92624d2d7c96db6160041a4512785f0cb3a2f9f2148`;
`/tmp/alan-ui-tool-details-frozen-20261001.patch` and JSON. Root independently
passed final 184 unit and 4 integration tests, all 43 TUI Rust hashes stable
and matching the freeze. Immutable reviewed-checkpoint test evidence:
`/tmp/alan-ui-tool-details-reviewed-checkpoint-independent-20261001.log` and
JSON. Standards review found no actionable ownership findings, zero drift.
Spec review found three actual integration gaps: normal details still use the
mutable `/agent/root` alias rather than the attachment PID; generic
result_preview loses precedence to raw JSON fallback; and modal Ctrl+C consumes
an acknowledged input before Running when pending submission context exists.
Thus this checkpoint is reviewed incomplete, not final 4.2/4.1 qualification.
The UI controller submitted one TUI-only bounded test-first correction to the
same PID 53200 and observed working. Existing pinned attachment, metadata and
interrupt owners must be reused, with original retained evidence preserved.
The correction naturally froze at inclusive 25 paths, patch SHA-256
`0a070272bf94dfacc6d7094dbaa14b2de10e443a781e20fdae1d3a541203f93b`;
`/tmp/alan-ui-tool-details-reviewfix-frozen-20261001.patch` and JSON. Actual
owning source count is 44 Rust files, not the author's stale count of 43.
Root independently passed 186 unit and 4 integration tests with all 44 hashes
stable and matching the freeze: `/tmp/alan-ui-three-findings-final-independent-20261001.log`
and JSON. Standards found no actionable defect. Spec closed preview precedence
and pre-Running Ctrl+C, but found the missing attached-PID fallback still reads
through mutable Root alias after failed reattachment. One bounded TUI correction
must show explicit unavailable/retry and launch no reads for Root alias without
an attached PID, preserving explicit Process paths. The existing real alias
regression must exercise this production entry point. Slash repaint follows
that final owner guard and review. The guard then naturally froze at patch
SHA-256 `92957a209804a116fc68c0cc11e51f761b269fa5378cec8170c173a2214aaade`;
`/tmp/alan-ui-tool-details-rootguard-frozen-20261001.patch` and JSON.
Actual pre-fix regression a415 observed 13 AgentFS IO operations instead of
zero, Tool/cargo exit 101; `/tmp/alan-ui-unresolved-root-author-red-20261001.json`.
Root's final four-crate suite passed Engine 1255 unit plus 20 integration (1
ignored), protocol 55 plus 3, AgentFS 1 plus 68, TUI 187 plus 4, with all 329
Rust hashes stable; the 44 TUI hashes match the frozen guard checkpoint.
Evidence: `/tmp/alan-directory-and-ui-owner-guards-final-independent-20261001.log`
and JSON. Both independent axes closed the remaining backend and UI owner
findings without further actionable findings. This is a locally reviewed
checkpoint, not native chooser/visual/quality/CI or G3 qualification. The sole
UI controller was cleared to submit one slash synchronized-frame task; the
primary author received one Engine-only cleanup of five preceding WIP Clippy
findings. No duplicate FIFO tasks were queued.

Final 4.2 review must distinguish manually written expiry markers from actual
storing-server expiry and reopening. Root also traced a real layout mismatch:
modal drawing uses the frame area, while inline viewport sizing currently
counts only history/context/composer. A fresh or drained transcript therefore
can leave details a two-row viewport; production sizing plus Ctrl+O/Esc needs
an actual regression, not only a fixed-height TestBackend snapshot. The author
naturally added that production-sizing regression in the current task: Action
a351 compiled and failed with cargo/Tool exit 101, 0 passed/1 failed/183
filtered, actual height 2 versus expected 24. Evidence:
`/tmp/alan-ui-modal-height-author-red-20261001.ansi` and JSON. Alan is fixing
the existing layout branch and will verify close-to-draft sizing/cursor and
scrollback; no extra task was queued.

Directory-control first test reached a compiled failure (cargo exit 101, 0
passed/1 failed/1251 filtered), but the failure was missing Action receipt
`a0/result`, not a demonstrated cwd-change assertion. Root and independent
standards review found the initial adapter fixed at `/mnt/new`; production
`with_adapter` correctly adopts its adapter cwd, so that fixture already
started at the target. Evidence:
`/tmp/alan-paused-directory-selection-author-red-20260930.json`. The author
corrected the fixture to store distinct old/new adapter cwd. The second real
run compiled and failed (Action a627, cargo exit 101): binding stayed
`/mnt/old` instead of `/mnt/new` after actual `Shell.write(machine/ctl)`, with
Paused state and no generation. Evidence:
`/tmp/alan-paused-directory-corrected-fixture-author-red-20260930.json`.
The author completed and froze the shared Op/control lane. An independent
intermediate run encountered the new Op missing its submission-handler match
arm (E0004); that was a mid-edit compilation failure, not a behavior red.
Evidence: `/tmp/alan-paused-directory-red-independent-20260930.log` and JSON.
The final independent owning-three-crate run passed: Engine 1252 unit plus 20
integration cases (1 ignored), protocol 55 unit plus 3 integration cases,
AgentFS 1 unit plus 68 integration cases. All 284 owning Rust file hashes
were unchanged during the run. Evidence:
`/tmp/alan-paused-directory-final-independent-20261001.log` and JSON.
Standards review found no actionable defect. Spec review found the API
control sibling omitted `SelectProjectDirectory` from the immediate-control
whitelist, leaving it in the ordinary paused FIFO. One bounded Engine
correction was submitted after the author naturally reached Ready; real
submission-channel regression and final requalification are pending. The final
Spec review also found valid UUID/path-string envelopes losing correlated
rejection when path semantics are rejected before the existing control owner.
A second bounded correction will follow the active author task; it must retain
malformed-envelope/size rejection and publish the existing failed Action for
semantic invalid paths without changing cwd, queue or pause. The API regression
compiled and failed as Action a685, Tool/cargo exit 101, retaining `/mnt/old`
instead of `/mnt/new`; `/tmp/alan-directory-api-whitelist-author-red-20261001.json`.
After that correction naturally reached Ready, one semantic-rejection task was
submitted. Its compiled regression a702 failed with missing correlated Action
`Err(NotFound)` (0 passed, 1 failed); `/tmp/alan-directory-semantic-rejection-author-red-20261001.json`.
The wrapper continued with a later search and exited 0; the cargo exit was not
separately saved, so this evidence does not assert cargo exit 101. Production
correction naturally completed. Both API and semantic-rejection review findings
closed; the 1255+20 Engine/55+3 protocol/1+68 AgentFS final independent suite
above passed with unchanged owning hashes. Native chooser acceptance is pending. Backend
qualification alone does not complete the native project chooser or Host
Mount revoke/reauthorize acceptance. No production adapter semantics changed to accommodate the test.

Safe-startup-error preparation verified the LLMFS literal-error loss and the
namespace failed-commit early return. Background HTTP adapters also discard
startup errors by closing their stream without safe terminal classification.
The independent existing LLMFS startup-terminal test passed 1 case; this is
preparation only. Task 4.7 now explicitly covers these existing boundaries,
HTTP status/auth/timeout/connect categories, sensitive fallback and abort races.

The owning model-binding author reached a compiled initial Runtime/API/file
admission regression red and returned Ready without production implementation.
Root independently reproduced exit 101 (0 passed, 1 failed, 1256 filtered) and
preserved the exact original failed Tool record. Evidence:
`/tmp/alan-model-admission-initial-author-red-20261001.json` and
`/tmp/alan-model-admission-initial-red-independent-20261001.log`.
Root then submitted one continuation requiring the full accepted backend outcome,
not just a metadata-key check. A/B dispatch/recovery/steering and model selection
remain unimplemented/unqualified at this checkpoint.

An early moving-source model audit found two concrete issues to correct before
qualification: incompatible steering is acknowledged before removal, but the
removal-error path currently pauses then publishes terminal Failed without
retaining the input locally; selection failures project unrestricted authority
error chains. Captured controls also still require complete actual dispatch
integration. Root verified the relevant caller and saved observed source hashes
in `/tmp/alan-model-binding-early-review-20261001.json`. Connection injection is
actively being authored; this is preliminary evidence, not frozen review or a
passed model-selection gate. Corrections require real acknowledged-Steer removal
fault and safe-error regressions before final qualification.

### Candidate placement continuation (2026-10-01)

The user confirmed the synchronized slash preview no longer flickers, then
requested candidates below `: ` so filtering does not move the input. Task 2.3
now proceeds through the sole UI author controller; implementation and absolute
native cursor verification remain pending. The author must separate candidate
height from wrapped composer height and must not move the composer upward to
make room at the terminal bottom. The user-owned preview pane is unchanged.

The preceding safe project-control cleanup froze at patch SHA-256
`482cd7d377e07433e2c569c38d4fd75a4195e6373f2105d02332a5a01e39c810`.
Root reconstructed its exact 46 TUI Rust files in an isolated snapshot and
independently passed 195 unit and 4 integration tests, exit 0, all hashes stable:
`/tmp/alan-ui-project-safecheck-independent-20261001.log` and JSON. Independent
standards review found no new actionable cleanup findings. This does not close
the known production blanket pending-input guard, authoritative admission
receipt dependency or real transport/Host-ordering integration coverage.
Task 2.1 and paused project-control qualification remain open.

Root independently measured the existing synchronized preview at 73×22 using
the operator-only existing vt100 dependency, actual cursor-query responses and
PTY dimensions set before launch. Typing `/project` moved the absolute cursor
from row 7 at `/`, to row 4 at `p`, to row 2 at `r`; later letters stayed row 2
(zero-based). The invocation used no Tools or grants and normally quit with
exit 0. Evidence: `/tmp/alan-completion-anchor-before-20261001-73/metrics.json`
and per-key ANSI; the user-owned preview was untouched. This qualifies the
reported baseline drift, not any candidate fix. The compiled author regression
for wrong menu order also failed with cargo/Tool 101 (0 passed, 1 failed, 195
filtered): `/tmp/alan-ui-candidate-below-author-red-20261001.json`. Its current
Fixed-backend relative-row assertion masks viewport-origin movement; final
qualification must include absolute native rows and usable bounded disclosure
when the composer fills a short pane. No layout task is marked complete.

### Model-binding and native layout review continuation

The model failure checkpoint independently passed Engine 1258 unit and 20
integration tests (1 ignored), protocol 55 plus 3, and LLMFS 3 plus 41. The same
four-crate run failed in Service Manager: 115 passed, 5 failed, total exit 101.
All captured backend hashes remained stable. Evidence:
`/tmp/alan-model-failure-owning-independent-20261001.log` and JSON. Existing
ephemeral invocation/root recovery tests observed a disappearing Root or
NotFound, so this checkpoint is not qualified. Narrow startup-owner review is
ongoing; initial source tracing shows unconditional managed-profile authority
injection and a Ready publication before initial callable capture completes.

Independent standards review also found that the new generic rejection guard
uses historical admitted IDs as current disposition: after successful durable
removal it re-retains the input locally, while recovery excludes it. The same
shared cause affects a UI write failure after acknowledged Steer removal. Alan
received one bounded correction after its natural Ready; actual removal-failure
retention and safe-error regressions must remain intact. Model A/B actual
dispatch, unavailable restore and truthful owner status remain unqualified.

Root's preliminary UI candidate uses captured author TUI bytes over the prior
immutable preview backend, binary SHA-256
`30d9be8cbc395fe5a56b66840ea4909c707941bd293ef1fb48d8b56d43486a7a`.
At 73×22 with startup at row 0, candidates appear below input and all `/project`
filter keys keep absolute cursor row 1. With startup at row 20, the ready input
is row 21 but candidate appearance shifts it upward to row 15. Both invocations
normally quit with exit 0 and used no Tools or grants. Evidence:
`/tmp/alan-completion-anchor-preliminary-20261001-73/metrics.json` and
`/tmp/alan-completion-anchor-preliminary-bottom-20261001-73/metrics.json`.
This proves the captured layout still needs actual Inline-origin correction;
it is not the evolving author's final snapshot or a completed task 2.3.
