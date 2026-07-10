## 1. Goal Derivation

- [x] 1.1 Implement the mechanical preference order for fallback `Current Goal`: active plan state → latest substantive user request → latest message verbatim.
- [x] 1.2 Exclude `requests/<id>/response` control payloads from goal derivation by origin.
- [x] 1.3 Implement the acknowledgement-class fragment filter for chat messages, erring toward acceptance (imperative fragments pass).
- [x] 1.4 Retain the prior substantive goal (marked as carried forward) when the latest input is filtered; never emit an empty goal when any prior goal exists.

## 2. Tests

- [x] 2.1 Add tests: one-letter follow-up keeps prior goal; approval response keeps prior goal; new substantive request replaces goal; terse imperative passes the filter.
- [x] 2.2 Add tests: plan-state preference, no-substantive-context fallback to latest message, no extra model request during refresh.

## 3. Verification And Archive Readiness

- [x] 3.1 Run `just verify` and fix fallout.
- [x] 3.2 Run `openspec validate improve-memory-goal-salience --strict`.
- [x] 3.3 Open PR, address review, merge. (PR #620, merged 2026-07-11)
- [x] 3.4 After merge, sync the delta spec into `openspec/specs/runtime-memory-surfaces/` and confirm archive readiness. (synced 2026-07-11)
