# Self-Eval Suite

> Status: current runner guide. Normative harness behavior lives in OpenSpec:
> [`runtime-harness-contract`](../../../openspec/specs/runtime-harness-contract/spec.md).

Self-eval compares `baseline` vs `candidate` profile behavior and emits a promotion report.

## Run Modes

1. `local`
   - Command:
     - `bash scripts/harness/run_self_eval_suite.sh --mode local`
   - Uses deterministic blocking autonomy scenarios.
   - Always emits report; does not fail on gate mismatch.
2. `ci`
   - Command:
     - `bash scripts/harness/run_self_eval_suite.sh --mode ci`
   - Uses deterministic blocking autonomy scenarios.
   - Baseline and candidate both run against current `HEAD` by default.
   - Fails with non-zero exit code when promotion gate checks fail.
3. `nightly`
   - Command:
     - `bash scripts/harness/run_self_eval_suite.sh --mode nightly`
   - Uses full autonomy scenario set (`run_autonomy_suite.sh` without `--ci-blocking`).
   - Baseline and candidate both run against current `HEAD` by default.
   - Intended for broader trend monitoring.

## Execution Isolation

To avoid cache-order bias in duration comparisons, baseline and candidate runs use isolated
`CARGO_TARGET_DIR` directories under each profile artifact directory.

## Profile Fixtures

`run_autonomy_suite.sh` treats non-default `HARNESS_PROFILE` values as strict profile
selectors and requires per-profile fixture overrides under:

- `docs/harness/scenarios/profiles/{profile}/autonomy/*.json`
- `docs/harness/scenarios/profiles/{profile}/governance/*.json`

If an override is missing, the suite fails instead of silently falling back to default fixtures.

## Artifacts

Generated under:

- `target/harness/self_eval/run.XXXXXX/` (the exact run directory is printed)

Key files:

1. `input_script.json` (scenario fixture snapshot)
2. `promotion_thresholds.env` (resolved threshold config)
3. `baseline/profile_metrics.json`
4. `candidate/profile_metrics.json`
5. `profile_regression_report.json` (comparison + gate checks)

Each profile retains `build-evidence/build.json` (source revision, command, exit
code and cleanup outcome) and `build-evidence/build.log`. Compiler output uses
`CARGO_INCREMENTAL=0` and is retired after the command exits if no consumer
remains. Interrupted runs keep diagnostics and any busy output for explicit
`just cache-clean` maintenance. Historical reports and changed worktrees are
never force-deleted by a later run.

## Threshold Configuration

Versioned config file:

- `docs/harness/self_eval/promotion_thresholds.v1.env`
