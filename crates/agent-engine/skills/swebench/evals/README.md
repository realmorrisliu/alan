# SWE-bench Evals

This directory contains benchmark inputs for Alan's SWE-bench operator package.
It does not define Agent Process execution or repository-coding behavior.

## Current surfaces

- `files/swebench_lite_case.template.json`: single-case input template.
- `files/swebench_lite_subset.template.json`: subset input template.
- `files/swebench_lite_*ids.txt`: curated Lite instance lists.
- `../bin/swebench-lite-prepare-workspaces`: materializes clean benchmark
  workspaces at the official base commits.
- `../bin/swebench-lite-materialize-subset`: creates case and suite manifests.
- `../scripts/check_swebench_harness_env.sh`: validates the official harness
  prerequisites.
- `../scripts/setup_swebench_harness_env.sh`: prepares a dedicated harness
  environment.
- `../scripts/score_swebench_predictions.sh`: scores an existing predictions
  file with the official harness.

Alan currently has no package-local benchmark runner. A caller must produce
`predictions.jsonl` through an independently selected execution workflow. This
package deliberately does not recreate the removed host control surface.

## Typical flow

```bash
crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces \
  --instance-ids-file crates/agent-engine/skills/swebench/evals/files/swebench_lite_pilot_v1.ids.txt \
  --dataset-name princeton-nlp/SWE-bench_Lite \
  --workspace-root target/benchmarks/swebench_lite/workspaces/pilot_v1

crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset \
  --instance-ids-file crates/agent-engine/skills/swebench/evals/files/swebench_lite_pilot_v1.ids.txt \
  --dataset-name princeton-nlp/SWE-bench_Lite \
  --workspace-root target/benchmarks/swebench_lite/workspaces/pilot_v1 \
  --output-dir target/benchmarks/swebench_lite/manifests/pilot_v1

bash crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh
bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh \
  /absolute/path/to/predictions.jsonl \
  --work-dir target/benchmarks/swebench_lite/results
```

Official resolved/unresolved results come from the external harness. Benchmark
findings should be generalized into reusable product, governance, prompt, or
verification improvements rather than dataset-specific behavior.
