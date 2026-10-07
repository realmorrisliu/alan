# Finite-choice Connection entry delivery — 2026-10-07

Scope: provider-neutral finite-choice DTOs, explicit callable capability (default
unsupported), independent llmfs `evaluate` allocation and quota, strict request
validation, captured Connection provenance, single dispatch, typed Selected/NoMatch,
deadline and cancellation. Generation retains its version-2 request DTO. No actual
provider advertises evaluation yet; no Machine consumer or automatic dispatch is
connected by this slice.

Both operations reuse existing lifecycle/terminal fencing. Generation startup now
includes provider-lock waiting in its cancellation race. The regression failed on
the original implementation, then passed after moving lock acquisition inside the
race. Unknown shared cost fields now return null; the dead zero-valued counter was
removed. No production reader of those cost fields existed at review time.

## Verification

- `cargo test -p alan-llm`: 204 unit tests plus integration/doc tests passed;
  credential-dependent live tests remain ignored and are not claimed as evidence.
- `cargo test -p alan-llmfs`: 52 tests passed (4 local, 4 evaluation boundary,
  44 generation boundary). This includes malformed input, unknown selection,
  NoMatch, independent quota, captured old Connection, duplicate commit, timeout,
  provider-lock cancellation and returned-result publication cancellation.
- The late-result regression explicitly polls abort and publication futures to
  Pending behind the held FIFO terminal mutex. Removing the production terminal
  guard makes it fail on usage leakage; restoring the guard makes it pass.
- `openspec validate add-cognitive-model-routing --strict` passed.
- Independent Standards and Spec review passed on implementation patch SHA-256
  `da3d0691fbaed0d7c96ff2ff81d7060f4c6c770e812986fce302ad4ce520a691`.
  The initial Spec finding (missing late-publication race test) was fixed and
  re-reviewed. These are local reviews, not external PR approval.
- Full repository quality gate passed before the final review fixes; the final
  staged snapshot must also pass the mandatory commit hook before delivery.

Logs live under `~/Library/Caches/Alan/`: `typed-entry-final-tests.log`,
`typed-entry-fence-tests.log`, `typed-entry-fence-mutation.log`,
`provider-lock-regression.log`, `typed-entry-quality.log` and
`typed-entry-commit.log`. The frozen qualification corpus/budgets were committed
in `020779a7` before any candidate measurements; this slice performs no such
measurements.

## Remaining gates

Verify a real adapter protocol and live typed result; connect Machine-owned shadow
observation and durable evidence on both admission surfaces; verify recovery of
unfinished evaluations; collect the frozen 324 observations and both baselines;
score correctness, failure denominators, latency and verified cost. Unknown billing
cannot pass the cost gate. Keep tasks 2.2 onward open; PR merge, canonical sync and
archive are also pending. User authorization is still required before enabling
automatic execution, even if all shadow gates later pass.

## Full-workspace steering boundary correction — 2026-10-07

Full workspace verification at `9defc084` found that the shadow integration had
made the steering leaf receive `RuntimeLoopState`, violating the existing
`transition_leaf_workflows_do_not_receive_the_runtime_loop_aggregate` contract.
The leaf now receives only Agent Machine and namespace environment. Shared shadow
observation/dispatch remains in its existing module; ordinary and steering paths
reuse it with unchanged durable observation-before-dispatch, cancellation,
publication, requeue and skipped-Tool behavior. Callers pass the remaining Tool
slice directly instead of the complete batch and a separate index.

The unchanged boundary suite passes all 20 tests. The full workspace rerun passes
2,824 tests with zero failures and 13 existing ignored tests across 95 result
summaries. Spec and Standards review passed without weakening assertions.
Logs: `~/Library/Caches/Alan/routing-shadow-current-workspace-tests.log` (failure),
`shadow-leaf-boundary-tests.log` and
`routing-shadow-current-workspace-tests-fixed.log` (successful rerun).
This is local current-source evidence, not remote CI or a new real-model
qualification result; the v1 routing failure and disabled activation remain.

## Rollout-local response identity correction — 2026-10-08

PR #1035 review reproduced a response lookup colliding with copied history when
a fresh AgentFS reused a request id after recovery. Both the Runtime lookup and
Machine persistence lookup now scope response observations to their owning
rollout. Ordinary submission UUID reconciliation remains across recovered
history, preserving the existing no-repeat evaluator contract.

The existing bypass regression now reuses a request id with identical and then
different bytes across successive recovered rollouts. Each new response records
one current-rollout observation; duplicate delivery records no additional event,
changed bytes within the same rollout still fail, and evaluator calls remain zero.
Before the fix the identical response retained the old rollout identity. Full
Agent Engine and Service Manager tests pass, including ordinary recovered-input
deduplication and the existing writer's rejected re-admission checks. Logs:
`~/Library/Caches/Alan/shadow-response-scope-{red,tests}.log`.

The separate evaluation-default review claim was disproved through the actual
CLI in an isolated store at `e1ec537b`: default selection of an evaluation-only
profile exits 1, leaves metadata byte-for-byte unchanged and retains the generation
default on a subsequent metadata read. The CLI writes through Connection Service;
its existing replace-metadata validation rejects this before commit, not merely
at boot. No duplicate production guard was added. Receipt:
`~/Library/Caches/Alan/eval-default-review-rfwdqyzf/rejection-confirmation.json`.

These checks do not replace frozen v2 measurements or qualify the changed
current Machine source. Automatic routing remains disabled; any later candidate
qualification must use a fresh freeze and complete comparison.
