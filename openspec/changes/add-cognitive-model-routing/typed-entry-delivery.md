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
