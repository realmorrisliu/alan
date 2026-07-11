# SWE-bench Package

This first-party package productizes alan's SWE-bench benchmark operator layer
under `crates/agent-engine/skills/swebench/`.

It is separate from the coding product contract. The measured coding path
remains alan steward orchestration plus `$repo-coding` repo-worker delegation.

## What this package contains

1. `SKILL.md` as the parent-facing benchmark operator entry.
2. `bin/` entrypoints for deterministic workspace preparation and subset
   materialization.
3. `scripts/` for official harness setup, validation, and scoring.
4. `evals/files/` templates and curated instance-id lists for Lite-first runs.
5. `tooling/` as the colocated workspace crate that builds the SWE-bench helper
   binaries used by this package.

## Quick entrypoints

```bash
crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces ...
crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset ...
bash crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh
bash crates/agent-engine/skills/swebench/scripts/setup_swebench_harness_env.sh
bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh ...
```

## Boundary

This package is the benchmark operator layer. It should not become the place
where repo-local coding behavior is defined or specialized.
