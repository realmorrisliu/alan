# Repo-Worker Delivery Contract

Repo-worker runs should end with a bounded delivery artifact that the parent
steward can inspect without replaying the full child transcript.

## Required fields

The contract is expressed as JSON with these required fields:

1. `status`: one of `completed`, `blocked`, or `failed`
2. `summary`: concise explanation of what happened
3. `changed_files`: array of repo-relative file paths
4. `behavioral_guards`: non-empty array describing nearby tests, public
   interfaces, or invariants treated as constraints
5. `verification`: structured verification summary
6. `residual_risks`: array of remaining risks or blockers
7. `evaluator`: conditional evaluator decision and reason

## Conditional fields

1. `test_change_reason`: required when `changed_files` includes one or more
   test files, because test-only or test-shaping changes need an explicit
   behavior-level reason

## Verification entry shape

`verification.entries` should contain zero or more verification entries. Each
entry should include:

1. `command`
2. `scope`: `targeted` or `broader`
3. `status`: one of `passed`, `failed`, `blocked`, `environment_blocked`, or
   `not_run`
4. `exit_code`: integer exit code for attempted commands, or `null` when the
   command was blocked or not run
5. `summary`

## Verification summary shape

`verification` should also include:

1. `overall_status`: one of `passed`, `failed`, `blocked`,
   `environment_blocked`, `mixed`, or `not_attempted`
2. `verification_attempted`: boolean
3. `attempted_count`
4. `passed_count`
5. `failed_count`
6. `environment_blocked_count`
7. `blocked_count`
8. `not_run_count`
9. `all_passed`

Reporting rules:

1. `passed` means every attempted verification command succeeded.
2. `blocked` means every attempted verification command was blocked by policy
   or governance.
3. `environment_blocked` means every attempted verification command failed for
   host-environment reasons such as missing dependencies or executables.
4. `mixed` means multiple attempted outcomes occurred, or some cited commands
   were intentionally not run.
5. `not_attempted` means no verification command actually ran.

## Evaluator field

`evaluator` should record:

1. `mode`: `not_needed`, `recommended`, or `used`
2. `reason`: concise explanation tied to the task state

This keeps the repo-worker output explicit about whether the run stayed inside
the stable solo loop or crossed into a case where evaluator support should have
been considered, while making verification honesty and behavior-preserving
changes machine-checkable.
