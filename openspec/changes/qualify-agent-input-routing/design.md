# Design

## Context and ownership

This is the active destination for the predecessor's unimplemented tasks 3.1–3.4
and their owning spec deltas. ADR-0058's accepted direction and original interview
remain in the predecessor; this document makes the delivery boundary explicit.
The existing Machine is the consumer; `add-cognitive-model-routing` owns generic
typed operations and provider capability discovery. Jev is a candidate, not delivered support.

## Decisions

- `!` and `:` bypass classification; pending responses are consumed first. Preserve
  original command text and use the same governed dispatch for interactive and redirected input.
- Typed results are command, Agent or ambiguous. Ambiguity requests clarification;
  unavailable/malformed/timeout results use bounded governed Agent fallback or report
  unavailable generation. Neither failure nor cancellation may select direct execution.
- Before measuring candidates, freeze labeled discussion, quoted commands, explicit
  requests, ambiguity and adversarial cases, plus numeric quality/latency/cost budgets.
  Record false execution classifications, p50/p95 latency and cost against deterministic
  and generation-only baselines. Fixtures alone do not establish provider benefit.
- Shadow mode has no classification-caused effects. Known discussion-to-execution
  cases block activation until fixed and requalified. Actual activation requires an
  explicit decision and visible command route; `alan: ` must not conceal direct execution.
- Disabling returns to the unprefixed Agent baseline. No renderer state or separate
  execution authority may bypass existing rights, policies, approvals and sandbox.

## Delivery gates

Capability delivery and predecessor merge come first. Implement and qualify using
current code/provider evidence; record immutable candidate/configuration/corpus versions.
No numeric budgets are invented by this handoff. Set them before candidate evaluation,
then obtain the explicit activation decision. Sync only implemented and merged deltas.
