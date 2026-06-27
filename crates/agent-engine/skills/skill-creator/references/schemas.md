# Structured Eval Manifest

alan's structured eval entrypoint looks for `evals/evals.json`.

Current case kinds:

- `trigger`: deterministic direct skill-reference / force-select checks against
  the skill metadata
- `command`: explicit command-driven candidate runs with optional baseline,
  grading, analyzer, and comparator stages

Command stages receive environment variables such as:

- `ALAN_SKILL_EVAL_PACKAGE_ROOT`
- `ALAN_SKILL_EVAL_MANIFEST`
- `ALAN_SKILL_EVAL_OUTPUT_DIR`
- `ALAN_SKILL_EVAL_CASE_ID`
- `ALAN_SKILL_EVAL_CASE_DIR`
- `ALAN_SKILL_EVAL_PROMPT`
- `ALAN_SKILL_EVAL_CANDIDATE_ARTIFACT`
- `ALAN_SKILL_EVAL_BASELINE_ARTIFACT`
- `ALAN_SKILL_EVAL_STAGE_PROMPT_FILE`
- `ALAN_SKILL_EVAL_COMPARISON_MODE`

Use explicit commands for benchmark, grading, and review flows rather than
implicitly loading those assets into the runtime prompt.
