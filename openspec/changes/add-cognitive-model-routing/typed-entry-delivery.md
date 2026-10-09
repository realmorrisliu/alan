# Finite-choice Connection entry delivery — 2026-10-07

## Current delivery status — 2026-10-09

PR #1036 merged the bounded typed-entry canonical synchronization at `1c53cf52`.
PR #1039 then delivered explicit structured source-owner work at `90a05fe5`, and
PR #1040 synchronized only its bounded requirements at `498f97e9`; see
[mixed-task-acceptance.md](mixed-task-acceptance.md). The dated receipt below
retains its earlier preparation state. General composition and package exposure
remain unfinished; input-routing qualification failed through v4 and automatic
routing remains OFF, as recorded in [v4 results](../qualify-agent-input-routing/native-baselines-v4-results.md).

## Merge and synchronization receipt — 2026-10-08

PR [#1035](https://github.com/realmorrisliu/alan/pull/1035) merged at
`cb0ec7ec7746a8f5b2af77782b281bbce7a838f0`, final head
`ac0a8af5b200dcd26c95accd3a6032116e919cce`. The PR head had only the label
check. Post-merge main `cb0ec7ec` now has all 15 reported checks passing:
all 12 jobs in [CI run 37709563067](https://github.com/realmorrisliu/alan/actions/runs/37709563067),
[CodeQL](https://github.com/realmorrisliu/alan/actions/runs/37709563095),
Cargo Audit and release draft. This is combined-code acceptance, not new live
routing qualification. Final local acceptance and review corrections are recorded
below with their own sources and logs.

Delivered through the existing Connection owner: `alan-llm` finite-choice types
and TypeSafe adapter; llmfs independent `evaluate` allocation, capability,
quota, commit/terminal fences and typed failures; Host-backed credential and
generation-binding protections. Agent Machine consumes captured advice through
the namespace, persists before commit and terminal publication, and exposes a
read-only `machine/evaluation` projection. `alan --shadow-evaluator <profile>`
selects it explicitly for one invocation; children do not inherit it. Prefixes
and current responses bypass it. Advice never selects command, Tool or fallback
dispatch. Interrupted recovery does not repeat the evaluator or settle queued work.
The owning deltas contain the focused boundary and recovery scenarios.

The closure patch synchronizes only delivered requirements in `llm-file-server`,
the TypeSafe requirement in `provider-request-controls`, the bounded snapshot in
`agent-file-layout-contract`, both shadow requirements in `agent-namespace-runtime`,
the invocation option in `alan-shell`, and current documentation status.
General `cognitive-model-routing`, structured-only task completion, capability
exposure and effecting routing deltas remain unsynchronized and unfinished.
Broader tasks remain open; both changes stay active rather than being archived.

Closure review passed independently on Spec and Standards axes. Review corrected
an old delta that implied a runtime live-probe publication gate: fixture evidence
alone cannot establish provider support, and successful publication alone cannot
qualify routing. No new runtime gate is added. Pinned OpenSpec 1.4.1 validates all
67 surfaces strictly; current-surface guard/fixtures and the complete mandatory
quality/distribution commit gate pass. The synchronization PR still requires its
own current-head checks and merge before canonical closure is complete.

Frozen v2 was measured at `1f551ad9`, before later review fixes: 34 false-command
classifications, 75.76% typed accuracy, observed Machine p95 2684 ms. It failed
the frozen gates; full admission timing, generation billing and 12 redirected
pending-response slots remain incomplete. These are historical candidate results,
not scores for the merged source. Automatic routing stays disabled. Future
activation requires a fresh freeze, full baseline comparison, passing gates and
explicit authorization; see
[shadow-qualification.md](../qualify-agent-input-routing/shadow-qualification.md).

## Historical entry slice

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

## Malformed evaluator error provenance — 2026-10-08

PR #1035 review reproduced adapter validation failures losing their category
through the generation-oriented diagnostic formatter. Introduce a provider-neutral
malformed-response error marker, apply it to TypeSafe response decoding and
response-size validation, and preserve it through the existing llmfs terminal
event and Namespace evaluation decoder. Transport/HTTP failures remain unavailable;
generation diagnostics, timeout/cancellation and the terminal fence are unchanged.
No error text is parsed or copied into namespace evidence.

Local HTTP regressions cover invalid JSON, model mismatch, invalid distribution
and oversized bodies, with one request and no credential/body leakage. The
existing mounted-operation regression covers invalid selections, provider outages
and the new typed failure without turning settled terminals into abort uncertainty.
A Machine regression verifies durable malformed outcome, unknown usage/cost and
recovery without another evaluator call. Both adapter and mounted-category checks
failed before the correction. All 1,375 Agent Engine unit tests, 20 architecture
checks, 210 LLM unit tests and the llmfs/integration/doc suites pass; opt-in live
tests remain ignored. Logs:
`~/Library/Caches/Alan/shadow-malformed-{adapter-red,category-red,category-tests}.log`.

These regressions are implementation evidence, not new frozen qualification.
Historical v2 results remain unchanged, the changed candidate remains unqualified
and automatic execution stays disabled. Independent reviews, strict OpenSpec
validation, the mandatory quality gate and fresh current-head CI remain required.
