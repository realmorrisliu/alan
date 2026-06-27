---
name: repo-worker-edit-verify
description: Execute repo-local code edits and run deterministic verification.
metadata:
  short-description: Edit code with a tight repo-worker verify loop
  tags: ["coding", "editing", "repo-worker", "testing"]
---

# Instructions

1. Apply the minimal edits needed for the current step while preserving
   unrelated behavior.
2. Treat existing tests as constraints and do not weaken or rewrite them
   without an explicit behavior-level reason.
3. When changing validation, invariants, or error behavior, update all
   neighboring code paths that implement the same rule so callers observe
   consistent semantics.
4. Replace assertion-based public input checks with explicit runtime errors
   when the task is about user-facing validation semantics; keep assertions
   only when they are intentionally internal invariants. Do not add new
   public-input validation with `assert`.
5. If a search or command result is truncated, rerun a narrower command or use
   exact file reads before editing.
6. Run targeted checks first, then broader checks only when the task requires
   them.
7. Add focused regression coverage when a missing behavior guard is the safest
   way to lock in the fix.
8. Record command-output summaries with explicit pass, mixed, blocked, failed,
   or not-run outcomes and note unresolved risks.
9. Keep shell commands simple and workspace-safe: avoid `&&`, shell globs, and
   brace expansion; use separate tool calls when you need multiple reads or
   checks.
10. Prefer repo-local runners or interpreters when available, such as
   `.venv/bin/python`, `venv/bin/python`, `python -m pytest`, `tox`, or
   `nox`; if the environment is clearly unavailable, report
   `environment_blocked` instead of continuing blind retries.
11. Do not install dependencies, sync environments, or invoke approval-requiring
   bootstrap commands solely to get verification running. If the available
   environment cannot execute the targeted checks directly, stop and report
   `environment_blocked`.
