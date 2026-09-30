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
- G1 live behavior is qualified locally; G2/G3, independent review, merge and
  current-head CI remain unpassed. The historical failed attempts below remain
  evidence of the previous candidate, not the status of this new candidate.

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

- [ ] 3.1 After G1 passes, prepare an isolated checkout at a recorded Alan SHA and a dev candidate executable; select one small remaining accepted defect with a clear failing check, verify relevant instructions/dispositions, and bound Alan's writable scope to the checkout.
- [ ] 3.2 Through Herdr, ask Alan to inspect its own source, identify the shared cause and produce the patch plus regression check; preserve its prompt/Tool evidence and confirm no operator supplied or silently edited its patch.
- [ ] 3.3 Have Alan run the focused check and inspect its diff; independently verify failing-before/passing-after, review scope and actual command exit statuses, and ensure the patch complies with repository architecture and test placement.
- [ ] 3.4 Build the reviewed candidate through the governed workflow and freshly launch it in Herdr; demonstrate the corrected interaction and clean exit, retaining the known-good binary. Record source/binary identity and mark G2 passed only after this loop succeeds.
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
