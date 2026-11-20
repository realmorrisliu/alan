# Activated bounded entry slice — 2026-10-07

The user's four-phase goal activates typed evaluation after the locally qualified
lifecycle, real model-switch and UI slices (PR #1032/#1033/#1034). Those PRs remain
unmerged; canonical sync and archive are not authorized by this local evidence.
This supersedes only the queued status of the finite-choice shadow slice, not
unrestricted mixed-Machine execution or automatic command routing.

## Concrete operation boundary

Use the existing project task's input intent decision as an advice-only choice
among Process-supplied command, Agent and ambiguous candidate IDs. Explicit
prefixes and pending responses are deterministic before this boundary. The frozen
qualification corpus/budgets belong to qualify-agent-input-routing; its commit
`020779a7` precedes all candidate measurements.

A Connection advertises finite-choice evaluation only when its actual provider
callable implements it. Provider-family generation support is not evidence.
Opening `connections/<id>/evaluate` ReadWrite allocates an independent operation
using the existing data/events/ctl/status lifecycle and captures that Connection.
Legacy `clone` and its current version-2 generation DTO stay unchanged. Evaluation
requests use an independent llmfs version-1 DTO with schema `choice.v1`, original
input, 1–64 uniquely identified candidates and a 1–30,000 ms deadline.
Candidate IDs are 1–128 ASCII alphanumeric/dot/underscore/hyphen bytes; the
entire wire document is capped at 1 MiB. No credentials,
Tools, arbitrary JSON schemas or generation-only controls enter this DTO.

Provider-side types return Selected(candidate ID) or NoMatch plus usage, never an
assistant message or an executable command. llmfs validates selection membership
again, emits a typed evaluation result with captured provider/model provenance,
and uses existing terminal error/rejected/aborted records for failures. Unknown
billing cost remains null. The shared Connection meter and generation status
also expose unknown cost as null; the old never-incremented zero counter is
removed, so no consumer can mistake it for verified free operation. Evaluation quota is distinct from generation quota;
shared lifecycle does not relabel evaluations as generated answers.

The initial adapter interface defaults to unsupported. A mock proves contracts,
not capability or real-model benefit. Advertise a real adapter only after current
provider protocol verification and a live typed-result probe. The generation-only
baseline may parse advice for comparison but must remain labeled generation.

## Lifecycle and Machine ownership

Exactly one clunk commit can dispatch. Cancellation while waiting for the provider
lock, during the call or before publication wins through the shared terminal fence;
no result may follow aborted state. Deadline includes waiting for that lock. There
is one attempt, no implicit retry or evaluation-triggered generation fallback.

The Machine consumer records original submission, operation ID, captured Connection,
schema, deadline, outcome and fallback disposition in its existing durable execution
evidence. AgentFS exposes a read-only Machine projection; it owns no second history.
On recovery an unfinished evaluation is interrupted/unavailable, never silently
resubmitted or converted to command execution. Pending approvals, Tool effects and
Process lifecycle retain their current owners. The initial shadow collector observes
both admission surfaces without dispatching an evaluator-selected Tool or command.

## Implementation sequence and evidence

1. Provider finite-choice types and explicit unsupported default, with validation.
2. llmfs capability projection, evaluation allocation and shared lifecycle tests:
   malformed request, unknown candidate, unavailable, timeout, lock-wait cancellation,
   duplicate commit, late result, Connection replacement and independent quotas.
3. Real adapter capability probe, then mounted Connection evaluation and Machine
   shadow observation/evidence for both admission surfaces.
4. Collect all frozen repeats and both baselines; score missing cost as incomplete.
   Only then evaluate enablement. User activation remains a separate required decision.
