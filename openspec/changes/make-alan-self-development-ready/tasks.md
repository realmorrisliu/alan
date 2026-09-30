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

## 1. P0 — Make project entry usable

- [ ] 1.1 Connect `/project` and Agent mount requests to a host-local chooser using the current invocation and existing Host Mount Service; verify approve/read-only/read-write/cancel/revoke in the bare CLI without a manually supplied runtime directory, and document the visible flow.
- [ ] 1.2 Select the Process cwd after approval and project truthful project-relative status; verify fresh launch without approval cannot access the project, revoked grants fail closed, and `!pwd` plus Agent `read_file` agree on an authorized disposable project.
- [ ] 1.3 Replace missing-project internal errors with an actionable authorization summary; verify both explicit commands and Agent Tools reach the same explanation and retain diagnostics in details.
- [ ] 1.4 Reproduce the recorded cancel → queued follow-up → continue → unresponsive-control sequence against exact HEAD, preserving submission/Action identifiers and process evidence; if reproduced, fix the shared cause under the unified-input owner and add the smallest regression that fails before the fix. If not reproduced, retain the uncertainty and run the G1 interruption cases rather than claiming it fixed.
- [ ] 1.5 Review the P0 slice, resolve findings, pass focused Host/CLI tests and required current-head CI; record actual ordinary-terminal and Herdr results here.

## 2. P1 — Complete the everyday task loop and G1

- [ ] 2.1 Render immediate authoritative admission receipts, queued previews/counts and paused choices; verify no lost or duplicate input across normal admission, rejection, cancellation and Tape reconciliation. Keep scheduling changes with the unified-input owner.
- [ ] 2.2 Wire existing channel history, authorized file candidates and installed/descriptor Skill sources at the real entrypoint; verify recall after restart, dev/stable separation, correct recalled intent and candidate removal on revocation.
- [ ] 2.3 Implement one-Enter slash execution, Tab insertion and idle Ctrl+C draft clearing with help text; verify paste/Enter, multiline input, active interruption, rejected drafts and UTF-8 cursor editing through focused composer tests and Herdr.
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

## 5. Delivery and archive readiness

- [ ] 5.1 Confirm every shipped slice has current-head review and passing required CI; queued/skipped checks do not count, and the plan's existence does not mark implementation complete.
- [ ] 5.2 Sync only merged behavior into its canonical spec, reconcile shared requirement owners and update user-facing instructions; verify strict OpenSpec validation and current-surface guards.
- [ ] 5.3 Archive only after owned tasks are complete or explicitly handed to an active successor with preserved status; verify G1/G2/G3 claims match live artifacts and leave any unfinished runtime lifecycle acceptance with its existing owner.
