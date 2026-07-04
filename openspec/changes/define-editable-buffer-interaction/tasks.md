## 1. Contract Definition

- [x] 1.1 Capture ADR-0026 D4 as an OpenSpec proposal without changing M0-M2
  runtime scope.
  Done 2026-07-03: proposal records the editable-buffer layer as M4+ and keeps
  current agent operation on `io/` + `ctl`.
- [x] 1.2 Write the technical design for a headless file-server-first editable
  buffer surface.
  Done 2026-07-03: design defines `editfs`, one directory per buffer, explicit
  `ctl` execution, and no first-slice native UI dependency.
- [x] 1.3 Add the `editable-buffer-interaction` capability spec with testable
  requirements.
  Done 2026-07-03: spec covers `body`, `tag`, `ctl`, `addr`, `event`, range
  addressing, explicit execution, event observation, and M0-M2 independence.

## 2. Scope Guardrails

- [x] 2.1 Make clear that this change absorbs the idea, not Acme's literal UI.
  Done 2026-07-03: proposal and design explicitly exclude Acme mouse chords,
  visual taste, and native macOS UI from this contract slice.
- [x] 2.2 Make clear that execution remains capability-bounded and auditable.
  Done 2026-07-03: spec requires explicit `ctl` execution, normal
  namespace/policy checks, and event-stream records.

## 3. Verification

- [x] 3.1 Run `openspec validate define-editable-buffer-interaction --strict`.
  Done 2026-07-03: strict OpenSpec validation passed.
- [x] 3.2 Run `git diff --check`.
  Done 2026-07-03: diff whitespace check passed.

## 4. PR Hygiene

- [x] 4.1 Commit this contract slice separately from the aP wire transport.
  Done 2026-07-03: committed on
  `feat/northstar-editable-buffer-contract` as
  `docs(openspec): define editable buffer interaction layer`.
- [x] 4.2 Open a stacked PR on top of `feat/northstar-ap-wire` and mark it ready
  for review.
  Done 2026-07-03: opened #592 on top of `feat/northstar-ap-wire`; GitHub
  reports it is already ready for review.

## 5. Archive Readiness

- [ ] 5.1 After this change merges, sync `editable-buffer-interaction` into
  `openspec/specs/` without delta markers before archiving the change.
