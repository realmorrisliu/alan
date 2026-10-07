## Why

System 1/System 2 were originally modeled as fast/deep generating child Agents.
ADR-0055 accepts one Agent Machine composing deterministic computation, typed
evaluation and generation. The old mandatory two-Process routing plan is superseded.

## What Changes

- Keep Process lifecycle and namespace authority; spawn only when isolation or
  independently bounded work requires it.
- Introduce explicit evaluation versus generation capability and typed results,
  without encoding scores as assistant text.
- Define Machine completion, waiting and recovery separately from final prose.
- Reuse action governance, effect lifecycle and rollout/checkpoint evidence.
- Keep fallback bounded and explicit input modes authoritative.

## Capabilities

### New Capabilities

- `cognitive-model-routing`: Machine-owned mixed-capability decisions,
  evidence, bounded fallback and unchanged authorization.

### Modified Capabilities

- `alan-shell`: Explicit invocation-scoped advice evaluator selection, separate from generation.

- `provider-request-controls`: Resolve controls against the selected operation
  and Connection, not a presumed fast/deep generation role.

## Impact

Agent Machine, AgentFS projections, llmfs/provider operations and evaluation
tests. No Kernel cognition type, mandatory Process-per-call, global router,
new package kind or renderer launch authority.

Status: the bounded finite-choice Connection entry is active under
[entry-slice.md](entry-slice.md). This delivers the typed operation boundary and
fixture contracts only; mixed-Machine composition, a real evaluator adapter,
shadow qualification and automatic routing remain unfinished. The broader
[next-planning.md](next-planning.md) roadmap is not activated wholesale.
