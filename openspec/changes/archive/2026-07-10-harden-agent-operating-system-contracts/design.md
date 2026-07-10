## Context

The analyzed session exposed a product boundary problem rather than a single
bug. alan has separate machinery for delegated runtimes, workspace guards,
rollout persistence, approval checkpoints, task-store status, daemon APIs, and
memory surfaces. Those subsystems are individually useful, but the user
experience is not yet a coherent agent operating system:

- delegation can launch a child target that lacks the task's required
  capabilities;
- child output references can point at paths the parent runtime cannot read;
- prompt-facing truncation can erase the durable evidence needed to audit a
  final answer;
- child progress and approval resume states are not always visible as running
  work;
- generated memory surfaces can treat a one-character follow-up as the current
  goal.

This change turns those observations into product contracts for capability-aware
delegation, auditable evidence, and human-visible lifecycle state.

## Goals / Non-Goals

**Goals:**

- Route delegation using explicit task requirements and known target
  capabilities.
- Preserve full answer-supporting evidence as authorized runtime artifacts
  while keeping prompt-facing payloads bounded.
- Expose delegated work and approval/resume progress through session events,
  daemon DTOs, and client timeline semantics.
- Keep workspace isolation intact by reading child artifacts through runtime
  authorization instead of parent filesystem access.
- Improve generated memory/handoff surfaces so they preserve substantive goals.

**Non-Goals:**

- Build a general search/query UI for all rollouts.
- Remove truncation from model-facing context.
- Give parent runtimes raw filesystem access to child workspaces.
- Replace OpenSpec, rollout, or memory storage formats wholesale.
- Implement parallel multi-agent planning or scheduling beyond the existing
  delegated child model.

## Decisions

### 1. Capability matching is a runtime contract, not only prompt guidance

alan should derive a bounded capability requirement record before launching
delegated work or choosing a recovery path for a failed delegated result. The
record can start small: workspace read/write, shell, network, GitHub, browser,
side effects, target workspace, and artifact access.

Each built-in delegated target should expose a capability descriptor. The
runtime compares required capabilities with the descriptor and either launches
the target, chooses a direct tool path, requests approval, or reports a visible
capability gap.

Alternative considered: rely on prompt instructions that tell the model to pick
the right child. That is insufficient because the failure mode is itself a
routing error; the runtime must make the capability mismatch observable and
recoverable.

### 2. Evidence artifacts are separate from prompt projections

Rollout and tape projection should stay compact for model performance, but
answer-supporting evidence needs durable provenance. Large tool stdout, child
output, and structured results should be stored as redacted evidence artifacts
with stable artifact ids, digest, original size metadata, source tool/child ids,
and rollout/session references.

The prompt-facing tool message may contain a bounded preview plus an
`evidence_ref`. The durable rollout should keep enough metadata to prove what
artifact existed and how to retrieve it under authorization.

Alternative considered: raise the durable string truncation limit. That helps
some sessions but does not solve auditability, redaction, child-workspace
authorization, or answer-level provenance.

### 3. Artifact reads go through runtime authorization

`output_ref` should not be a raw path contract. Raw paths are useful for local
debugging, but parent runtimes can be scoped to a different workspace. The
runtime should expose a child-output or evidence-artifact read surface that
checks parent session ownership, child-run association, and workspace policy
before returning bounded content.

Alternative considered: exempt child rollout paths from workspace guard when the
parent has a child-run reference. That would mix filesystem policy with runtime
ownership and make later remote/relay clients harder to reason about.

### 4. Lifecycle state has explicit approval/resume and delegated-progress phases

Task-store and client-facing state should distinguish `awaiting_approval`,
`resuming`, `running`, and terminal states. Approval replay must transition back
to running before the resumed tool executes or the next model step begins.

Child work should generate parent-visible lifecycle events at start, progress,
and terminal boundaries. The parent should not appear idle while a delegated
child is running.

Alternative considered: infer state from the last turn event only. That misses
runtime confirmation resumes and long-running delegated work where the parent
tool call does not return until completion.

### 5. Memory goal derivation uses salience filters

Generated fallback memory should prefer explicit plan state or the latest
substantive user task. It should ignore runtime control messages, tool approval
controls, and low-information fragments such as a single-letter reply unless no
better goal is available.

Alternative considered: keep using the latest user message. That is simple but
actively damages continuity when a user sends a short acknowledgement after a
substantive task.

## Risks / Trade-offs

- [Risk] Capability descriptors can drift from actual child permissions. →
  Add tests that exercise known targets and failure paths, and include
  capability descriptors in child-run metadata.
- [Risk] Evidence artifacts can leak sensitive data if stored before redaction.
  → Reuse rollout redaction rules, record redaction summaries, and forbid
  plaintext secrets in durable artifacts.
- [Risk] More lifecycle states can complicate clients. → Keep a small canonical
  state enum and provide display labels through shared DTO semantics.
- [Risk] Artifact storage can grow without bounds. → Treat retention as part of
  session/rollout retention and include original size metadata for cleanup
  policy.
- [Risk] Salience filtering can skip a real short command. → Preserve recent
  message context and only suppress short fragments for generated goal fields,
  not from the conversation history itself.

## Migration Plan

1. Add capability descriptors and requirement classification behind internal
   runtime structs without changing public behavior.
2. Add evidence artifact persistence and read APIs while continuing to emit
   existing previews and rollout paths for compatibility.
3. Update delegated-result handoff to prefer authorized artifact refs.
4. Extend lifecycle state transitions and client DTOs; keep old terminal state
   labels compatible where possible.
5. Update memory surface fallback logic and tests.
6. Update built-in delegated skill instructions once the runtime contract is
   available.

## Open Questions

- Should evidence artifacts live inline beside rollout JSONL records, in a
  per-session artifact directory, or in a small content-addressed store?
- Which exact capability vocabulary should be exposed to user-facing surfaces in
  the first iteration?
- Should final answers show evidence provenance by default, or should provenance
  remain available through debug/inspection surfaces unless the user asks?
