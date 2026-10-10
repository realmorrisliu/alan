## Context

This is delivery 3 of the user's ordered goal, after terminal presentation and
Linux toolchain/authority qualification. UI implementation is merged in PR #1044;
canonical closure is prepared in #1045. Linux toolchain #1046 is still draft and
its ADR-0058 disjoint read-only dependency adoption remains outstanding. Controlled
candidate qualification is authorized by the goal: the live matrix starts after
the predecessor candidate's native qualification, code review and current-head CI
pass, with source/binary/authority scope frozen. User merge and canonical closure
remain required final-delivery gates rather than prerequisites for disposable
candidate tests. Testing does not adopt the proposed grant policy. If review
changes its production behavior, rerun affected rows on the final candidate.

Existing `scripts/harness/run_repo_worker_suite.sh` collects executable scenario
results but primarily runs scripted/CI commands. Its exit-zero score cannot prove
real-model authorship. Reuse its receipt conventions and existing CLI/Process,
AgentFS, rollout and Host Mount boundaries; keep the thirty native runs distinct
from CI scenarios and earlier UI/native Tool receipts.

## Goals / Non-Goals

**Goals:**
- Complete five task families at least three times each on macOS/Herdr and
  Linux/ordinary PTY, recording first outcomes, intervention, time and effects.
- Exercise real generation, retained diagnostics, model-selected Tools and source
  changes through a fresh Alan invocation with explicit disposable project grants.
- Fix shared UI/reliability causes exposed by these tasks and verify corrected
  retries without replacing original records.

**Non-Goals:**
- Enable automatic routing or change grant, policy, provider or recovery semantics.
- Delegate production implementation to Alan, install unused Node/Python tooling,
  construct a terminal host or claim general autonomous self-development.

## Decisions

### Freeze a complete matrix before launching generation

`acceptance-matrix.md` owns the thirty slots, family inputs/checks and measurement
fields. Each repetition uses a fresh disposable fixture and source/effect baseline.
Initial task variants and prompts are hashed before their first run; expected
answers and verifier state remain outside the granted project. Preserve model
`gpt-6.1-sol` with medium effort when the actual supported Connection profile can
supply it; any unavailable profile/model is an environment-blocked slot, never an
unannounced substitute. Record effective model/effort for every resumed segment.

Freeze the complete thirty-slot inputs, bounds and measurement fields first, then
gate live generation separately on each Host's actual model/backend readiness.
An unavailable Linux Connection does not prevent a ready macOS Host from running
its fifteen slots. Keep unavailable prerequisites explicit and task 1.4 open until
both Hosts are ready; do not replace Linux slots, reduce the denominator or infer
their completion from macOS results. Final qualification still requires three
proven completions of every family on each Host. This staging changes no success
criterion and avoids making external login a prerequisite for unrelated evidence.

Fresh invocations are separate Process/boot identities, not proof of statistically
independent model samples. Record exposed Memory Store/definition/config identity;
do not clear personal memory to manufacture independence. A recovery-family run
contains explicitly selected durable continuation and records both old and fresh
execution identities. Do not resume unrelated runs or auto-recover product-wide.

Alternative: reusing successful native Tool/UI receipts would avoid new execution,
but leaves the model/plan/edit/verify chain untested and cannot satisfy this goal.

### Keep Alan as the product under test

macOS uses Herdr-hosted ordinary Alan terminal operation; Linux uses an ordinary
PTY. Freeze 80x24 initially; actual narrow/resize defects receive targeted 48-column
reruns. Additional cross-host smoke and UI reruns do not replace any of the thirty
slots. Keep draft/cursor/detail/scrollback observations with raw terminal captures
and Process/AgentFS evidence. Native Host probes must establish the enforcing
backend independently of portable test counts.

Provision projects/expected test failures as Host fixture setup. Submit the task
through Alan Shell to its Root Agent Process and let the real model choose
permitted Tools and author requested production changes. Host scripts verify
results independently afterward; they do not write the solution. Any operator
source edit, advice or tool substitution is recorded as an intervention and does
not qualify model-only coding success. Codex implements actual product repairs.

### Minimal receipt collection, separate from runtime ownership

Reuse harness JSON/JSONL conventions, native terminal captures and existing
read-only inspection paths. Add only collection/assertion code required by the
matrix, with a small malformed/missing/duplicate-record self-check if new parsing
logic is needed. Do not build another runtime controller or repair the generic
CI runner merely to make its scripted pass count look like native qualification.

Per run, correlate prompt/submission, request/action IDs, Root Process/boot,
rollout/checkpoint references, tool results, terminal observation and verified
source/artifact/effect changes. Product runtime evidence stays in its owning
System Store; qualification receipts/captures use a unique task-owned Host cache,
with no credential bodies or copied personal stores. Repository fixtures and
operator guidance are executable evidence, not an alternative planning surface.

### Outcomes and intervention remain separate dimensions

A task passes only when its predeclared behavioral/effect checks and required
terminal/lifecycle assertions pass. A model's final answer, command exit zero,
screenshot or fixture setup success alone is insufficient. Planned grant approval,
cancel/revoke/recovery steps are counted separately from unplanned steering,
operator code edits, environment repair and corrective reruns.

Report first-attempt pass/failed/environment-blocked/unsupported/not-run totals
out of thirty, with per-platform/family counts. Report assisted completion and
zero-unplanned-intervention completion separately. Corrections add linked attempts
rather than overwriting the original slot. Wall time runs from submission through
verified completion, with model/tool/wait portions when evidence allows. Provider
usage/cost remains unknown when unavailable; never fabricate token totals or
pricing. The five-family sample supports only those measured boundaries.

### Repair observed defects in their durable owners

Capture the smallest reproduction before changing production code. Add the owning
OpenSpec delta when needed: terminal presentation/detail retention belongs to
existing TUI/Tool result owners; Process/AgentFS and Host adapters keep lifecycle
and native authority. Fix a shared cause once, run affected tests and native reruns,
and freeze a new source/binary identity. Every final qualification slot must have
applicable final-candidate evidence; retain earlier-source attempts as historical
results and rerun affected rows. No speculative rewrite follows from this plan.

### Observed directory context uses existing Action evidence

Native F1 r3 and r1 retry show `no project` after public Host authorization and a
successful ordinary `cd`: the TUI tracks the correct namespace cwd but its header
requires a local picker receipt. Reuse that observed cwd as the fallback location
when it differs from `/`; keep the existing default at `/` and picker labels/access
when a receipt exists. Do not infer authority or Native Host paths from cwd. Clear
the previous observed cwd at Root replacement while retaining grant/control fencing.
Existing narrow-line priorities and the editable draft remain unchanged.

## Risks / Trade-offs

- Provider or tool environment unavailable → retain the blocked first slot and
  inspect actual readiness; no mocked output, weaker-backend or model substitution.
- Operator intervention hides failure → log each intervention before it changes
  the run and distinguish planned lifecycle actions from corrective help.
- Reused cache or durable work yields a false pass → fresh fixture baseline,
  selected recovery only, exact expected effects and independent retained checks.
- Failed task leaves a live writer → track actual owned processes, cancel/exit
  through product controls, wait beyond its scheduled effect and verify cleanup.
- Thirty runs give limited reliability coverage → report the measured task/model/
  platform scope and failure counts; do not extrapolate an autonomy guarantee.

## Migration Plan

No product-state migration is needed. First qualify the Linux predecessor
candidate, freeze candidate/provider/fixture identities and readiness, then run
the matrix serially in disposable projects. Preserve outstanding user adoption,
merge and canonical closure explicitly until final delivery.
Review repairs and rerun affected rows, publish evidence with current-head CI and
let the user merge. After merge, sync the harness delta to canonical specs and
archive only after verified delivery. Remove only exact owned stopped fixtures;
retain unsuccessful attempts and evidence until their disposition is reviewed.

## Open Questions

- The predecessor's explicit disjoint read-only shell grant adoption remains a
  user review/merge decision. Controlled candidate tests state that proposed
  boundary and do not decide adoption or deploy it to an installed runtime.
- Provider access and exact supported model/effort on each Host need live inventory
  before freezing tasks; current macOS profile history is not Linux credential proof.
