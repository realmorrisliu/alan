# Ordered continuation — 2026-10-08

User-authorized goal, starting from merged main `1c53cf52`: close planning debt,
qualify reliability boundaries, deliver one read-only mixed-Machine task, then
requalify automatic input routing. Codex implements directly; historical Alan
self-development instances remain stopped. Existing change owners remain the
source of truth. Checklist counts measure evidence closure, not implementation
percentage. No parked change is activated by this plan.

## 1. Planning closure

- [x] Verify #1025 merge and final-head checks; synchronize its delivered package,
  brand and quality-gate deltas, including the quality-gate Purpose.
- [x] Verify #1034 delivery and #1036 canonical synchronization; compare complete
  UI delta requirements with canonical specs before archiving.
- [x] Archive both completed changes locally, preserving their evidence and
  metadata; repair relative references and reconcile stale current status.
- [x] Validate current surfaces, strict OpenSpec and changed links; review the
  exact closure patch, obtain passing CI and record merge separately.
  PR #1037 reviewed head `d87a3d8a132f6ab1209f839a0c5294bd2f2e641d` passed all
  16 checks with no automated review findings; the user merged it as
  `411828d5f3ad1a6a5e60f93bee6a70ac8deadbcb`. Post-merge CI and CodeQL also passed.

## 2. Reliability qualification

Owner: `unify-agent-command-input`, especially tasks 2.1, 2.4–2.7, 2.11–2.14.
Local boundary evidence and the reproduced revocation fix are indexed in
[reliability acceptance](reliability-acceptance.md). Current-head review, CI and
merge remain required before this stage is delivered.

Inventory existing public-boundary tests first. Keep code changes limited to
reproduced failures at their shared owner; add only missing boundary checks.

- [x] Same-Agent clients: concurrent identical text with distinct IDs; independent
  result consumption, correlated failure/cancellation and ordered cwd changes.
  Separate foreground invocations are not evidence for this matrix.
- [x] Cross-grant operations: explicit and generated Tool paths, noncurrent and
  read-only mounts, symlink escape, revocation before dispatch/during work,
  descendant cancellation, pending buffer saves and stale-edit/save failures.
  Assert effects and authoritative outcomes, not just error strings.
- [x] Output retention: stdout/stderr beyond the projection budget, exact retained
  output references and a subsequent Agent question in a correlated native trace;
  missing/expired evidence uses the existing owning
  retention traces rather than adding a Host management API. Ordinary output
  content must not create authority.
- [x] Linux: run the shipped namespace/mount/network adapter on a capable Linux
  host and verify confinement, read-only and virtual-only mounts, revocation and
  explicit degradation/fail-closed behavior. A skip or macOS mock is not a pass.
- [ ] Run owning suites, quality and required CI; review exact changes and close
  only fully covered parent clauses. Record remaining platform gaps explicitly.

## 3. One mixed-Machine task

Owner: `add-cognitive-model-routing`, tasks 1.1/1.4 and 2.2–3.3. The user activates
this bounded scope after the reliability stage; broader roadmap stays gated.
Use a small read-only project question: identify which existing crate owns a
specified behavior and return a structured answer with namespace file citations.

- [ ] Freeze the task, available read-only capabilities, result schema and
  deterministic/generation baselines before implementation. Define the typed
  evaluation point, no-match/unavailable semantics and finite attempt/time/cost
  fallback budgets in the owning change; reuse Connection and Machine evidence.
- [ ] Exercise deterministic reads, typed selection and optional generation in
  the same Agent Machine. Structured-only completion must be distinct from wait,
  failure, cancellation and Process exit; missing authority stays unavailable.
- [ ] Verify explicit wait/resume and crash recovery retain decision/effect
  identity; Unknown is reconciled without replay. Cancel before fallback must
  prevent further calls. No global router or renderer launch path is introduced.
- [ ] Complete focused fixture and real-entry acceptance, exact review, CI and
  delivered-only canonical sync. This task does not qualify input auto-routing.

## 4. Input-routing requalification

Owner: `qualify-agent-input-routing`. Frozen v2 remains failed historical evidence
(200/264 correct labels; 34 false command classifications). Later reviewed source
is unqualified. Keep automatic routing disabled throughout this stage.

- [ ] Diagnose wrong command/ambiguous selections; improve the existing rubric or
  deterministic admission boundary with explicit prefixes and exact input text
  preserved. Separate development cases from the frozen qualification corpus;
  do not relabel failures or lower thresholds merely to pass.
- [ ] Freeze a new source/binary-bound candidate, controls, rubric, cases and the
  existing numeric gates before collecting fresh measurements without retries
  that erase failures. Verify classification itself causes zero effects.
- [ ] Compare typed, deterministic and generation baselines across the full
  interactive/redirected matrix, including pending responses, malformed results,
  outages, ambiguity and overrides. Measure native admission added latency and
  verified cost consistently; unknown or unsupported slots remain incomplete.
- [ ] Report pass/fail against accuracy >=95%, per-class recall >=90%, valid
  results >=98%, zero false commands, p50 <=300 ms, p95 <=1000 ms and verified
  mean cost <=1000 micro-USD, plus the existing relative generation-baseline gates.
- [ ] Deliver reviewed code/evidence and record a qualification decision. Failure
  or incomplete measurement leaves auto-routing off. A pass permits requesting
  explicit activation approval; it does not authorize activation by this goal.

## Delivery discipline

Each stage records exact source, owning checks, review, CI and merge identities.
Prepare concrete reviewable PRs before requesting merge authorization; existing
no-auto-merge policy remains. Do not mark this goal complete merely because a
qualification run finished while reliability or mixed-task obligations remain.
