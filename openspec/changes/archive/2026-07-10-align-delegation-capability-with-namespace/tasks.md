## 1. Requirement Vocabulary And Classification

- [x] 1.1 Define the first capability vocabulary as namespace terms (workspace read/write mounts, shell, network/GitHub tool bindings, browser, LLM connection, side effects) with in-repo docs.
- [x] 1.2 Implement task-requirement classification for delegated launches, producing a bounded requirement record.
- [x] 1.3 Add unit tests for classification of GitHub-review, local-inspection, and mixed tasks.

## 2. Spawn-Boundary Eligibility Check

- [x] 2.1 Implement the requirement-vs-namespace satisfaction check at the child spawn boundary (single seam shared by `invoke_delegated_skill` today and the `delegate` Agent Executable later).
- [x] 2.2 Decline or narrow launches whose assembled namespace does not satisfy requirements; narrowed tasks state the narrowed scope and withheld capability in the child task description.
- [x] 2.3 Add tests: mismatch decline, narrowed spawn with explicit scope, satisfied spawn passes unchanged.

## 3. Recovery And Observability

- [x] 3.1 Implement recovery selection (parent-path satisfaction, narrowing, ask-user, limitation answer) with the no-silent-substitution guarantee.
- [x] 3.2 Record declined/narrowed decisions on the parent action record or tape; include classified requirements in child-run launch metadata with the namespace summary.
- [x] 3.3 Add tests: parent-path recovery is recorded, declined launch is auditable, launched child's record carries requirements and namespace summary.

## 4. Skill Guidance

- [x] 4.1 Update delegated-skill instructions to describe requirement narrowing, recovery semantics, and where decisions are recorded.

## 5. Verification And Archive Readiness

- [x] 5.1 Run `just verify` and fix fallout.
- [x] 5.2 Run `openspec validate align-delegation-capability-with-namespace --strict`.
- [x] 5.3 Open PR, address review, merge.
- [x] 5.4 After merge, sync delta specs into `openspec/specs/` and confirm archive readiness.
