# Tasks

## 1. Scope and authority
- [x] 1.1 Implement explicit user-authored delegation with existing owners; test malformed scope, spoofed AgentFS approval and absence of repository-config auto-discovery.
- [x] 1.2 Implement saved Host identity/revision revalidation and revocation; test replacement, symlink retarget, concurrent invocation, queued action and child projection boundaries.
- [x] 1.3 Wire task-relevant execution/dependency scopes and exact publication capabilities; verify installation scripts, unexpected destinations, ordinary versus force push and no credential inference.
- [ ] 1.4 Document the actual delegation UI/CLI and verify each example on isolated native fixtures, without advertising full-trust activation before its successor ships.

## 2. Guardian and pending decisions
- [x] 2.1 Wire dedicated reviewer Connection and trusted-user policy selection; verify no secret duplication, immutable action/profile binding, no-tools results and main fallback.
- [x] 2.2 Implement aggregate fallback/deadline limits, one correlated pending request and revocation of stale decisions; test outage, malformed output, cancellation and concurrent model selection.
- [ ] 2.3 Permit only provably independent work while a decision is pending; test dependency ordering, no repeated prompt, no duplicated effects and unchanged cancellation.
- [x] 2.4 Add finite task time/Tool/model budgets through Machine-owned state; verify pause, explicit extension and persistence across recovery without reset.

- [x] 2.5 Apply the OS-boundary rule: skip Guardian for effects contained by the active confinement and review only uncontained effects; verify no repeated prompts for contained work and no grant from reviewer output.

## 3. Qualification and integration

> **Pruned under ADR-0061 (2026-11-20):** Retired the pre-ADR-0061 Guardian
> shadow-corpus freeze, classifier false-denial/latency/cost threshold gate, and
> formal shadow reviewer comparison (former `3.1` and shadow-comparison clause in
> `3.2`). Under ADR-0061, the OS sandbox and explicit mount grants are the
> security boundary: contained effects bypass Guardian, uncontainable/out-of-grant
> effects remain governed by deterministic red lines and fail-closed human
> fallback, Guardian never grants authority, and M3 shipped slim project
> delegation directly without a shadow classifier corpus. Pilot budget calibration
> and daily-use measurement belong to `qualify-personal-daily-development`.

- [ ] 3.2 Run the actual macOS project delegation flow and publish observed platform and cost limits; do not substitute input-routing metrics.
- [ ] 3.3 Validate relevant Rust/authority tests, quality, native UI, strict specs and current-head review/CI.
- [ ] 3.4 After user merge, verify canonical adoption and delivered controls before archive readiness.

## Workflow follow-up
- Sync only delivered deltas and archive after verified merge. Full-trust posture has its own change and cannot be silently enabled here.
