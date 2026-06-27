# Repo-Coding Package

This first-party package productizes alan's repo-scoped coding worker under
`crates/agent-engine/skills/repo-coding/`.

It is not the full coding product boundary by itself. The steward/worker model
is defined in `docs/spec/alan_coding_steward_contract.md`.

## Package Principles

This package should make alan better at general repo-local coding work. It
must not absorb benchmark orchestration or corpus-specific behavior.

Rules:

1. The parent alan runtime remains the coding steward; this package provides
   the bounded repo-worker path inside that broader stewardship model.
2. Any improvements motivated by benchmark findings should be reusable coding
   improvements that help normal repository work as well.
3. Explicit continuity handles handed down by the steward, including optional
   memory, are for project continuity and task execution quality, not for
   benchmark-only escape hatches.
4. Verification claims and delivery summaries produced through this package
   must remain evidence-backed and behavior-preserving.

## What this package contains

1. `SKILL.md` as the parent-facing entry for repo-scoped coding delegation.
2. `skill.yaml` declaring delegated execution through the package-local
   `repo-worker` child target.
3. `agents/openai.yaml` with package-level compatibility metadata for catalog
   display and activation hints.
4. `agents/repo-worker/` with the child agent root, coding micro-skills, and
   extension manifest examples.
5. `references/delivery_contract.md` and `references/evaluator_boundary.md`
   describing the bounded output contract and conditional evaluator boundary.
6. `evals/evals.json`, `evals/files/benchmark_cases.json`, and
   `evals/files/delivery_contract_*.json` for manifest-first local eval and
   delivery-contract scaffolding.
7. `scripts/` with deterministic validators and local benchmark helpers.
8. External smoke and harness entrypoints under `scripts/repo-worker/` and
   `scripts/harness/`.

## Quick Validation

1. `bash scripts/repo-worker/run_smoke.sh`
2. `bash scripts/harness/run_repo_worker_suite.sh --ci-blocking`
3. `bash scripts/harness/run_coding_steward_suite.sh --ci-blocking`
4. `cargo run -p alan -- skills eval crates/agent-engine/skills/repo-coding --output-dir target/skills-eval/repo-coding/latest`

## External Benchmarks

SWE-bench and similar operator-run external benchmark adapters no longer live
in this package. They are owned by the separate `swebench` first-party skill:

1. `crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces`
2. `crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset`
3. `crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_case.sh`
4. `crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh`
5. `crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh`

That split is intentional:

1. `repo-coding` remains the measured coding product.
2. `swebench` owns benchmark orchestration, dataset adaptation, and official
   harness scoring.
3. Full-steward benchmark runs still exercise the real product path:
   steward session -> `$repo-coding` -> repo-worker child.
