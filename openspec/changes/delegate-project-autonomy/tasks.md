# Tasks

## 1. Scope and authority
- [ ] 1.1 Implement explicit user-authored delegation with existing owners; test malformed scope, spoofed AgentFS approval and absence of repository-config auto-discovery.
- [ ] 1.2 Implement saved Host identity/revision revalidation and revocation; test replacement, symlink retarget, concurrent invocation, queued action and child projection boundaries.
- [ ] 1.3 Wire task-relevant execution/dependency scopes and exact publication capabilities; verify installation scripts, unexpected destinations, ordinary versus force push and no credential inference.
- [ ] 1.4 Document the actual delegation UI/CLI and verify each example on isolated native fixtures, without advertising full-trust activation before its successor ships.

## 2. Guardian and pending decisions
- [ ] 2.1 Wire dedicated reviewer Connection and trusted-user policy selection; verify no secret duplication, immutable action/profile binding, no-tools results and main fallback.
- [ ] 2.2 Implement aggregate fallback/deadline limits, one correlated pending request and revocation of stale decisions; test outage, malformed output, cancellation and concurrent model selection.
- [ ] 2.3 Permit only provably independent work while a decision is pending; test dependency ordering, no repeated prompt, no duplicated effects and unchanged cancellation.
- [ ] 2.4 Add finite task time/Tool/model budgets through Machine-owned state; verify pause, explicit extension and persistence across recovery without reset.

## 3. Qualification and integration
- [ ] 3.1 Freeze a bounded pilot and labeled shadow corpus; measure and freeze false-denial/latency/cost thresholds before formal evaluation, requiring zero observed high-impact unauthorized allows.
- [ ] 3.2 Run the formal reviewer comparison and actual macOS project delegation flow; do not activate an unqualified candidate or substitute input-routing metrics.
- [ ] 3.3 Validate relevant Rust/authority tests, quality, native UI, strict specs and current-head review/CI; publish actual platform and cost limits.
- [ ] 3.4 After user merge, verify canonical adoption and delivered controls before archive readiness.

## Workflow follow-up
- Sync only delivered deltas and archive after verified merge. Full-trust posture has its own change and cannot be silently enabled here.
- The [Guardian deadline slice](guardian-delivery.md) records partial task 2.2 work; it does not close dedicated reviewer, fallback or stale-decision gates.
- The [Guardian shadow configuration slice](guardian-shadow-delivery.md) records explicit no-effect capture and policy selection; it does not activate or qualify a new reviewer.
