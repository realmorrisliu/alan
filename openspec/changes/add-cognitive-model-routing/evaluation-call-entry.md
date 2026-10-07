# Mounted evaluation caller entry — 2026-10-07

The namespace runtime now provides a finite-choice caller over the existing
mounted Connection. Allocation returns an operation identity without writing
`data`; the Machine can acknowledge its started record before consuming the
operation through `commit`. Commit consumes the handle and never retries a model
call or dispatches command, Tool or fallback work. The caller now has an opt-in ordinary-input Machine consumer with acknowledged
start/terminal records. Host selection of the evaluator profile remains unfinished.

One monotonic deadline spans allocation, the intervening durability barrier and
commit/result validation. Cancellation and an exhausted deadline before commit
abort the allocated operation without calling its evaluator. The remaining budget
is sent to llmfs, which includes its provider-lock wait. Results validate version,
schema, captured provider/model, finite selection membership, bounded JSON and
nonnegative consistent token usage. Unknown billing remains null.

The data descriptor intentionally does not use a cleanup guard that clunks on
cancellation: clunk commits a request. Error cleanup first aborts and waits for
acknowledgement, then releases any buffered data descriptor. If abort cannot be
confirmed, the descriptor remains uncommitted until server teardown and the error
retains its original cancellation/timeout classification with uncertainty context.
Dropping an allocated handle requests bounded best-effort abort; this never commits
data. If allocation open/read confirmation is lost, bounded re-reading of the
same allocator reconciles its identity without opening another operation. If
identity or abort remains unconfirmed, the error explicitly retains uncertainty.
Cleanup clunks are bounded as well. There is no implicit evaluation retry after
lost acknowledgements.

Mounted fixtures cover delayed commit with zero initial calls, typed command
advice with one evaluator call, expired durability budget, explicit abort,
in-flight cancellation, captured-model mismatch, and cancellation after a complete
request was buffered but before its write acknowledgement returned. The latter
asserts zero evaluator calls and release after abort. Additional gates cover
lost allocation open/read acknowledgements and blocked cleanup acknowledgement. They use actual llmfs with
fixture evaluators; they are not real-provider routing measurements.

Remaining work: wire explicit evaluator profile selection through the Host and
Connection Service, prove both admission surfaces without executing corpus
commands, then collect and score the frozen real-model corpus and baselines. No task is marked complete solely by this API entry.

Validation: 50 namespace runtime tests passed, including eight evaluation
fixtures. Independent Spec and Standards review passed after correcting allocator
acknowledgement uncertainty, bounded cleanup and typed failure classification.

The Machine now owns an acknowledged observation writer over its existing rollout
recorder. It validates the typed payload and complete prior evaluation sequence,
requires storage, binds a new attempt to the current rollout, and rejects changing
an existing submission's identity, including after recovery into a new rollout.
Identical records return `false`; callers must not interpret that as permission
to commit another model call. Only acknowledged evidence updates the Machine snapshot. Duplicate records
reconcile the latest acknowledged state without appending records or moving the
latest observation backwards. A terminal transition requires a live acknowledged
start in the current rollout; recovered or uncertain starts cannot be continued.
Interrupted is reconstructed by recovery, never manufactured as a provider outcome.

The writer reuses durable history for duplicate reconciliation. This is a bounded
shadow-entry implementation choice, not a new Machine history store. Ordinary FollowUp/Steer dispatch now invokes the consumer only when a separately
captured evaluator was explicitly injected into its namespace environment.
Agent Runtime Service supplies the AgentFS owner publication callback before
startup, so recovery and live snapshots share the same owner. Writer tests
cover repeated records, conflicting terminal outcomes, changed recovery identities,
missing storage, and failed start/terminal persistence with recovery. They do not
establish end-to-end paid-call ordering or the two-surface qualification gates.

The six evaluation tests passed, including lost start/terminal acknowledgement
reconciliation. Independent Spec and Standards reviews passed after closing
recovered-attempt continuation and stale-projection reconciliation gaps.

The consumer preserves the exact single text body of ordinary Agent input. It
allocates only after reconciling previous evidence for the submission, persists a
start before commit, publishes the acknowledged start, consumes one evaluation,
and persists a terminal before publication. The original generation capture and
normal dispatch branch are unchanged: even command advice does not select command
execution. Explicit intents and pending/control input bypass the evaluator;
durable bypass evidence and original-byte correlation across real clients remain
unfinished. No default environment contains a shadow evaluator.

Mounted consumer tests cover accepted-submission dispatch with command advice,
repeat/recovery without another evaluator call, both failed persistence barriers,
cancellation before result publication, and explicit-intent/control bypass.
These engine fixtures are not the frozen two-client real-model qualification.

All three dispatch consumers (initial input, brokered follow-up, and active Tool
batch steering) share the same shadow boundary. Preflight reconciliation and
start publication obey the attempt deadline and cancellation. Terminal evidence
and publication have one bounded one-second settlement window, including recording
an already expired/cancelled operation; a successful result remains cancellable
until publication. If its write acknowledgement is lost, no live success is
published. Later reconciliation may expose a reliably recorded historical terminal
without repeating its model call. Typed abort uncertainty is propagated unchanged;
it leaves the acknowledged start unsettled instead of inventing confirmed cancellation.
Seven mounted consumer tests cover these dispatch and publication windows.

Current integration validation: 1,371 engine unit tests passed (one opt-in test
ignored), 142 Service Manager tests passed, and strict OpenSpec validation passed.
Independent Spec and Standards reviews passed on the integrated dispatch path.
