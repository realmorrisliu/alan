## REMOVED Requirements

### Requirement: Coding handle profiles are explicit
**Reason**: The existing handle profile carries parent Session identity.
**Migration**: Use parent Agent Process path, bounded repo descriptors, worker executable, policy, and rollout/checkpoint evidence.

### Requirement: Coding execution is recoverable and fail-safe
**Reason**: Recovery is coupled to a Session-recovery path.
**Migration**: Recover from Process state, worker/steward files, checkpoints, rollouts, and durable repo evidence.

## ADDED Requirements

### Requirement: Coding handles identify parent and worker Processes
Coding steward handles SHALL identify the parent Agent Process, worker Agent Process or executable,
bounded repository descriptors, namespace/policy inputs, and rollout/checkpoint evidence. They
SHALL use those concrete owners as their complete identity and authority boundary.

#### Scenario: A steward delegates repository work
- **WHEN** the steward spawns a bounded repo worker
- **THEN** the handle links the worker to the parent Agent Process and delegated repo resources
- **AND** Process, namespace, policy, and evidence files are sufficient to inspect and govern it

### Requirement: Coding recovery is file and Process based
Coding execution SHALL recover from authoritative Process state, steward/worker files, durable
checkpoints, rollouts, and repository evidence. It SHALL fail closed when those owners cannot prove
continuity.

#### Scenario: A worker disappears during coding work
- **WHEN** the worker Process exits or becomes unavailable before handoff
- **THEN** the steward reconstructs status from Process and durable evidence
- **AND** it resumes work only when those owners prove a safe continuation point

## MODIFIED Requirements

### Requirement: Coding eval ladder separates product invariants from benchmark adapters
Alan SHALL validate coding steward orchestration, repo-worker execution, package-local evals, and
external benchmark adapters as separate layers.

Validation ladder:

1. **Coding steward harness** validates parent-side orchestration behavior: delegated launch
   contracts, workspace-root versus nested-cwd binding, default non-inheritance, explicit handle
   handoff, bounded result integration, and fail-safe behavior when delegated execution or artifact
   routing is unavailable.
2. **Repo-worker harness** validates bounded child behavior: minimum
   inspect -> plan -> edit -> verify -> deliver loop, control-mode stability, restart recovery,
   irreversible-effect dedupe continuity, and governance boundary coverage.
3. **Package-local benchmark scaffold** lives under
   `crates/agent-engine/skills/repo-coding/evals/evals.json` and covers activation selection,
   bounded single-repo routing, multi-repo steward-owned cases, and owner-boundary escalation cases.
4. **External benchmark adapters** prepare benchmark workspaces and manifests, validate or set up
   the official harness, and score predictions produced by an independently selected Agent Process
   execution workflow. They do not define or host Agent Process execution.

Recommended external benchmark bring-up order is Lite-first:

1. prepare and materialize one SWE-bench Lite case
2. produce `predictions.jsonl` through an independently selected Agent Process execution workflow
3. score the single case with the official harness
4. expand to curated Lite subsets and full Lite runs
5. expand to curated SWE-bench Pro only after the Lite measurement path is stable

#### Scenario: Benchmark fixture suggests a prompt shortcut
- **WHEN** an external benchmark reveals a failure
- **THEN** the fix is generalized into reusable contract, prompt, policy, or harness behavior
  rather than encoded as a benchmark-only heuristic

#### Scenario: Benchmark execution is requested before a file-native launcher exists
- **WHEN** an operator needs predictions and Alan has no accepted file-native benchmark Process
  launcher
- **THEN** the benchmark package exposes preparation, manifest, harness, and scoring surfaces only
- **AND** it does not recreate a removed host-control runner or compatibility wrapper

### Requirement: Coding eval surfaces and KPI fields are stable
Alan SHALL keep executable coding eval entrypoints and KPI output fields stable enough for steward
and worker regression tracking.

Minimum executable surfaces:

1. `bash scripts/harness/run_coding_steward_suite.sh`
2. `bash scripts/harness/run_repo_worker_suite.sh`
3. `cargo run -p alan -- skills eval crates/agent-engine/skills/repo-coding`

Current external benchmark operator surfaces:

1. `crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces`
2. `crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset`
3. `bash crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh`
4. `bash crates/agent-engine/skills/swebench/scripts/setup_swebench_harness_env.sh`
5. `bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh <predictions-jsonl>`

The benchmark package SHALL NOT expose a package-local Agent Process execution runner until a
separate accepted change defines a file-native launcher. Prediction production remains an explicit
external input to the retained scoring surface.

Shared KPI fields:

1. `suite`
2. `mode`
3. `total`
4. `passed`
5. `failed`
6. `skipped`
7. `pass_rate_percent`
8. `duration_secs`
9. `executed_scenarios`
10. `kpi_tag_counts`

Suite-specific fields may extend this, such as `profile` for autonomy.

#### Scenario: Harness KPI is emitted
- **WHEN** a steward or repo-worker harness writes KPI output
- **THEN** it includes the shared fields needed for later aggregation
- **AND** it may add suite-specific fields without redefining the shared contract

#### Scenario: Operator scores independently produced predictions
- **WHEN** an operator has a `predictions.jsonl` file from an independently selected execution
  workflow
- **THEN** the retained scoring entrypoint evaluates it with the official harness
- **AND** no deleted package-local execution runner is required
