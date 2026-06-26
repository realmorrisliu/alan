---
name: swebench
description: |
  Run and inspect SWE-bench benchmark workflows for alan's coding line.

  Use this when:
  - The user wants to run SWE-bench Lite or curated SWE-bench subsets
  - The user wants to prepare benchmark workspaces or materialize suite manifests
  - The user wants to score benchmark predictions with the official harness
  - The user wants benchmark operations to remain separate from the repo-coding product contract

metadata:
  short-description: Operate SWE-bench benchmark workflows
  tags: [benchmark, swebench, evaluation, coding]
capabilities:
  required_tools: [bash]
compatibility:
  requirements: Use the package-local `bin/` and `scripts/` entrypoints for SWE-bench preparation, execution, and scoring flows.
---

# SWE-bench

This first-party package owns SWE-bench benchmark orchestration for alan's
coding line.

## Working Model

1. Treat SWE-bench as a measurement layer, not as the definition of coding
   behavior.
2. Keep benchmark orchestration separate from `$repo-coding`.
3. Use this package to prepare benchmark inputs, materialize suite manifests,
   run full-steward cases, and score predictions.
4. Let benchmark cases exercise the real coding product path:
   steward session -> `$repo-coding` -> repo worker child.

## Package Surfaces

- `bin/`: package-local executable entrypoints for deterministic SWE-bench
  preparation/materialization helpers
- `scripts/`: shell glue for full-steward case/subset runs and official harness
  scoring
- `references/`: package map and operator guidance
- `evals/files/`: benchmark templates and curated instance-id lists

## Rules

1. Do not encode repository-specific or corpus-specific behavior into alan's
   coding product.
2. Benchmark findings should be generalized back into reusable coding,
   governance, prompt, or harness improvements.
3. The benchmark operator flow should remain auditable and reproducible.
4. `repo-coding` stays the measured coding skill; this package owns only the
   benchmark orchestration layer.
