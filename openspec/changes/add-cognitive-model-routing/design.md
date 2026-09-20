## Context

ADR-0055 supersedes this change's original two-generating-Agent design.
See research-jev-and-fx.md and architecture-review.md for evidence. Current
runtime remains generation-based; namespace Tape is not complete recovery state.

## Goals / Non-Goals

Compose deterministic work, evaluation, generation and governed effects in the
existing Machine. Do not add Kernel types, a global scheduler, an obligatory
router call on every input, implicit grants or a universal AI gate for human
Shell commands. Do not implement Herdr topology or q executable distribution.

## Decisions

1. Explicit input mode and deterministic checks precede probabilistic advice.
   System 1/System 2 are work modes, not fixed Processes or permission tiers.
2. Connection profiles retain their metadata/secret owners. Operations declare
   capabilities; evaluation input/result retain types, provider identity, schema
   and model version. No extra_params-only or assistant-text representation.
3. Preserve clone/commit/result-or-events/status/ctl lifecycle where applicable.
   The exact operation path and versioned DTO are entry decisions for the next
   implementation plan, not settled wire compatibility promises here.
4. Machine owns decision state. AgentFS exposes it; rollout/checkpoint owns
   durable recovery evidence. Model input is a task-specific projection.
   Do not create an independent cognitive history beside existing execution
   evidence. Do not treat today's text Tape CAS root as a full checkpoint.
5. All Agent-originated effects reuse existing deterministic policy and effect
   lifecycle. Model confidence never grants access. Unknown external effects
   require reconciliation, not replay. Ordinary human Shell governance remains
   distinct. Evaluation timeout/unavailability cannot auto-allow an action.
6. Candidate sets come from the Process's resolved, available capabilities,
   respecting Skill exposure and explicit references. Static rubrics are normal
   definition/Skill resources; installing them is not execution authorization.
7. Work completion may be structured-only. Wait, resume, cancellation, failure
   and Process exit have separate meanings. State commit uses transition write
   ownership, not a generation-only lease assumption.
8. Fallback is bounded by configured attempts/time/cost; cancellation stops
   subsequent dispatch. Do not guarantee any performance before task-level eval.

## Risks / Trade-offs

- Incomplete contract replacement → do not apply until owning llmfs/runtime
  deltas and recovery invariants have been reviewed.
- Miscalibrated choices → shadow evaluation against deterministic and generation
  baselines, including no-match and unavailable outcomes.
- Replay side effects → preserve pending effect identity and Unknown handling.
- Excessive scope → one bounded existing task before memory learning, watchers,
  remote products or general executable packaging.

## Migration Plan

First deliver the usable-agent task through the existing generation Connection
as specified in [next-planning.md](next-planning.md). Then select one bounded
evaluation point inside that working task and complete the entry contracts.
Necessary Machine recovery contracts may land earlier for the reliability slice;
they do not require evaluation or Jev. Reuse those contracts rather than creating
another checkpoint path. Real-provider benefit measurement belongs to the later
Jev adapter change; fixtures establish state-machine correctness only.
Implementation, rollout and canonical sync remain pending.
