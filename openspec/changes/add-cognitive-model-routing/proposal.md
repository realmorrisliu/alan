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

- `provider-request-controls`: Resolve controls against the selected operation
  and Connection, not a presumed fast/deep generation role.

## Impact

Agent Machine, AgentFS projections, llmfs/provider operations and evaluation
tests. No Kernel cognition type, mandatory Process-per-call, global router,
new package kind or renderer launch authority.

Status: parked for the next planning pass on merged main. This is accepted
direction, not an implementation-ready slice. Before apply, that pass must
complete the llm-file-server and agent-namespace-runtime deltas, choose versioned
wire DTOs, reconcile state recovery, and explicitly activate the task scope.
