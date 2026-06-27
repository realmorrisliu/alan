---
name: repo-coding
description: |
  Delegate bounded repo-scoped coding work to a fresh repo worker.

  Use this when:
  - alan has already selected the target repo or directory
  - The task needs inspect -> plan -> edit -> verify -> deliver inside that bound scope
  - The work should run in a focused child runtime instead of the home-root steward

metadata:
  short-description: Launch a repo-scoped coding worker
  tags: [coding, delegation, repo-worker, verification]
capabilities:
  required_tools: [bash]
---

# Repo Coding

This first-party package is the parent-facing entry for repo-scoped coding
work.

## Working Model

1. Treat the parent alan runtime as the coding steward.
2. Use this package when the task should move into a fresh repo worker bound to
   one repo or directory.
3. Keep launch inputs explicit: delegated task, cwd, workspace boundary, child
   time budget, and approval scope.
4. Expect bounded result integration rather than inheriting the full child
   transcript into the parent tape.
5. Do not ask the repo worker to end with prose. The child handoff is the
   package delivery contract JSON even when the user-facing parent response
   should be concise prose.

## Working Principles

1. Treat this package as alan's general repo-local coding mode, not as a
   benchmark adapter.
2. Optimize reusable coding behavior such as understanding constraints, keeping
   edits bounded, verifying honestly, and delivering clear residual risk.
3. Use explicit continuity handles from the steward when they materially help,
   including `plan`, `conversation_snapshot`, and optional `memory`.
4. Treat repeated validation and error-handling patterns as repo-wide
   invariants: inspect adjacent entrypoints and keep semantics consistent.
5. Treat truncated search output as incomplete discovery; narrow the search or
   read candidate files before deciding implementation scope.
6. Do not tune repo-worker behavior to a specific repository, benchmark corpus,
   or issue family.

## Repo-Worker Expectations

The repo worker should:

1. inspect local code and restate constraints,
2. identify nearby behavior guards such as tests, invariants, and public
   interfaces before editing,
3. decompose the change into short verifiable steps,
4. apply minimal edits that preserve unrelated behavior,
5. keep shared validation, invariant, and error semantics consistent across
   neighboring code paths,
6. distinguish public/user-facing validation errors from internal assertions,
7. prefer focused regression coverage over weakening existing tests,
8. run targeted verification,
9. describe verification outcomes honestly, including `mixed`,
   `environment_blocked`, or `not_run` states when they occur,
10. return a concise delivery summary with residual risk.

## Package Resources

- Read `references/package.md` for the package map and local validation entrypoints.
- Read `references/delivery_contract.md` for the bounded repo-worker output shape.
- Read `references/evaluator_boundary.md` before recommending evaluator support.
- Use the package-local child launch target `repo-worker`.
- Preserve the repo-worker delivery contract in delegated task text; do not
  override it with "return a concise summary" prose instructions.
- Keep repo-local edits bounded; return control when the task expands beyond
  delegated scope.
