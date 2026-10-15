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
tests. The engine-library run after input/binding lifecycle fixes passed 1499 tests, with one
existing ignored test. Quality and native qualification are tracked separately;
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

## Known qualification gaps

Task 2.4 remains unchecked. Mixed-Machine owner work still needs an exact budget
continuation rather than losing its program state or entering ordinary generation.
Input-shadow budget pause/resume now has source integration coverage; physical
terminal acceptance remains pending. Descendant Agent Process accounting
needs an explicit aggregate or delegated finite-allowance contract and acceptance;
independent per-Process defaults do not prove a task-tree cap. Whole-transition
time boundaries and all cancellation/error paths still need acceptance. Native UI,
installed-binary and complete repository tests are not yet qualified.

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
