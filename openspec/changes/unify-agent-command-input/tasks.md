# Tasks

## 1. Planning and contract reconciliation

- [x] 1.1 Record confirmed interview decisions, ADR draft and glossary; verify links and distinguish target behavior from the current implementation.
- [x] 1.2 Produce proposal/design and owning deltas, including old task-lease, bash-request and generation-only conflicts; verify complete modified requirement blocks preserve unrelated scenarios.
- [x] 1.3 Final shared-understanding confirmation received on 2026-09-24; ADR-0058 and disposition record accepted direction, with runtime implementation still pending.

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
  - [x] 2.13.1 Implement `/bin/agent_work` status/submit/cancel/continue/discard, invocation help, versioned receipts and action-specific Tool schema through the caller namespace. PR #973 local tests cover input identity, schema/deserializer agreement, invalid targets, read-only/missing authority and mounted Process dispatch; full quality passes. Merge gates are tracked by 4.1.
  - [ ] 2.13.2 Complete end-to-end Agent discovery/invocation and fault-injected commit-error/unknown-outcome acceptance; retain the parent task as incomplete until these checks and the merged delivery are verified.
- [ ] 2.14 Align structured project read/edit/search path parameters with Host shell cwd and paths through existing Host adapters. Verify Agent edit → native read/git diff and native edit → Agent read for the active grant; verify other grants remain available to Agent file tools and become shell-visible only after explicit cwd switching, while one shell action cannot span disjoint grants. Cover grant-relative and shared-cwd-relative Agent paths plus native cwd-relative shell paths, pending-buffer/save failure, stale-content conflict, read-only grants, symlink containment and revocation; no mirror copies or shell-text rewriting.
- [ ] 2.15 Review normal user flows and Agent command help: ordinary work requires neither aP terminology nor internal mount/descriptor/commit knowledge, while explicit developer inspection remains available.

- [ ] 2.16 Implement `alan: ` / `alan! ` as presentation of canonical one-shot intent; verify empty-entry typing/paste, literal embedded prefixes, explicit `:`, empty-body Backspace, accepted/reset versus rejected/preserved drafts, history recall, pending responses, multiline/resize cursor geometry and prompt-free redirected IO.

- [ ] 2.17 Reconcile lifecycle deltas and ADR references for `alan-os-host-lifecycle`, `local-alan-os-attachment`, `service-manager`, `agent-namespace-runtime`, `agent-file-layout-contract`, `alan-shell`, `alan-renderer-host-contract`, `alan-interaction-model`, `rust-inline-tui`, `host-command-plane` and `standalone-cli-distribution`. Replace channel-singleton startup, client-exit survival and automatic Root recovery promises with foreground ownership and explicit recovery; preserve file-native renderer and service ownership. Update the Ctrl-D/exit scenarios and their acceptance checks in `alan-interaction-model` and `rust-inline-tui`. Audit every existing delta for implicit restart-triggered recovery, channel-global execution and exit-survival assumptions before sync; the owner list is not an exemption for another conflicting surface. Audit and update older interview/report summaries and other active changes that depend on the superseded model.
- [x] 2.17.1 Define foreground Host lifetime, instance-scoped local attachment and explicit Root recovery in the three owning lifecycle deltas. These are target contracts; runtime and canonical sync remain pending.
- [ ] 2.17.2 Reconcile renderer/CLI/Host command/distribution deltas, ADR references and dependent active planning against these lifecycle contracts.
- [ ] 2.18 Implement independent foreground startup through existing composition; remove default background process launch/attach and automatic channel-wide recovery selection. Verify two simultaneous invocations have independent Root identity, cwd, queue and shutdown, and audit concurrency of shared package/connection/credential stores. Align distribution and commands that previously addressed the ambient channel Host; preserve native authorization boundaries.
- [ ] 2.19 Deliver explicit recovery selection/discovery and lifecycle acceptance: fresh default launch, chosen durable history and Action evidence, paused reliable queue, authority-validated cwd, no unknown-effect replay, clear missing-record failure, clean one-shot shutdown, cancellation of owned descendants, ordinary terminal operation and Herdr view-detach versus actual process exit. Do not claim Herdr acceptance from unit tests or absent integration access.

## 3. Typed intent routing and qualification — transferred

- [x] 3.1 Transfer original implementation task 3.1 to active successor [qualify-agent-input-routing, task 2.1](../qualify-agent-input-routing/tasks.md); typed integration is not implemented here.
- [x] 3.2 Transfer original corpus/budget task 3.2 to successor task 2.2; budgets and measurement remain unfinished.
- [x] 3.3 Transfer original shadow qualification task 3.3 to successor task 2.3; no provider qualification is claimed.
- [x] 3.4 Transfer original activation task 3.4 to successor task 2.4; automatic routing remains disabled and still requires explicit activation.

## 4. Review, merge and archive readiness

- [ ] 4.1 Run strict OpenSpec validation and applicable implementation checks, complete PR review and required current-head CI for each delivery slice; record the exact reviewed and merged commits.
- [ ] 4.2 Sync only implemented and merged requirements to canonical specs; verify no automatic-routing guarantee is synced merely because the explicit slice shipped.
- [ ] 4.3 Archive only after remaining work is delivered or explicitly handed to an active successor, with canonical specs synced; verify links, disposition and implementation evidence before archive.
