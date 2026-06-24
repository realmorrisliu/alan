## Why

Alan Kernel needs the semantic types for Agent Capability before apps or Host
Service APIs can request AI-mediated work consistently. The target model is
defined by `define-agent-capability-os-model`, but the implementation must
start with model-only Kernel types so provider execution, daemon sessions,
memory storage, sandboxing, and runtime supervision stay outside Kernel.

## What Changes

- Add Agent Capability semantic types to the Alan Kernel incubation surface:
  descriptor ids, Agent Run identity, Context Grants, Result Contracts, Effect
  Classes, Command Risk, Execution Guard metadata, agent actor references,
  yielded task references, evidence references, and audit references.
- Encode the V1 descriptor taxonomy from
  `define-agent-capability-os-model/descriptor-taxonomy.md`: `explain`,
  `summarize`, `plan`, `propose_commands`, and `delegate`.
- Keep these types serializable, testable, renderer-independent, and free of
  concrete LLM/provider/session/sandbox dependencies.
- Add dependency-boundary tests proving Kernel types do not depend on
  `alan-runtime`, `alan-protocol`, daemon clients, provider clients, or sandbox
  implementations.

## Capabilities

### New Capabilities

- `agent-capability-kernel-types`: Owns Alan Kernel semantic types for Agent
  Capability descriptors, Agent Runs, Context Grants, Result Contracts, command
  risk, execution guard metadata, evidence, and audit.

### Modified Capabilities

- `alan-kernel-contract`: This implementation advances the existing Kernel
  incubation contract by adding Agent Capability semantics only, not service
  execution.

## Impact

- Affected crates: `alan-kernel`.
- Affected future work: unblocks Agent Capability Service compatibility adapter
  and Alan Agent Workspace projection.
- Affected current behavior: none; this change adds semantic model types and
  tests only.
