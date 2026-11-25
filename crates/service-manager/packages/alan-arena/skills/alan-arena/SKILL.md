---
name: alan-arena
description: |
  Orchestrate a thin, read-only multi-provider design bakeoff or root-cause
  competition using parallel `spawn_subagent` contenders across distinct
  configured models and an independent read-only verifier.

  Use this when:
  - Comparing competing architectural designs, refactors, or trade-offs across models
  - Running competing root-cause diagnoses before writing a fix
  - Delegating the multi-model comparison step of an imported community playbook (such as `pstack`)
metadata:
  short-description: Parallel multi-model read-only bakeoff with independent verifier
  tags: [arena, multi-model, bakeoff, architecture, subagent, verifier, read-only]
capabilities:
  required_tools: [read_file, grep, glob]
---

# Alan Arena (`alan-arena`)

Run an impartial, read-only multi-model comparison by dispatching parallel `spawn_subagent` contenders across distinct configured models and adjudicating their outputs through an independent read-only verifier.

## Non-Negotiable Safety & Neutrality Invariants

1. **Read-Only Contenders and Verifier**: Every `spawn_subagent` call in the arena MUST set `read_only: true`. Never grant `read_write` Host Mounts or mutation tools to contenders or the verifier.
2. **Configured Models Only**: Use only distinct model identifiers actually configured in the user's active Alan runtime or explicitly specified by the user. Never fabricate unconfigured provider/model names. If only one model is configured, state that honestly and either run distinct framing roles or ask the user to configure/select additional models.
3. **One-Off Ephemeral Streams**: Keep arena runs stateless (`persistence: "one_off"` when named workstreams are active) so contender and verifier runs auto-collapse cleanly without polluting persistent project streams.
4. **No Unverified Execution**: Treat contender outputs purely as competing proposals. Never apply edits or execute commands suggested by a contender without independent verification and Root confirmation.

## Composition with Community Playbooks (e.g., `pstack`)

When an imported community playbook bundle is mounted under `/lib/pkg/<pkg>/` (such as `pstack`'s `how`, `architect`, or `prove-it` skills and shared `playbooks/*.md`):

- Follow the upstream playbook's phases, constraints, and problem framing verbatim without forking or rewriting its instructions.
- Hand `$alan-arena` the bounded problem statement, relevant read-only paths, and acceptance criteria for the multi-model bakeoff step, then return the verifier's scored synthesis back into the playbook flow.

## Arena Protocol

### 1. Frame the Bounded Bakeoff Prompt

Before spawning any subagent, assemble:
- **Problem Statement**: Exact question, bug symptom, or architectural decision.
- **Read-Only Scope**: Relevant namespace paths (and explicit read-only `host_mounts` if needed).
- **Acceptance Criteria**: Concrete scoring axes (correctness, boundary/security adherence, simplicity, testability, operational risk).

### 2. Dispatch Parallel Read-Only Contenders

Emit a contiguous batch of 2 or more `spawn_subagent` tool calls in one turn so Alan executes them concurrently:

- Give each contender a distinct `role` (for example `arena-contender-1`, `arena-contender-2`, `arena-contender-3`) and a distinct configured `model` override.
- Set `read_only: true` on every call.
- Pass the exact same bounded problem statement, scope, and acceptance criteria to every contender, requesting concrete file/symbol citations, trade-offs, and failure modes.

### 3. Adjudicate with an Independent Read-Only Verifier

Once all contenders finish, spawn one independent `spawn_subagent` call with:
- `role: "arena-verifier"`
- `read_only: true`
- Optional distinct configured `model` override (never sharing a mutable workspace with contenders).
- Prompt containing ONLY the problem statement, acceptance criteria, read-only paths to spot-check, and neutrally labeled contender outputs (`Contender A`, `Contender B`, ...).

Instruct the verifier to:
1. Verify factual file/symbol claims read-only against the codebase/namespace.
2. Score each contender explicitly against every acceptance criterion.
3. Flag ungrounded claims, hidden coupling, security/boundary risks, or missing edge cases.
4. Recommend the strongest option (or concrete hybrid synthesis) with clear rationale.

### 4. Deliver Scorecard to Root / Operator

Present:
- Which distinct configured models participated (and any skipped/unavailable models).
- The verifier's side-by-side scorecard and verified findings.
- The recommended path and residual risks, stopping before any write or mutation unless explicitly directed.
