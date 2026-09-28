# Tasks

## Tracer bullet — execution priority accepted 2026-09-28

Complete one real terminal workflow before expanding horizontal hardening. This
milestone does not complete the full change or authorize archive.

- [x] TB1 Verify the merged baseline through the real CLI in an ordinary terminal
  and Herdr: ordinary Agent input, explicit `!`, one approved project and shared
  cwd, Agent write → shell read, shell write → Agent read, stdout/stderr and exit
  status. Exact-source live evidence is recorded below.
- [x] TB2 Verify owned exit, fresh default startup, and explicit `--resume`.
  Prior and restored Root rollout/Action evidence is recorded below. Moving the
  pane to an unfocused tab was also tested; this is not full Herdr view-detach
  acceptance, which remains open under task 2.19. The baseline integration test
  verifies completed-effect non-replay; the targeted engine test verifies
  unknown-effect gating.
- [ ] TB3 Fix only gaps observed in this workflow in independently reviewable
  PRs. Merge only after current-head Codex review and required CI pass; record
  verified evidence and clean unused merged worktrees while retaining branches.

PR #1014 remains open at `ed089a95a29b8e6d40a27e0909d68890c18da6e5`. The exact
HEAD has 31 unresolved Codex inline findings (6 P1, 25 P2), including raw path
disclosure in prose/JSON and path or URL corruption. All required checks are green,
but `reviewDecision` is empty and GitHub reports the PR blocked. Its 1,239-line
output-projection expansion (1,239 additions, 11 deletions across 5 files) is not
part of the merged baseline; retain the branch and findings, and defer this separate
path-projection work until a necessary smaller slice is isolated. This does not
waive path privacy or sandbox authority. Automatic routing and universal
output-format parsing are outside this milestone.

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

Rapidly submitting a second command while `!false` was still active produced the
visible `submit blocked: waiting for this input to complete` error and retained
the draft. Pressing Enter again after the first command settled executed the
draft once. This verifies visible rejection and recovery, not ordered queuing;
the FIFO/next-turn submission slice remains open under tasks 2.1 and 2.5.

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
- [ ] 2.15 Review normal user flows and Agent command help: ordinary work requires neither aP terminology nor internal mount/descriptor/commit knowledge, while explicit developer inspection remains available.

- [ ] 2.16 Implement `alan: ` / `alan! ` as presentation of canonical one-shot intent; verify empty-entry typing/paste, literal embedded prefixes, explicit `:`, empty-body Backspace, accepted/reset versus rejected/preserved drafts, history recall, pending responses, multiline/resize cursor geometry and prompt-free redirected IO.

- [x] 2.17 Reconcile lifecycle deltas and ADR references for `alan-os-host-lifecycle`, `local-alan-os-attachment`, `service-manager`, `agent-namespace-runtime`, `agent-file-layout-contract`, `alan-shell`, `alan-renderer-host-contract`, `alan-interaction-model`, `rust-inline-tui`, `host-command-plane` and `standalone-cli-distribution`. Replace channel-singleton startup, client-exit survival and cross-invocation automatic Root recovery promises with foreground ownership and explicit recovery; preserve automatic recovery only for replacement inside a live instance. Update the Ctrl-D/exit scenarios and their acceptance checks in `alan-interaction-model` and `rust-inline-tui`. Audit every existing delta for implicit restart-triggered recovery, channel-global execution and exit-survival assumptions before sync; the owner list is not an exemption for another conflicting surface. Audit and update older interview/report summaries and other active changes that depend on the superseded model.
- [x] 2.17.1 Define foreground Host lifetime, instance-scoped local attachment and explicit Root recovery in the three owning lifecycle deltas. These are target contracts; runtime and canonical lifecycle-spec sync remain pending.
- [x] 2.17.2 Reconcile the renderer, CLI, interaction-model, inline-TUI and Agent recovery deltas with foreground exit and explicit recovery. Preserve per-Agent file ownership and correlated outcomes; no runtime or canonical-sync claim.
- [x] 2.17.3 Reconcile Host command/distribution deltas and add current-direction bridges to ADR-0047/0054/0056/0058, the interview/report and cognitive-model next-planning. Preserve dated implementation evidence; defer syncing lifecycle requirements that are not implemented.
- [x] 2.17.4 Finish the cross-surface audit after these planning PRs merge; verify all active deltas, repository guidance, operator commands and acceptance checks agree before claiming lifecycle reconciliation complete. Active deltas, AGENTS.md, operator docs, lifecycle ADRs 0044, 0045, and 0056, and the canonical startup and Ctrl-D/exit scenarios in `host-command-plane`, `alan-interaction-model`, and `rust-inline-tui` are aligned in this slice. Historical archived decisions remain unchanged.
- [x] 2.18 Complete cross-invocation cwd and queue isolation, audit concurrency of shared package/connection/credential stores, and align commands that previously addressed the ambient channel Host while preserving native authorization boundaries.
- [x] 2.18.1 Ship CLI-only distribution and independent foreground startup through existing composition: each bare or redirected Agent-execution invocation owns a Root, runtime endpoint and shutdown when `ALAN_INSTANCE_RUNTIME_DIR` is unset or distinct from every other live invocation. PR #1009 merged at `faf7ee8e6b71c9f6c460e45ac049485035ec4d3b`; current-head Codex review had no findings, required CI passed, and integration tests cover independent simultaneous endpoints and shutdown.
- [x] 2.18.2 Verify that simultaneous invocations also have independent cwd and queue state; complete the shared-store concurrency and ambient-Host command audit. The concurrent CLI integration test proves inputs stay on their own AgentFS stream; a runner test proves cwd bindings are isolated even when Process IDs match. Package catalog, Connection metadata, credential writes and legacy migration use their existing cross-process locks. `alan host status/stop` require `ALAN_INSTANCE_RUNTIME_DIR`; ordinary `alan connection` commands operate on channel stores without booting or selecting a live instance. PR #1011 merged at `0e9092e1ec5092f78f797e7ec2898e068b75f2bf` from reviewed head `2dc85e87a9469c4b2f559f60cc6d0ab35035fb18`; GitHub Codex review had no findings and required current-head CI passed. PR review and CI for the remaining delivery slices remain tracked by 4.1.
- [ ] 2.19 Deliver explicit recovery selection/discovery and lifecycle acceptance: chosen durable history and Action evidence, paused reliable queue, authority-validated cwd, no unknown-effect replay, clear missing-record failure, clean one-shot shutdown, cancellation of owned descendants, ordinary terminal operation and Herdr view-detach versus actual process exit. Do not claim Herdr acceptance from unit tests or absent integration access.
- [x] 2.19.1 Add `alan --resume` as the explicit cross-invocation Root rollout selector while retaining instance-scoped continuation after Root replacement, even when another foreground invocation updates the channel selector. Verify default fresh startup, explicit restore, malformed/missing selection failure and restart history preservation. PR #981 ships the selector; PR #982 verifies command-result preservation and no replay after explicit resume. Exact review, merge and CI evidence is recorded under 4.1; the remaining lifecycle acceptance stays under parent task 2.19.

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
- [ ] 4.2 Sync only implemented and merged requirements to canonical specs; verify no automatic-routing guarantee is synced merely because the explicit slice shipped.
- [x] 4.2.1 Sync the CLI-only distribution requirement delivered by PR #1009; the canonical standalone-distribution specification and strict validation are included in PR #1010.
- [x] 4.2.2 Sync shipped foreground startup, PR #1011's verified cross-invocation input/cwd isolation, and the CLI's Ctrl-D/exit lifecycle in the `alan-os-host-lifecycle`, `host-command-plane`, `alan-shell`, `alan-renderer-host-contract`, `local-alan-os-attachment`, `alan-interaction-model`, and `rust-inline-tui` canonical specs, plus ADR-0045 and ADR-0056. Explicit recovery restoration and Herdr detach acceptance remain pending.
- [ ] 4.3 Archive only after remaining work is delivered or explicitly handed to an active successor, with canonical specs synced; verify links, disposition and implementation evidence before archive.
