# SWE-bench Evals

Package-local SWE-bench benchmark fixtures and operator-run workflow assets
live here.

## Operating Principles

Treat this directory as a benchmark adapter layer, not as the place where
alan's coding behavior is defined.

Rules:

1. External benchmarks measure how well alan's general coding behavior
   transfers to outside corpora; they do not define repo-worker prompts or
   justify dataset-specific shortcuts.
2. Full steward mode means the root alan runtime remains the owner of routing,
   continuity, and delivery framing. Repo-local work still runs through
   delegated `$repo-coding` child execution.
3. Findings from SWE-bench or similar ladders should be generalized back into
   reusable contracts, prompts, tools, or harness checks rather than encoded
   as benchmark-only heuristics.
4. alan-native `passed` fields in local run artifacts are orchestration and
   delivery assertions. Official resolved/unresolved status still comes from
   the external harness when that harness is run.
5. Existing tests, nearby invariants, and public interfaces remain behavior
   constraints. Benchmark patches that require weakening them should be treated
   as suspect and reviewed explicitly.

## Current Surfaces

1. `files/swebench_lite_case.template.json` as the single-case bring-up
   template.
2. `files/swebench_lite_subset.template.json` plus the curated
   `files/swebench_lite_*ids.txt` lists for subset runs.
3. `../bin/swebench-lite-prepare-workspaces` and
   `../bin/swebench-lite-materialize-subset` as package-local deterministic
   prep/materialization entrypoints.
4. `../scripts/run_swebench_full_steward_case.sh`,
   `../scripts/run_swebench_full_steward_subset.sh`,
   `../scripts/score_swebench_predictions.sh`,
   `../scripts/check_swebench_harness_env.sh`, and
   `../scripts/setup_swebench_harness_env.sh` as operator-facing shell flows.
5. `../tooling/` as the colocated workspace crate that builds the deterministic
   helper binaries behind the package-local `bin/` surface.

## Lite-First Full-Steward Bring-Up

`run_swebench_full_steward_case.sh` is the M1 single-case external benchmark
entrypoint for this package.

It does not bypass alan's orchestration layer. Instead it:

1. starts a real root/steward session,
2. submits one benchmark problem to that steward,
3. requires repo-local coding work to run through child `$repo-coding`
   delegation,
4. reads rollout/session artifacts after completion,
5. exports:
   - `model.patch`
   - `prediction.json`
   - `predictions.jsonl`
   - `run.json`
   - `verification_entries.json`
   - `verification_summary.json`
   - `assertion_report.json`
   - `kpi.json`

The case-level `run.json` also records alan-native orchestration metadata such
as:

1. `spawn_count`
2. `escalation_count`
3. `child_runs`
4. `duration_secs`
5. `verification_summary`

Case-level `run.json.passed` is intentionally an alan-native orchestration
result. It means the steward completed, delegated repo-local work through a
child launch, and produced a non-empty patch. It is not the official
SWE-bench resolved/unresolved outcome.

Use the template case file as a starting point:

- `evals/files/swebench_lite_case.template.json`

The case file should point at:

1. one prepared `SWE-bench Lite` workspace checkout,
2. one problem-statement text file for that instance.

Run one full-steward case locally with:

```bash
bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_case.sh \
  crates/agent-engine/skills/swebench/evals/files/swebench_lite_case.template.json
```

This is intentionally operator-run and non-CI-blocking.

Recommended rollout order:

1. one Lite case,
2. a 3-case Lite smoke subset,
3. a curated Lite pilot subset,
4. full Lite,
5. curated Pro subsets.

For the M2 curated-subset step, use:

- `evals/files/swebench_lite_subset.template.json`
- `evals/files/swebench_lite_pilot_v1.ids.txt`

For the smoke-first gate before the larger pilot subset, use:

- `evals/files/swebench_lite_smoke_v1.ids.txt`

Before materializing the alan suite, prepare one clean git workspace per
instance id at the benchmark `base_commit`:

```bash
crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces \
  --instance-ids-file crates/agent-engine/skills/swebench/evals/files/swebench_lite_pilot_v1.ids.txt \
  --dataset-name princeton-nlp/SWE-bench_Lite \
  --workspace-root target/benchmarks/swebench_lite/workspaces/pilot_v1
```

That preparer:

1. reads official Lite rows,
2. mirrors each upstream repo once under `.repo-cache/`,
3. materializes `<workspace-root>/<instance_id>` as a clean detached checkout at
   `base_commit`,
4. writes `workspace_map.json` and `preparation_report.json`.

Reruns are idempotent at the owned-output level: if a workspace directory is a
stale or partial previous attempt, the preparer recreates it automatically. If
you pass `--reuse-existing-workspaces`, already-clean matching workspaces are
kept instead.

The workspace preparer intentionally stops at clean git checkout
materialization. It does not install repo dependencies or reproduce the
official SWE-bench Docker images. alan's final resolved/unresolved scoring
still comes from the official harness wrapper, while any richer host-native
verification remains an operator-owned environment concern.

To materialize a real pilot subset from official Lite rows into alan case/suite
manifests, use:

```bash
crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset \
  --instance-ids-file crates/agent-engine/skills/swebench/evals/files/swebench_lite_pilot_v1.ids.txt \
  --dataset-name princeton-nlp/SWE-bench_Lite \
  --workspace-root target/benchmarks/swebench_lite/workspaces/pilot_v1 \
  --output-dir target/benchmarks/swebench_lite/manifests/pilot_v1
```

If you do not want to install the optional `datasets` package, the same
entrypoint also accepts one or more local dataset exports via repeated
`--dataset-file` arguments. It supports:

1. JSONL rows with `instance_id` and `problem_statement`
2. JSON arrays of row objects
3. Hugging Face datasets-server JSON responses with `rows[].row`

The materializer writes:

1. `cases/<instance_id>.json`
2. `problem_statements/<instance_id>.txt`
3. `suite.json`
4. `materialization_report.json`

For an official rows fallback without extra Python packages:

```bash
curl -L -o /tmp/swebench-lite.rows-0.json \
  'https://datasets-server.huggingface.co/rows?dataset=princeton-nlp/SWE-bench_Lite&config=default&split=test&offset=0&length=100'
curl -L -o /tmp/swebench-lite.rows-100.json \
  'https://datasets-server.huggingface.co/rows?dataset=princeton-nlp/SWE-bench_Lite&config=default&split=test&offset=100&length=100'
curl -L -o /tmp/swebench-lite.rows-200.json \
  'https://datasets-server.huggingface.co/rows?dataset=princeton-nlp/SWE-bench_Lite&config=default&split=test&offset=200&length=100'

crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset \
  --instance-ids-file crates/agent-engine/skills/swebench/evals/files/swebench_lite_pilot_v1.ids.txt \
  --dataset-file /tmp/swebench-lite.rows-0.json \
  --dataset-file /tmp/swebench-lite.rows-100.json \
  --dataset-file /tmp/swebench-lite.rows-200.json \
  --workspace-root target/benchmarks/swebench_lite/workspaces/pilot_v1 \
  --output-dir target/benchmarks/swebench_lite/manifests/pilot_v1
```

The end-to-end pilot order is now:

1. `crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces`
2. `crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset`
3. `run_swebench_full_steward_subset.sh`
4. `score_swebench_predictions.sh`

For the smoke-first variant, replace `swebench_lite_pilot_v1.ids.txt` with
`swebench_lite_smoke_v1.ids.txt` and keep the rest of the flow unchanged.

```bash
bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh \
  crates/agent-engine/skills/swebench/evals/files/swebench_lite_subset.template.json
```

Or run the generated pilot suite directly:

```bash
bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh \
  target/benchmarks/swebench_lite/manifests/pilot_v1/suite.json
```

That suite runner aggregates per-case artifacts into one suite directory and
generates:

1. `predictions.jsonl`
2. `case_results.jsonl`
3. `run.json`
4. `benchmark.json`
5. `kpi.json`
6. `score_with_official_harness.sh`
7. `official_harness_run.json` when official scoring is invoked
8. `official_harness_submitted_report.json` when official scoring is invoked

Suite-level `run.json` and `benchmark.json` also summarize
`total_escalation_count` across all executed cases. When the suite is run with
`--score-official`, they embed the collected official harness manifest. Their
`passed` / `failed` case counts are orchestration counts for the case runner;
the official SWE-bench result remains the harness manifest.

When `score_swebench_predictions.sh` is run later against an existing suite
directory, it also syncs the official result back into the suite-owned
artifacts when they exist:

1. `cases/<instance_id>/official_harness_instance_result.json`
2. `case_results.jsonl`
3. `run.json`
4. `benchmark.json`
5. `kpi.json`

Use `official_harness_run.json` for the full raw wrapper manifest and
`official_harness_submitted_report.json` for the compact subset-only summary.

Official harness scoring still happens outside alan's runtime loop, but the
package provides a thin wrapper so operators do not need to remember the raw
Python module entrypoint:

```bash
export ALAN_SWEBENCH_HARNESS_PYTHON_BIN=/absolute/path/to/harness/python

bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh \
  target/benchmarks/swebench_lite/suites/swebench_lite_curated/predictions.jsonl \
  --work-dir target/benchmarks/swebench_lite/suites/swebench_lite_curated \
  --manifest-file target/benchmarks/swebench_lite/suites/swebench_lite_curated/official_harness_run.json
```

To check or set up that dedicated harness environment:

```bash
bash crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh

bash crates/agent-engine/skills/swebench/scripts/setup_swebench_harness_env.sh
export ALAN_SWEBENCH_HARNESS_PYTHON_BIN=/absolute/path/to/repo/target/benchmarks/swebench_harness/.venv/bin/python
```

The setup script installs the official harness into a dedicated virtualenv so
the harness Python does not have to match the Python that alan child runtimes
use inside benchmark workspaces. It also installs `socksio` so the harness can
run on hosts that expose Hugging Face access through a SOCKS proxy.

To run the curated suite and trigger the official harness in one step:

```bash
bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh \
  target/benchmarks/swebench_lite/manifests/pilot_v1/suite.json \
  --score-official
```
