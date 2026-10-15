# Task budget implementation slice — 2026-10-14

This tracks task 2.4 implementation on `codex/agent-task-budget-20261014`.
It is work in progress, not installed behavior or qualification evidence. The
parent planning PR is #1060; merging planning does not deliver these controls.

## Verified ledger work

The Machine-owned shared ledger reserves Tool/model admission before acknowledged
rollout publication. Concurrent users cannot spend the same allowance. Any exhausted
dimension blocks subsequent admission of either kind; the final already-admitted
operation may settle. Retries and reviewer calls must use the same model admission
boundary, rather than receiving a new allowance.

Execution time counts active work, including model/Tool waits, retries, review and
cancellation cleanup. A settled human wait does not consume execution time. Recovery
of an unclosed active interval conservatively charges at least the entire recorded
time allowance and identifies that value as an upper bound, not a measurement.
Consumed counts and increasing allowances cannot move backwards during recovery;
unfinished task identity cannot be replaced with a fresh budget.

An extension is separate from configured limits: zero leaves a dimension unchanged,
an all-zero extension is invalid, overflow is rejected and consumption never resets.
Budget confirmation defaults to reject in both the request payload and empty-response
handling. An extension must not authorize a Tool escalation or an unknown-effect replay.

Tests cover concurrency, cross-dimension exhaustion, waiting, time-limit cleanup,
real rollout recovery, failed flush, cancelled acknowledgement before/after write,
identity reuse, backwards evidence, selective extension and malformed configuration.
The latest focused `cargo test -p alan-agent-engine budget --lib` run passed 45
tests. The engine-library run after mixed-program budget continuation passed 1508 tests,
with one existing ignored test. Quality and native qualification are tracked separately;
these counts do not establish native UI acceptance.

## Runtime wiring under verification

Ordinary generation, retries, Guardian and compaction use the shared generation
admission boundary. Typed evaluation reserves model admission before allocation and
cancels/settles its operation before returning a task deadline. Native and virtual
Tools reserve admission before dispatch; remaining ordered calls are retained.
Standalone explicit `cd` uses the same boundary and resumes as a Process-directory
operation, never as an independent shell's directory change.

Budget waits are acknowledged rollout records before AgentFS publication. Recovery
rebinds them to invocation-local request IDs and current accounting, retaining
continuations across a crash after extension without adding the original allowance
twice. Extension approval grants no Tool escalation or unknown-effect replay right.
Empty/negative responses settle without generation; a completed explicit command
does not request unnecessary extra allowance. Cancellation preserves its separate
user-controlled token and stops foreground work rather than creating an extension
prompt. Already-captured background work retains its original finite allowance.

Deferred memory jobs capture the original ledger. Preemption pauses it for a
bounded retry; later foreground tasks have separate ledgers. Recovery validates
each task's monotonic evidence and selects the most recently started foreground
task, even if older background evidence was written later. Uncertain execution or
publication errors retain spending rather than silently allowing a fresh budget.

Integration regressions cover the actual submission/retry/resume path, virtual
Tool batches, real Process timeout cleanup and unknown-effect reconciliation,
typed evaluation cancellation settlement, `cd` budget resume without a shell, and
explicit-command rejection after cleanup already recorded a terminal result.

### Input and model continuity follow-up — 2026-10-15

Clean shadow-advice budget exhaustion preserves the original input on Tape and
reaches the ordinary budget pause before further model or Tool admission. A refused
evaluation records a zero-call `task_budget_exhausted` bypass; a started evaluation
retains its settled cancellation evidence. Uncertain evidence or user cancellation
still prevents dispatch. Resume continues generation without repeating the advice
or appending the input twice, including recovery from a durable budget pause into
a fresh environment. This does not qualify arbitrary crashes between queue dispatch
and the first durable Tape message; that transfer barrier remains to be audited.

Continuation controls preserve the admitted model. Budget waits record its non-secret
binding and reuse the existing Connection restoration validator during recovery;
an unavailable/revoked revision, missing binding evidence or foreign task identity
cannot silently select the current default. Hosts with fixed namespace connections
record a null selectable binding and retain their explicitly supplied environment.

Time extension requests cover already-recorded cleanup overrun plus a fresh finite
tranche, without refunding consumption. All exhausted dimensions are included in one
proposal. Declining an extension marks the task cancelled rather than completed.

### Bounded mixed-program continuation — 2026-10-15

The existing source-owner Machine program now checkpoints its ordered source prefix,
unfinished read identity, selected owner, validation cursor and evaluator identity
inside the acknowledged budget wait. Explicit budget resume advances a monotonic
program revision; the existing historical duplicate-snapshot guard remains intact.
Captured source evidence and model selections are retained instead of restarting the
program or entering ordinary generation. Final validation restarts after suspension
so previously checked source bytes and authority must still be valid in the resumed
Process. The explicit finite proposal covers the full fresh validation pass; all old
spending remains charged, and unfinished validation reads are retired before the new
pass. A resume revision permits this validation reset without weakening replay guards. Model admission is reserved once before recording
its attempt and invoking the existing bounded operation; fallback is not double charged.

Source integration tests cover repeated one-Tool extensions, recovery into a fresh
Process/namespace/rollout before evaluation and fallback, retained user selection,
changed/ungranted evaluator rejection, empty/negative replies, interrupt cleanup,
foreign checkpoints, replacement of unfinished read identity and fresh-Process
revocation/change of an already-validated first source while later sources remain
readable. An actual deadline/completion-gate regression covers a successful first
validation result retained with a zero cursor; it is not reused after suspension.
State mismatch is rejected before extending budget or consuming the request. Requests are rebound
to the new invocation; local request strings may legitimately repeat across separate
namespaces. Decline/cancellation retires an unexecuted read without dispatching it.
This does not qualify native terminal presentation or arbitrary active-crash replay.

### Descendant accounting under verification — 2026-10-15

Agent Runtime Service passes a trusted Process-bound budget binding alongside its
existing launch template. Child arguments, definitions and configuration cannot
select that binding. Launch captures the current parent task, including its
ancestry; a subsequent parent task cannot move a previously captured child onto a
fresh allowance. Every descendant model/Tool attempt charges each ancestor ledger
once before its own acknowledged admission. Concurrent children share the same
ledger locks; partial or uncertain acknowledgement never refunds earlier charges.
Child-local extension does not enlarge an ancestor allowance. Deferred parent jobs
retain their own budget but cannot reopen a completed foreground task for children.

Operations also use the shortest applicable time allowance. Parent pause or
completion stops further descendant admission and cancels in-flight operations.
The parent awaits their cleanup while its active clock continues. A durable paused
record identifies unsettled descendants; recovery conservatively charges the
recorded time allowance rather than treating that interval as a free human wait.
Budget-ledger foreground/terminal completion is not recorded until cleanup settles.

### Foreground completion publication under verification — 2026-10-15

The submission boundary withholds `TurnCompleted` events until dispatch, Tape close
and owning budget settlement succeed. Other events still stream normally. Normal
answers, empty-answer fallback and explicit interrupt do not announce completion
while descendant cleanup is held; a failed Tape close withholds the buffered
completion. This does not turn a finalization error into a successful cancellation.

The source-owner program also drains descendants before publishing its terminal
work projection or AgentFS idle state. Its new internal settling phase closes
admission while continuing to charge final evidence and IO time; recovery of an
unclosed settling interval charges the same conservative upper bound as an active
interval. Settled human waiting remains uncharged. Source integration tests cover
held cleanup, the real AgentFS activity projection, late Tape-close failure and
settling-clock recovery. They do not qualify physical terminal presentation or all
error/Process-exit paths.

Recovered descendant evidence retains its original ancestor identities. Missing or
foreign bindings fail before startup publication or model admission. Current Agent
Executable lifecycle still exits a child on a terminal result or human wait; an
exhausted/stopped ancestor fails the child closed rather than offering a child-local
extension for someone else's ledger. Automatic descendant restart, subtree recovery
and a correlated parent/child continuation UX are not delivered by this slice; do
not use a failed child or Root fallback to qualify delegated success.

### Agent Process exit boundary under verification — 2026-10-15

The existing Process cleanup guard now closes its trusted budget binding even when
Kernel control aborts the runtime future. Every captured ledger for that Process,
including old deferred work, shares the exit signal; child and grandchild bindings
inherit cancellation. New admission and resume fail after exit, while an admitted
descendant operation uses its existing cancellation/cleanup path before reporting
failure. This cannot grant a child-local extension for a dead ancestor. It does
not rewrite an interrupted ledger as completed or refund consumed calls/time;
existing recovery conservatively accounts for its unclosed active interval.

Each replacement Root Agent Process receives a fresh live binding before explicit
recovery validates the original durable budget. The exited Process's captured
work remains stopped. Source integration covers both Service Manager-recorded exit
and direct `/proc/<pid>/ctl` cancellation while a real child Agent Process holds a
model call, settled model cleanup, no retry/replay and a replacement Root reaching
readiness. Ledger tests cover old/current allowances, grandchildren and cleanup
time. These are source tests, not physical terminal or arbitrary host-effect
cleanup acceptance. Correlated subtree recovery and inherited-stop UX remain open.

### Dropped runtime operation retirement under verification — 2026-10-15

Root's own in-flight work also needs retirement when Kernel abort drops the runtime
future. Source regressions reproduced a live model stream and an executing Tool
Process surviving that drop. Allocated generation and Tool operations now retain
adapter-owned cancellation guards; evaluation reuses the same tracked Drop cleanup.
Admission is checked before allocation/spawn. A guard retains a returned identity
before rechecking scope, so uncertainty racing allocation cannot leave a known PID
or operation unguarded. Evaluation also rechecks before commit. A review regression
first observed namespace IO before uncertain Tool admission was rejected; it now
covers Tool, generation and evaluation with zero IO, plus retirement of a captured
identity when uncertainty arrives during allocation.
Confirmed terminal results disarm the guards. Dropped operations issue bounded
existing `abort`/`cancel` control writes without rerunning the effect.

Runtime exit waits for these cleanup jobs before closing its rollout writer.
Timeout, failed control acknowledgement or a panicked cleanup job retains an
unconfirmed-cleanup error; it does not turn interrupted work into a completed task.
A refused abort of an already-terminal model requires a confirming status read.
The scoped tracker owns only retirement jobs, not Agent execution or Process
lifecycle; existing Kernel, Runtime and rollout ownership remains unchanged.
The existing tokio-util dependency enables its task-tracking feature; no package
version or new package is introduced.

Source integration now stops an established model stream and an executing registered
Tool after direct Root Kernel cancellation. Additional tests hold the control
acknowledgement, fail/time out/panic cleanup, check closed admission and confirm
runtime exit still drains/closes its writer while returning uncertainty. These do
not qualify arbitrary OS effects or allocation/dispatch identity-loss windows.
Interrupted execution evidence remains subject to existing explicit recovery and
unknown-effect reconciliation, never automatic replay.

## Known qualification gaps

Task 2.4 remains unchecked. The existing source-owner program has budget continuation
coverage; this does not implement a general mixed-program scheduler. Uncertain model
or publication settlement remains fail-closed; fallback deadline/cleanup uncertainty
and active-crash transfer edges still need acceptance. Input-shadow budget pause/resume
has source integration coverage; physical terminal acceptance remains pending. Descendant
admission and cleanup accounting now have implementation under verification;
native task-tree acceptance, inherited-stop presentation and explicit subtree recovery
remain open. Whole-transition
time boundaries and all cancellation/error paths still need acceptance. Native UI,
installed-binary and task-tree qualification are pending; source test counts remain
separate from those acceptance claims.

## Remaining integration, in order

- [ ] Finish start/reuse lifecycle coverage before input shadow evaluation, generation, explicit
  command or mixed-Machine evaluation; queue-only/control input must not create a
  fresh task allowance. Only a settled terminal task may receive a new identity.
- [ ] Bind retries, Guardian, compaction, typed evaluation and child work to finite
  accounting. Capture a deferred memory job's original budget; it must not consume
  a subsequent task's allowance or continue unmetered after foreground completion.
- [ ] Admit every Tool attempt, including virtual Tools; preserve the remaining
  ordered batch on exhaustion. A time limit must cancel and await existing cleanup,
  reconcile completed/unknown effects and never replay an already completed effect.
- [ ] Persist the correlated budget wait and exact continuation before UI publication;
  restore them through new invocation-local request IDs during explicit recovery.
  Do not equate budget approval with the original Tool/effect approval.
- [ ] Resolve extension before consuming the pending request. Default/negative
  responses must not resume model generation. Persistence failure retains the wait
  and prevents execution; duplicate/stale responses cannot add allowance twice.
- [ ] Settle active time at every human wait, cancellation and terminal path,
  including direct-command and deferred-work paths. Preserve any already-existing
  permission wait rather than silently replacing its authority decision.
- [ ] Run fake-provider/Tool integration, recovery and cancellation tests; then the
  complete engine suite, quality gate and isolated native pause/extension flow.
- [ ] Only after runtime verification, launch the frozen two-repository read-only
  preflight pilot with enforced limits and retain every failed attempt. Do not mark
  task 2.4 complete or count these pilot audits as formal real development tasks.

The provisional code defaults (1800 execution seconds, 256 Tool attempts, 64 model
requests) are not frozen daily-use qualification thresholds. The initial pilot plan
uses 600 seconds, 30 Tool attempts and 10 model requests per task, with zero task
retries; that plan has not started a model task. It remains paused at preflight until
the runtime can enforce all three dimensions. No monetary cost is inferred from an
admission counter.
