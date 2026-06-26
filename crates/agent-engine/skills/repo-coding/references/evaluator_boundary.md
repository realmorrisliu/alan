# Conditional Evaluator Boundary

The repo worker should not default to always-on evaluator or critic loops.

Planner remains load-bearing by default. Evaluator support is conditional.

## Recommend evaluator when

1. targeted verification fails repeatedly,
2. deterministic checks are missing or too weak,
3. the task is UI or browser heavy,
4. the refactor is large or unusually risky.

## Do not require evaluator when

1. the task is a small repo-local change,
2. deterministic targeted verification exists,
3. the first verification pass succeeds,
4. the residual risk is already bounded and explained clearly.

## Reporting rule

Even when evaluator support is not used, the repo worker should state that
decision in the delivery contract through `evaluator.mode` and
`evaluator.reason`.
