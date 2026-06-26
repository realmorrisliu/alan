# Repo-Coding Evals

Package-local repo-coding eval fixtures and review assets live here.

## Operating Principles

Treat this directory as a local evaluation surface for the measured coding
product. It is not the place where benchmark adapters or official external
scoring workflows live.

Rules:

1. `repo-coding` local evals check skill selection, bounded repo-local routing,
   delivery-contract honesty, and evaluator-boundary behavior.
2. Full steward mode still means the root alan runtime owns routing,
   continuity, and delivery framing. The repo-worker child owns only bounded
   repo-local execution.
3. Findings motivated by SWE-bench or similar ladders must still be
   generalized back into reusable contracts, prompts, tools, or harness checks.
4. Existing tests, nearby invariants, and public interfaces remain behavior
   constraints. Any benchmark-inspired patch that weakens them should be
   treated as suspect.

## Current Surfaces

1. `evals.json` as the manifest-first entrypoint for `alan skills eval`.
2. `files/benchmark_cases.json` as deterministic steward-vs-legacy routing
   fixtures.
3. `../scripts/run_benchmark_fixture.sh` and `../scripts/grade_benchmark_case.sh`
   as package-local benchmark helpers.
4. `../scripts/check_delivery_contract_examples.sh` plus
   `files/delivery_contract_*.json` as executable verification-honesty and
   behavior-preserving delivery fixtures.

Run the local scaffold with:

```bash
cargo run -p alan -- skills eval crates/agent-engine/skills/repo-coding --output-dir target/skills-eval/repo-coding/latest
```

Cross-package harness scenarios remain under:

- `docs/harness/scenarios/repo_worker/`
- `docs/harness/scenarios/coding_steward/`

## External Benchmark Adapters

SWE-bench and other external benchmark adapters are owned by the separate
first-party `swebench` package:

- `crates/agent-engine/skills/swebench/references/package.md`
- `crates/agent-engine/skills/swebench/evals/README.md`

That split keeps the boundary clean:

1. `repo-coding` is the measured coding product.
2. `swebench` is the operator-facing benchmark package.
3. Full-steward benchmark runs still exercise the same measured path:
   steward session -> `$repo-coding` -> repo-worker child.
