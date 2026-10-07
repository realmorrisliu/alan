# Mounted evaluation caller entry — 2026-10-07

The namespace runtime now provides a finite-choice caller over the existing
mounted Connection. Allocation returns an operation identity without writing
`data`; the Machine can acknowledge its started record before consuming the
operation through `commit`. Commit consumes the handle and never retries a model
call or dispatches command, Tool or fallback work. The caller is not yet wired to
the Machine's input transition or durable event producer.

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

Remaining work: connect this caller to the Machine's two acknowledged durability
barriers and live read-only publication, prove both admission surfaces without
executing corpus commands, then collect and score the frozen real-model corpus
and baselines. No task is marked complete solely by this API entry.

Validation: 50 namespace runtime tests passed, including eight evaluation
fixtures. Independent Spec and Standards review passed after correcting allocator
acknowledgement uncertainty, bounded cleanup and typed failure classification.
