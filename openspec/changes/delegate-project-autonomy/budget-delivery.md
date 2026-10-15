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

## Isolated native budget acceptance — 2026-10-15

A clean `948e51e4` binary ran through Herdr with isolated Host/System Stores, an
empty project directory and no Host project grant. A real read-only namespace task
paused at one admitted model request before Tool dispatch. Exit and explicit
`--resume` preserved the same task, one model call, zero Tools and 3302ms charged
time; recovery itself made no new call. Rejecting extension stopped the task with
the same consumption. This is native acceptance work, not a pilot or formal
daily-use task, and the installed product was not changed.

The run exposed duplicate budget text and lost confirmation controls: the file
renderer classified `task_budget_extension` as custom input. Source regressions
also reproduced omission of the runtime's default option from AgentFS. The
follow-up recognizes the existing budget request kind as confirmation, projects
the runtime-selected default, and shares a compact consumed/limit/proposal summary
between fresh and recalculated recovery waits. Zero increases are omitted; recovered
upper-bound time remains labeled. A recovered native wait also initially showed
`model unknown` until the response despite retaining the admitted binding; that
model-observation gap is addressed in the follow-up below. The initial fixture's missing
`memory.strict_store` was corrected before any model request and retained separately.

A fresh clean `d3da75e0` candidate verified the revised display in the same isolated
Herdr fixture. Its pause showed each summary line once, all three consumed/total
allowances and the waiting-for-approval cue. Exit and explicit recovery retained
one model call, zero Tools and 2767ms without dispatch. Numeric `2` approved the
proposed increase: the model limit rose from one to two with consumption still one.
The retained Tool was attempted once and charged; it was denied because no Host
project directory was authorized. The second model call accurately reported that
failure. Final accounting retained the same task, two models, one Tool attempt and
6954ms, with foreground completion. The namespace-listing task did not succeed;
this proves budget controls, not a successful development task or wider authority.
Both earlier native candidates are stopped. At that checkpoint fixture model
admissions were three, with no pilot or formal qualification task. Empty-input
rejection had source coverage; its later native acceptance is recorded below.

CI for `948e51e4` failed an older cause-read timeout test on its one-second wall
clock assertion, then cancelled the paired Linux job. The test now uses Tokio's
paused clock for the same timeout and descriptor-cleanup checks; the runtime's
100ms bound is unchanged. New-head CI must pass independently.

## Recovered waiting-task model observation — 2026-10-15

The native `model unknown` observation was reproduced through Runtime startup:
model A reached its budget pause, next-input selection changed to B, and a fresh
Runtime/AgentFS recovered the wait with no active input. Its initial model snapshot
had next-input B but no retained A. The Machine had already restored and validated
A; the existing observation path only considered currently executing inputs.

The existing Process-local model publisher now derives its paused budget binding
from the Machine-owned wait at stable loop boundaries and before Ready. It exposes
that binding only when it matches the already-restored callable. Model selection
and actual running activity keep their existing owners; no queue input or running
state is synthesized. Resolving or cancelling the wait clears this projection.
Missing, null, malformed or mismatched binding evidence cannot infer a model from
the current selection. The observation includes only the existing safe model and
reasoning fields, without Connection restore revisions or credential references.

Source checks cover separate next-input B, rejection and explicit extension under
the retained A controls, cleanup after completion and invalid observation evidence.
The full workspace passed 3148 tests, with zero failures and 15 existing ignored
tests across 100 suites; both independent review axes passed the source snapshot.

An isolated Herdr candidate built from reviewed tree `d3f579bf` requested only the
internal plan Tool. It paused before dispatch at one model call, zero Tools and
3049ms. Exit and explicit `--resume` retained the same task and spending, and the
first waiting prompt showed `gpt-6.1-sol / medium` without another model request or
invented running activity. Empty Enter defaulted to rejection, preserving those
counts and executing no Tool. The fixture task was deliberately cancelled, not
counted as successful development. The invocation is stopped; total fixture model
admissions are now four, with pilot/formal starts still zero. This does not qualify
subtree recovery or broaden the existing budget slice. Final source identity and
binary equality are retained with the per-head evidence receipt.

## Retry backoff deadline follow-up — 2026-10-15

The integration audit found that streaming generation and the shared reviewer/memory
generation path checked budget at each model admission but slept through the next
retry delay without its deadline. Source tests reproduced a settled retryable failure
near a one-second limit returning after that limit. Both paths now use one existing
generation-adapter wait backed by the same ledger's cancellation/deadline operation;
waiting reserves no additional Tool or model call. Unbudgeted adapter callers retain
their existing bounded retry count and cancellation behavior. Two paused-clock source
regressions cover the foreground pause and nonstream exhaustion, one admitted model,
zero Tools and no next attempt. This is source timing acceptance, not a live-provider
latency measurement or native reviewer qualification.

## Active cancellation control follow-up — 2026-10-15

The next audit reproduced normal generation cancellation waiting indefinitely for
the control acknowledgement, before its bounded Drop cleanup could run. Active model
and Tool cleanup now use the same adapter-owned one-second retirement attempt as
dropped operations. Error settlement is awaited before a model retry. A rejected
redundant model abort still requires a terminal-status read within that bound.
Timeout, failed acknowledgement or a dropped/panicked attempt keeps the operation
scope unconfirmed and rejects later model/Tool admission. Tool errors retain their
PID and observation category. No effect is redispatched by retirement.

The typed uncertainty is preserved through retry classification, reviewer handling,
compaction, generation cancellation and source-owner work. The accepted-submission
completion gate also rejects a successful result when its operation scope is
unconfirmed, including best-effort paths that previously hid cancellation errors;
it does not publish a normal completion or a clean budget-extension request.

Twenty-one focused source cleanup tests pass. New held-acknowledgement regressions cover
both generation entry points, zero provider requests, closed subsequent admission
and withheld cancellation completion, plus an actual Tool Process retaining its PID
and cancellation category after its runner stops. Existing cases cover acknowledged controls,
failed/timed-out/panicked retirement, terminal model confirmation, real Tool/Process
cleanup and descendant completion. These are source fixtures, not native fault
injection or proof that arbitrary Host effects have stopped. Allocation before an
identity is captured and active-crash transfer windows remain separate gaps.

The first workspace run caught an uncertainty marker hidden inside an `anyhow::Error`
context; independent correctness review identified the same defect. Retirement now
returns its concrete uncertainty type, so both adapter paths retain typed detection
alongside their original error. The cause-cancellation regression and both held-ack
paths verify that marker and non-retry behavior. The original failed run is retained.

## Known qualification gaps

### Unidentified allocation follow-up — 2026-10-15

The next regression reproduced an allocation lease being dropped without a captured
operation identity while its scope still permitted admission and confirmed cleanup.
Both explicit retirement and Drop now retain typed uncertainty in that scope when
identity is absent. Existing Tool, generation and typed-evaluation admission checks
therefore reject subsequent namespace IO. This is deliberately conservative even
when allocation may have failed before producing an effect. It does not discover
an unknown PID, prove an unidentified operation stopped, or qualify crash recovery.
Known-identity retirement retains its existing finite control path.

Independent review found that checking the scope only for successful results could
still turn an allocation error into a clean budget pause on the next retry. A real
LLM File-Server allocation/read-failure regression reproduced that false success.
The shared submission boundary now checks the scope on both success and failure,
keeping its concrete uncertainty underneath the original error context. Scope
closure also preserves that typed uncertainty. The integration fixture exercises
one-model exhaustion and cancellation after allocation, with no provider request,
no extension wait and no terminal completion. This does not locate or stop the
unidentified allocation.

Clean `b80e85ca` native acceptance also resolved the apparent startup failure: queue
`known:false` before first admitted input is existing evidence behavior, explicitly
covered by the queue projection suite. The terminal accepted the first text-only
task and returned exactly 1–500, with one model, zero Tools and 17318ms retained
accounting; the invocation exited cleanly. Ctrl-C arrived after task completion,
so this is not running-cancellation acceptance. The earlier no-task probes and
supervisor timeout remain recorded. Five native model admissions have occurred
across these isolated checks; pilot/formal tasks remain zero. Initial queue labeling
is an observed UI clarity issue, not evidence of a startup regression.

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
