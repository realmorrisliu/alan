# Tasks

## Current product acceptance coordination

2026-10-07 follow-through starts from merged main `59855ebe` in
`codex/alan-input-lifecycle-audit`. The [ordered delivery plan](next-delivery.md)
records the new user goal and initial implementation/acceptance inventory.
PR #1026 delivered the runtime and terminal work referenced by the historical
notes below; PR #1030 synchronized its bounded contracts and PR #1031 archived
the completed self-development milestone. Their delivery does not close this
change's broader remaining matrices. Earlier statements below that review or
implementation is pending describe their dated slice, not the current baseline.
In particular the TUI now sends `project-cwd-v1` through the existing control
lane; task 2.19.2 needs remaining product acceptance rather than another selector.
The new plan also records actual isolated Herdr last-view closure/reconnect and
Ctrl-D shutdown of an active native shell plus descendant on the product binary.
The dated all-client gap below is superseded by that bounded evidence; server
shutdown with active Alan, explicit recovery and broader parent matrices are not
inferred from it.

2026-10-02 current local qualification: linked readiness G1/G2/G3 are locally
accepted through supervised native Alan-authored tasks and independent frozen-source
review. Final604Rust26build candidate40583bd...b99fc6/binary9fa4cb...8fe0a passed
308TUI+10integration/fullquality/release; actualG3 sequence6e6t44ap preserves exact
E/Qonce,Dzero, fresh authority on explicit recovery, precise busy approval and
owned cancellation, natural exits and no process rescue. Ordinary Herdr view
detach preserves the process while another main client remains; all-client
disconnection/server-stop semantics are not qualified. This does not close the
entire change or imply unattended self-bootstrap. Required new-head CI/merge remain.

Remaining broad runtime matrices retain this active owner: same Agent multi-client
result/cwd ordering, end-to-end fault/unknown-outcome acceptance in2.13.2, and the
complete cross-grant/sandbox/path matrix. No parent checkbox or automatic-routing
qualification is inferred from the bounded G3 sequence. Linked readiness may close
its own terminal/supervised-bootstrap tasks after delivery while these remain active.

2026-09-30 local runtime follow-up: the self-development G1 retest identified a
FIFO mutex scheduling deadlock in Engine execution supervision. The execution
future remained queued for AgentFS state while a selected observer branch awaited
a later acquisition without polling that future. Both ordinary submissions and
deferred actions now keep execution and observer IO concurrently polled, and
settle admitted observer writes before final UI publication. Correlated evidence,
build identity and actual ordinary-terminal/Herdr G1 results are recorded in the
linked readiness task list. Engine tests and `just quality` pass locally;
independent review, CI and merge are pending. This runtime fix belongs here and
does not close unrelated unchecked queue/recovery requirements.

The [archived self-development readiness plan](../archive/2026-10-07-make-alan-self-development-ready/tasks.md)
records delivered project entry, terminal UX and supervised G1/G2/G3 acceptance. This
change retains queue, cwd, cancellation and recovery runtime ownership; the earlier
tracer bullet remains valid evidence but does not qualify self-development.

## Tracer bullet — execution priority accepted 2026-09-28

Complete one real terminal workflow before expanding horizontal hardening. This
milestone does not complete the full change or authorize archive.

- [x] TB1 Verify the merged baseline through the real CLI in an ordinary terminal
  and Herdr: ordinary Agent input, explicit `!`, one approved project and shared
  cwd, Agent write → shell read, shell write → Agent read, stdout/stderr and exit
  status. Exact-source live evidence is recorded below.
- [x] TB2 Verify owned exit, fresh default startup, and explicit `--resume`.
  Prior and restored Root rollout/Action evidence is recorded below. The
  current-main Herdr run also verified live completed-append non-replay. Moving
  the pane to an unfocused tab was tested, but full Herdr view-detach acceptance
  remains open under task 2.19. The targeted engine test verifies unknown-effect
  gating.
- [x] TB3 Fix only gaps observed in this workflow in independently reviewable
  PRs. The reproduced FIFO input gap shipped in PR #1021 after current-head
  Codex review and required CI passed. Current-main Herdr acceptance found no
  further runtime gap; PR #1022 records the queue evidence. Both merged PR
  worktrees were cleaned while their branches were retained. Details follow.

PR #937 was closed without merge on 2026-09-28 at head
`3abf4cb1ef739f13dd619e6e6f9dc82d2d6ecd42`. This stale aggregate branch changed
148 files (14,097 additions and 2,834 deletions from its original base) and had
31 unresolved Codex findings, including P1 sandbox and queue-durability issues.
Runtime and Agent command slices have since landed independently through PRs
#970 and #973; FIFO admission shipped through #1021. PRs #1022 and #1023 record
acceptance evidence only. Closing the aggregate branch does not complete unchecked
tasks; its branch and review threads are retained, and remaining requirements stay
open.

The competing output-projection attempts PR #949 and PR #1014 were closed without
merge on 2026-09-28 at heads `3cf82f2b6e9df2f4476e49f09f1b71dd2bb132cc` and
`ed089a95a29b8e6d40a27e0909d68890c18da6e5`, respectively. They changed the same
five Host files. PR #949 added 1,896 lines and retained 11 unresolved Codex P2
findings about CSV, GNU quoting, and root-relative Markdown/CSS behavior; PR #1014
added 1,239 lines and retained 31 findings (6 P1, 25 P2), including raw-path
disclosure and path/URL corruption. Both branches and all review threads are
retained. Neither implementation is accepted as delivery evidence.

OpenSpec task 2.7 remains open: shared-cwd path projection and protection against
raw Host roots and `/mnt` aliases are still required. These closures do not waive
path privacy or sandbox authority. Future work should implement only the captured
output forms required by that task and preserve unrelated project data; no generic
multi-format parser is accepted here. Other unchecked input, queue, cancellation,
and recovery tasks also remain open. Automatic routing remains outside this
milestone.

Baseline evidence (2026-09-28, source `08a1784a5217bfa760f7df5057cb5ae533c0ed29`):
`cargo test -p alan-os-host --test explicit_command -- --nocapture` passed
(1 test). It covers shared project edits, command streams/exit status, cancellation
and explicit recovery without completed-command replay using a mock provider.
`cargo build -p alan --bin alan` passed; the resulting binary SHA-256 was
`cdeecdff7611e7072ed079367bed1d43da8e35d699ffe6282f57168075f1a001`. The first
real Herdr startup waited on `PackageStoreLock::acquire` while the pre-existing
development Host held the channel lock. The user authorized a normal stop;
launchd's `KeepAlive` restarted that Host, so its `alan-dev.os-host` job was
temporarily booted out (registration not deleted) to let the foreground instance
start. The stable-channel Host was left running.

### TB1 live evidence

The ordinary pseudo-terminal run from the exact source build returned
`ORDINARY_TTY_READY` to normal Agent input and exited on Ctrl+D. In Herdr, the same
binary returned `ALAN_TRACER_READY`; after explicit read-write approval for the
test project at `/mnt/tracer`, `!cd /mnt/tracer`
succeeded. Agent `write_file` created the 11-byte `agent.txt` (`agent-first`),
`!cat agent.txt` read it, native shell redirection wrote `shell-second`, and an
Agent request specifically using `read_file` returned `shell-second`. A shell
command returned stdout `tracer-stdout`, stderr `tracer-stderr`, and exit code 7.
A later `!pwd` and append command also succeeded.

On baseline source `08a1784a5217bfa760f7df5057cb5ae533c0ed29`, rapidly submitting a
second command while `!false` was active produced the visible `submit blocked:
waiting for this input to complete` error and retained the draft. Pressing Enter
after the first command settled executed the draft once. This established the
original rejection and recovery behavior, not ordered admission. PR #1021 removes
that rejection on the Root TUI path; its separate Herdr queue acceptance is
recorded under task 2.5.1. Multi-client identity and Process-owned cwd checks remain
open under tasks 2.1 and 2.5.

### TB2 lifecycle and recovery evidence

Ctrl+D ended the successful foreground runs. Moving the still-running Herdr pane
to a new non-focused tab preserved its PID and accepted another Agent input; Ctrl+D
then ended that instance. This verifies tab-movement persistence, not actual Herdr
view detach; task 2.19 still requires that acceptance. A fresh default invocation
selected a new Root rollout. It recalled `ALAN_TRACER_READY` from the shared Memory
Store, as required by `runtime-memory-contract`; this is memory recall, not transcript
resume. Host Mount grants were absent from new invocations and required a new choice.

The fresh-default Root rollout was `0a3696be-f725-4538-8abe-119fe4269a87`; its
successful `!printf x > resume-marker.txt` is Action `a0`. Explicit `--resume` created Root rollout
`6e9e243a-bd94-40db-8475-f2e44694e94b`, whose restored transcript contains that
same command and Action `a0`; the Root selector resolves to this restored rollout.
This is direct evidence of CLI Root selection and Action restoration. The marker
was one byte both before and after recovery, but its overwrite is idempotent, so it
does not prove live non-replay. The baseline explicit-command integration test above
supplies replay-sensitive non-replay evidence. The targeted test `cargo test -p alan-agent-engine test_replay_approved_batch_bypasses_unknown_only_for_first_tool_call -- --nocapture` passed (1 test), confirming an unknown effect remains blocked unless explicitly approved. This is code-level unknown-effect coverage, not a live crash-injection test.

### Current-main Herdr re-acceptance (2026-09-28)

On merged main `176dcb19436026d48dedb7a86c8828ed9fa13340`,
`cargo build -p alan --bin alan` passed (binary SHA-256
`6b2fde0fdbfe997c4923ec7ddcd3ba6f009534e43fada70232336c6a81fb7b4f`). In a
Herdr pane, ordinary Agent input returned `CURRENT_HEAD_TUI_READY`. After an
explicit read-write Host Mount approval for the disposable test directory, the
Agent wrote the 11-byte `agent-first` file and `!cat` read it; shell redirection
wrote `shell-second` and the Agent's `read_file` returned it. A shell command
reported stdout `tracer-stdout`, stderr `tracer-stderr`, and exit code 7.

The first foreground instance appended one byte to a replay marker and exited
with Ctrl+D. A new default instance returned `FRESH_DEFAULT_ROOT`; after a new
explicit mount approval it appended one more byte and exited with Ctrl+D. Its
Root rollout was `rollout-20260928-145015-37c26229-f26d-46b3-97f9-23d32e9fca7a.jsonl`,
where the successful append is Action `a0`. Explicit `--resume` selected that
history as `rollout-20260928-145219-bfa07fad-82f7-4df3-8cca-2527d134071a.jsonl`;
the Root answered a follow-up about its prior transcript with
`FRESH_DEFAULT_ROOT`. The marker remained exactly two bytes before and after
resume, so the completed append was not replayed in this live run. Each owned
Alan exited on Ctrl+D; the test pane was closed and the stable Host was left
running. The separate Herdr view-detach acceptance and other lifecycle cases
remain open under task 2.19.

One earlier exact-source session did not process a quickly submitted follow-up
and its Host API timed out; no shell Action or marker file was recorded. We stopped
only that test PID rather than exiting it with Ctrl+D. Repeating the failed-command/
follow-up sequence later worked, including Host API queries. Treat that first timeout
as an unconfirmed anomaly, not a reproduced `exit 7` or aP failure.

## 1. Planning and contract reconciliation

- [x] 1.1 Record confirmed interview decisions, ADR draft and glossary; verify links and distinguish target behavior from the current implementation.
- [x] 1.2 Produce proposal/design and owning deltas, including old task-lease, bash-request and generation-only conflicts; verify complete modified requirement blocks preserve unrelated scenarios.
- [x] 1.3 Final shared-understanding confirmation received on 2026-09-24; ADR-0058 and disposition record accepted direction, with explicit-command runtime delivery still in progress.

- [x] 1.4 Record the user-approved KISS revision: mature Host shell, explicit aP access, shared Host Mount authority and scoped execution-path disclosure; supersede the earlier namespace-command grammar.

- [x] 1.5 Record the accepted internal-aP refinement: task-oriented alan9 command facade and shared project file/path identity across Agent editing and native commands.

- [x] 1.6 Record the user-approved 2026-09-27 lifetime revision: independent foreground Alan per Herdr terminal session, ordinary-terminal support, and explicit recovery; mark the prior background Host premise superseded without claiming implementation.

## 2. Explicit command slice

- [ ] 2.1 Define the versioned file record details for submission identity, prefix intent, queue controls and completion using existing AgentFS owners; verify protocol scheduling mode stays distinct from intent and two authorized clients of one Agent cannot consume each other's results, while separate Alan invocations remain independent.
- [ ] 2.2 Implement shared prefix framing across TUI and redirected input; verify `!`, `:`, nested prefix data, empty payloads, slash controls, pending responses, stdin EOF and multiline boundaries.
- [ ] 2.3 Reuse the native shell adapter with unchanged script bodies, selected shell/environment and Host cwd; verify pipelines, redirection, quotes, multiline scripts, PATH lookup and partial failures without command/path rewriting. Define the bounded standalone user `cd` parser and explicit errors for unsupported cd forms.
- [ ] 2.4 Dispatch user and Agent commands through the same governed native Tool Process path; verify model-free explicit execution, no authority amplification, sandbox scope limited to the current cwd grant, switching grants only through explicit `!cd`, descendant cancellation and correlated Action evidence.

- [ ] 2.5 Implement Process-owned cwd and ordered ordinary input admission; verify explicit `cd` ordering across two clients of one Agent and across delegated grants, failed/unsupported standalone `cd`, script-local `cd`, per-action cwd isolation and replacement of the old busy-client rejection without weakening correlation.
  - [x] 2.5.1 Replace the observed Root-terminal busy-input rejection with FIFO admission while preserving per-submission correlation across completion, interruption and Root reattachment. PR #1021 was reviewed at `11ce47ec4f5cbea345d1a87e9a70b3ae7dd43893` (Codex: no major issues), passed all current-head checks and merged as `bc7c5a4d21f4a6bf2bc28305a6b2b15f82517385`. In Herdr, a second Agent prompt was accepted after the first visibly entered its working state; the first answer appeared before the second prompt and `QUEUE_SECOND`, with no blocked error or duplicate. Ctrl+D exited the foreground Alan and released the dev-store lock. This verifies FIFO admission for one TUI client; separate-client result ownership, Process-owned cwd ordering and delegated-grant boundaries remain open under parent tasks 2.1 and 2.5.
- [ ] 2.6 Implement interrupt and paused-queue continuation/discard through runtime controls; verify pre-start cancellation, active cancellation, no dispatch after cancellation, pending request precedence and preserved completed effects.
- [ ] 2.7 Project Alan-captured command results into shared evidence and bounded model input; verify shared-cwd-relative path projection for `pwd`, diagnostics and captured stdout/stderr within the active grant, no raw Host root or `/mnt` alias in those outputs, truncation, readable references, retention gaps, exit status and a later Agent question without an automatic summary call. Also verify native `!pwd > cwd.txt` preserves shell redirection as ordinary project data, is not output-sanitized or copied into evidence, and grants no authority through the stored path string.
- [ ] 2.8 Persist recoverable queue/cwd state through existing rollout/checkpoint owners; verify explicitly selected recovery restores reliable pending work paused, unknown effects are not replayed, invalid cwd requires explicit replacement and missing records are reported.
- [ ] 2.9 Present route/cwd and truthful outcomes; verify empty-input Ctrl-D exits and shuts down the owned instance only with no pending Agent input, pending confirmation/structured input remains available on Ctrl-D, terminal-host view detach is distinct from Alan exit, redirected output stays clean, and missing response channels fail without hidden terminal input or fabricated rollback.
- [ ] 2.10 Run focused boundary checks, ordinary-terminal and Herdr acceptance, and `just quality`; record explicit-prefix slice evidence while documenting that unprefixed input remains Agent-routed.

- [ ] 2.11 Keep grant-to-native cwd/path resolution within ephemeral Host-adapter spawn/sandbox context while preserving logical service records; verify grant IDs/path strings cannot authorize access, raw backing-path metadata stays out of Alan-owned path fields and execution-path references in evidence, ordinary content in an explicitly delegated Host file remains user data, undelegated/private backing stays hidden and shell/model context is not rewritten to aP aliases.
- [ ] 2.12 Reconcile existing Linux reification with native path identity and macOS sandbox projection; verify read-only grants, outside-grant writes, symlink escape, virtual-only mounts, revocation before launch and truthful degraded-backend behavior without bypassing policy.
- [ ] 2.13 Inventory existing internal control operations and executable packaging; select the smallest task-oriented alan9 commands needed for real Agent workflows, specify exact invocation/help and result schemas, and implement thin aP clients with caller-scoped authority. Verify explicit discovery/invocation, no ambient broader connection, no duplicate state owner, commit errors, asynchronous acceptance versus completion and no replay of unknown effects; native `cat`/`q` lookup must not silently switch meaning.
  - [x] 2.13.1 Implement `/bin/agent_work` status/submit/cancel/continue/discard, invocation help, versioned receipts and action-specific Tool schema through the caller namespace. PR #973 tests cover input identity, schema/deserializer agreement, invalid targets, read-only/missing authority and mounted Process dispatch; review, merge and CI evidence is recorded under 4.1.
  - [ ] 2.13.2 Complete end-to-end Agent discovery/invocation and fault-injected commit-error/unknown-outcome acceptance; retain the parent task as incomplete until these checks and the merged delivery are verified.
- [ ] 2.14 Align structured project read/edit/search path parameters with Host shell cwd and paths through existing Host adapters. Verify Agent edit → native read/git diff and native edit → Agent read for the active grant; verify other grants remain available to Agent file tools and become shell-visible only after explicit cwd switching, while one shell action cannot span disjoint grants. Cover grant-relative and shared-cwd-relative Agent paths plus native cwd-relative shell paths, pending-buffer/save failure, stale-content conflict, read-only grants, symlink containment and revocation; no mirror copies or shell-text rewriting.
- [ ] 2.15 Review normal user flows and Agent command help: ordinary work requires neither aP terminology nor internal mount/descriptor/commit knowledge, while explicit developer inspection remains available. Clarify existing `agent_work` HELP/Tool description after observed self-report submissions: `root` means this invocation's Root Agent Process and may be the caller itself; submit queues input and does not notify an external operator. Preserve legitimate self-scheduling, caller-scoped target resolution, schemas and queue behavior; add no coordination mechanism.

- [x] 2.16 Implement the 2026-09-29 compact `: ` / `! ` markers as presentation of canonical one-shot intent; verify empty-entry typing/paste, literal embedded prefixes, explicit `:`, empty-body Backspace, accepted/reset versus rejected/preserved drafts, history recall, pending responses, multiline/resize cursor geometry and prompt-free redirected IO. Shipped in PR #1026 (`0bcdcbe5`); current full-workspace2763/0/10 and terminal evidence are indexed in `archive/2026-10-07-make-alan-self-development-ready/tasks.md`. This closes explicit prompt presentation only, not automatic routing or lifecycle task2.19.

- [x] 2.17 Reconcile lifecycle deltas and ADR references for `alan-os-host-lifecycle`, `local-alan-os-attachment`, `service-manager`, `agent-namespace-runtime`, `agent-file-layout-contract`, `alan-shell`, `alan-renderer-host-contract`, `alan-interaction-model`, `rust-inline-tui`, `host-command-plane` and `standalone-cli-distribution`. Replace channel-singleton startup, client-exit survival and cross-invocation automatic Root recovery promises with foreground ownership and explicit recovery; preserve automatic recovery only for replacement inside a live instance. Update the Ctrl-D/exit scenarios and their acceptance checks in `alan-interaction-model` and `rust-inline-tui`. Audit every existing delta for implicit restart-triggered recovery, channel-global execution and exit-survival assumptions before sync; the owner list is not an exemption for another conflicting surface. Audit and update older interview/report summaries and other active changes that depend on the superseded model.
- [x] 2.17.1 Define foreground Host lifetime, instance-scoped local attachment and explicit Root recovery in the three owning lifecycle deltas. These are target contracts; runtime and canonical lifecycle-spec sync remain pending.
- [x] 2.17.2 Reconcile the renderer, CLI, interaction-model, inline-TUI and Agent recovery deltas with foreground exit and explicit recovery. Preserve per-Agent file ownership and correlated outcomes; no runtime or canonical-sync claim.
- [x] 2.17.3 Reconcile Host command/distribution deltas and add current-direction bridges to ADR-0047/0054/0056/0058, the interview/report and cognitive-model next-planning. Preserve dated implementation evidence; defer syncing lifecycle requirements that are not implemented.
- [x] 2.17.4 Finish the cross-surface audit after these planning PRs merge; verify all active deltas, repository guidance, operator commands and acceptance checks agree before claiming lifecycle reconciliation complete. Active deltas, AGENTS.md, operator docs, lifecycle ADRs 0044, 0045, and 0056, and the canonical startup and Ctrl-D/exit scenarios in `host-command-plane`, `alan-interaction-model`, and `rust-inline-tui` are aligned in this slice. Historical archived decisions remain unchanged.
- [x] 2.18 Complete cross-invocation cwd and queue isolation, audit concurrency of shared package/connection/credential stores, and align commands that previously addressed the ambient channel Host while preserving native authorization boundaries.
- [x] 2.18.1 Ship CLI-only distribution and independent foreground startup through existing composition: each bare or redirected Agent-execution invocation owns a Root, runtime endpoint and shutdown when `ALAN_INSTANCE_RUNTIME_DIR` is unset or distinct from every other live invocation. PR #1009 merged at `faf7ee8e6b71c9f6c460e45ac049485035ec4d3b`; current-head Codex review had no findings, required CI passed, and integration tests cover independent simultaneous endpoints and shutdown.
- [x] 2.18.2 Verify that simultaneous invocations also have independent cwd and queue state; complete the shared-store concurrency and ambient-Host command audit. The concurrent CLI integration test proves inputs stay on their own AgentFS stream; a runner test proves cwd bindings are isolated even when Process IDs match. Package catalog, Connection metadata, credential writes and legacy migration use their existing cross-process locks. `alan host status/stop` require `ALAN_INSTANCE_RUNTIME_DIR`; ordinary `alan connection` commands operate on channel stores without booting or selecting a live instance. PR #1011 merged at `0e9092e1ec5092f78f797e7ec2898e068b75f2bf` from reviewed head `2dc85e87a9469c4b2f559f60cc6d0ab35035fb18`; GitHub Codex review had no findings and required current-head CI passed. PR review and CI for the remaining delivery slices remain tracked by 4.1.
- [x] 2.19 Deliver explicit recovery selection/discovery and lifecycle acceptance: chosen durable history and Action evidence, paused reliable queue, authority-validated cwd, no unknown-effect replay, clear missing-record failure, clean one-shot shutdown, cancellation of owned descendants, ordinary terminal operation and Herdr view-detach versus actual process exit. Do not claim Herdr acceptance from unit tests or absent integration access.
  2026-10-07 closure: [task 2.19 acceptance matrix](next-delivery.md#forced-termination-and-task-219-acceptance--2026-10-07)
  maps every clause to source-pinned native evidence and owning boot checks.
  Forced termination remains Unknown, repeated resume never replays it, and
  pending work requires fresh authority plus explicit continue. Current-head
  merge/CI and canonical synchronization remain 4.1/4.2.
- [x] 2.19.1 Add `alan --resume` as the explicit cross-invocation Root rollout selector while retaining instance-scoped continuation after Root replacement, even when another foreground invocation updates the channel selector. Verify default fresh startup, explicit restore, malformed/missing selection failure and restart history preservation. PR #981 ships the selector; PR #982 verifies command-result preservation and no replay after explicit resume. Exact review, merge and CI evidence is recorded under 4.1; the remaining lifecycle acceptance stays under parent task 2.19.
- [x] 2.19.2 Complete current-authority directory selection while reliable recovered work remains paused: use the existing file-native Agent Runtime control lane and Process cwd validator, publish correlated actual cwd or rejection, preserve queued inputs and paused state, reject unsettled/running selection, and connect the native project chooser without queuing its cwd choice behind recovered work. Verify invalid/revoked grants retain the previous binding and only explicit continuation executes pending work; coordinate terminal projection with canonical `alan-interaction-model` and `rust-inline-tui`.
  2026-10-07 closure: the native `/project` chooser restored current authority in
  an explicitly recovered invocation while preserving the exact pending input
  and paused state. Only `/continue` executed it once; repeat recovery retained
  the once-only outcome and no grant. The [product evidence](next-delivery.md)
  includes PID/boot, file-owned snapshots, correlated directory Actions and durable
  admission/dispatch records. Existing directory selection/publication tests
  (20 passing) cover invalid/revoked/running rejection and lost replies. This
  closes implementation and bounded acceptance; final slice review/CI remains 4.1.
  The following notes retain the earlier implementation trace:
  Read-only implementation trace confirms this lane is `machine/ctl` text →
  AgentFS `ctl:` events → real protocol `Submission`/`Op`; no `EngineControl`
  type exists. A scoped selector needs explicit protocol/control parsing plus
  classification before FIFO admission in the API pre-dispatch path and all four
  active/deferred API/namespace observer branches. Active/unsettled selection
  must reject immediately, without deferred preemption or later hidden execution.
  Settled selection updates the actual Runtime environment through the existing
  Process cwd validator, not only its queue-held clone. Reuse the existing `cd`
  Action evidence shape and durability barrier, but not the whole standalone-cd
  completion path, which changes Tape/activity to Idle and would unpause the UI.
  Acceptance must use real Shell writes to `machine/ctl` with current grants,
  exact correlated receipts, unchanged recovered queue/Paused state, rejected
  invalid/revoked/running choices, and execution only after explicit continuation.
  Backend directory-control implementation now has independent owning-crate
  regressions and two-axis review; full completion still awaits native chooser
  integration and actual paused recovery acceptance. On 2026-10-01 the sole UI
  controller submitted one TUI-only task to connect mount/revoke to this control
  lane, replacing ordinary queued cd submissions. Running, approval and unsettled
  admission must remain rejected, and authoritative correlated cwd settlement
  must precede success/revocation. No G3 or frontend completion is claimed.

- [x] 2.20 Coordinate Connection-confirmed model selection with input admission: capture the owning callable binding and resolver-owned controls for each acknowledged input; preserve earlier queued bindings across later selection, dispatch and explicit recovery; fail visibly if a captured binding is unavailable instead of silently remapping it. Install confirmed next-input bindings at a serialized admission boundary without Engine profile authority or implicit queue continuation. Owning contracts are canonical `provider-connection-contract` and `provider-request-controls`; delivered terminal acceptance is recorded in archived `2026-10-07-make-alan-self-development-ready` task4.4; an idle-only selection path does not complete this task.

  2026-10-07 closure: merged model delivery plus the
  [clause-by-clause recovery/binding audit](chatgpt-catalog-delivery.md#task-220-closure-audit--2026-10-07)
  cover exact recovery, unavailable original binding, resolved controls,
  serialized admission and no implicit continuation. Fresh owning checks passed
  (68 Engine and 18 Connection tests); canonical sync remains task 4.2.
  The following 2026-10-01 note is historical, not current missing implementation.

  On 2026-10-01 Root submitted one owning-backend author task via the live Alan
  Root in w5E:p1. It requires real provider/Runtime red regressions for A/B
  admission, selection, recovery/unavailable binding and compatible/incompatible
  steering before implementing the owner contracts. The UI author remains
  restricted to crates/tui; no picker-first or idle-only qualification is intended.
  Implementation and current-source review are still pending.

## 3. Typed intent routing and qualification — transferred

- [x] 3.1 Transfer original implementation task 3.1 to active successor [qualify-agent-input-routing, task 2.1](../qualify-agent-input-routing/tasks.md); typed integration is not implemented here.
- [x] 3.2 Transfer original corpus/budget task 3.2 to successor task 2.2; budgets and measurement remain unfinished.
- [x] 3.3 Transfer original shadow qualification task 3.3 to successor task 2.3; no provider qualification is claimed.
- [x] 3.4 Transfer original activation task 3.4 to successor task 2.4; automatic routing remains disabled and still requires explicit activation.

## 4. Review, merge and archive readiness

- [ ] 4.1 Run strict OpenSpec validation and applicable implementation checks, complete PR review and required current-head CI for each delivery slice; record the exact reviewed and merged commits.
  - PR #973: Codex found no major issues on reviewed head `c986639b8542cee7793032708dc0096f21d4d9bf`; all current-head checks passed; merged as `ff4dd79736717ce087421fe60a8c70dc919ac515`.
  - PR #981: Codex found no major issues on reviewed head `3928b50a3f1fffb06b6572335c2e04458ede4a78`; all current-head checks passed; merged as `b61400380b88acfa34208e0c0ea0debfdaa6da82`. `just quality` and strict OpenSpec validation passed.
  - PR #982: Codex review completed without findings on reviewed head `0a8177bb9d97fd941efbf4b93ffb11a6769f4245`; all current-head checks passed; merged as `54fb39a1287c9f839957029003f0695616f6532e`.
  - PR #1021: Codex found no major issues on reviewed head `11ce47ec4f5cbea345d1a87e9a70b3ae7dd43893`; all current-head checks passed; merged as `bc7c5a4d21f4a6bf2bc28305a6b2b15f82517385`.
- [ ] 4.2 Sync only implemented and merged requirements to canonical specs; verify no automatic-routing guarantee is synced merely because the explicit slice shipped.
- [x] 4.2.1 Sync the CLI-only distribution requirement delivered by PR #1009; the canonical standalone-distribution specification and strict validation are included in PR #1010.
- [x] 4.2.2 Sync shipped foreground startup, PR #1011's verified cross-invocation input/cwd isolation, and the CLI's Ctrl-D/exit lifecycle in the `alan-os-host-lifecycle`, `host-command-plane`, `alan-shell`, `alan-renderer-host-contract`, `local-alan-os-attachment`, `alan-interaction-model`, and `rust-inline-tui` canonical specs, plus ADR-0045 and ADR-0056. Explicit recovery restoration and Herdr detach acceptance remain pending.
- [ ] 4.3 Archive only after remaining work is delivered or explicitly handed to an active successor, with canonical specs synced; verify links, disposition and implementation evidence before archive.
