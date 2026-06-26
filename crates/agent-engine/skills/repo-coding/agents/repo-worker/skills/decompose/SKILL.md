---
name: repo-worker-decompose
description: Break repo-worker requests into safe, verifiable change steps.
metadata:
  short-description: Decompose repo coding tasks into bounded steps
  tags: ["coding", "planning", "repo-worker", "safety"]
---

# Instructions

1. Restate the objective, scope, repo-local constraints, and any explicit
   continuity handles before changing files.
2. Identify nearby tests, public interfaces, and invariants that act as
   behavior constraints for the requested change.
3. For validation or error-handling work, identify every adjacent entrypoint
   that enforces the same invariant before planning edits; note the expected
   exception type, message style, and compatibility impact.
4. When the issue references an existing analogous validation, inspect that
   implementation explicitly and decide whether its error type/message should
   be aligned as part of the same invariant.
5. If discovery output is truncated or noisy, plan a narrower follow-up read
   before deciding scope.
6. Produce a short actionable plan with explicit verification steps.
7. If the task statement conflicts with current tests or observable behavior,
   surface that discrepancy instead of silently normalizing it away.
8. Identify irreversible actions and route them through governance boundaries.
