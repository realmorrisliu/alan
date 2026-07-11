# Alan Harness

The harness runs repeatable system-level checks over Agent Execution Engine,
governance, compaction, repo-coding, and coding-steward behavior. Normative
requirements live in the OpenSpec `runtime-harness-contract` capability.

## Current suites

```bash
bash scripts/harness/run_autonomy_suite.sh
bash scripts/harness/run_autonomy_suite.sh --ci-blocking
bash scripts/harness/run_compaction_suite.sh
bash scripts/harness/run_compaction_suite.sh --ci-blocking
bash scripts/harness/run_repo_worker_suite.sh
bash scripts/harness/run_repo_worker_suite.sh --ci-blocking
bash scripts/harness/run_coding_steward_suite.sh
bash scripts/harness/run_coding_steward_suite.sh --ci-blocking
bash scripts/harness/run_self_eval_suite.sh --mode local
```

The autonomy suite now covers Process-scoped effect deduplication and recovery
governance. It does not emulate a host server, scheduled wakeup, remote client,
or restart manager.

## Scenario contract

Each fixture contains:

- a stable scenario id and category;
- an exact executable command;
- a blocking/non-blocking classification;
- human-readable assertions;
- KPI tags.

Runners write input, command output, decision traces, assertion reports, and a
suite KPI summary under `target/harness/<suite>/latest/`.

## Current invariants

- tool loops must not duplicate irreversible effects;
- recovery evidence is read from rollout/checkpoint owners;
- governance boundaries cannot be bypassed during recovery;
- compaction preserves tape state and exposes durable attempt evidence;
- repo-coding and coding-steward checks keep workspace and child Process
  boundaries explicit;
- failures remain attributable to a concrete engine, policy, tool, persistence,
  or harness layer.
