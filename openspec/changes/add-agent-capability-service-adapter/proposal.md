## Why

Alan Apps need a real Host Service API for Agent Capability before domain apps
can request AI behavior without embedding local chatbots or depending on the
Alan Agent UI. The safest first implementation is a compatibility adapter over
the existing Agent Execution Engine and daemon-backed session APIs, preserving
working behavior while exposing the new Agent Run, Context Grant, Result
Contract, streaming, yield, evidence, and audit semantics.

## What Changes

- Define an Agent Capability Service Host Service API for starting, scheduling,
  streaming, yielding, resuming, cancelling, and completing bounded Agent Runs.
- Implement the first compatibility adapter over current `alan-runtime` and
  daemon-backed session APIs.
- Translate V1 Agent Capability descriptors plus Context Grants into current
  execution inputs without exposing prompt dumps as the OS contract.
- Translate current events, yields, tool calls, child runs, rollout evidence,
  and terminal outcomes into Agent Run lifecycle, Result Contract output,
  evidence, and audit records.
- Keep Alan Kernel free of provider execution, daemon clients, concrete memory
  stores, sandbox backends, and runtime supervision.

## Capabilities

### New Capabilities

- `agent-capability-service-adapter`: Defines and implements the first Host
  Service API adapter for Agent Capability over the existing Alan Agent
  execution stack.

### Modified Capabilities

- `daemon-api-contract`: May later expose Agent Capability Service endpoints,
  but existing session endpoints remain compatibility paths.
- `runtime-core-contract`: Current runtime remains the internal Agent Execution
  Engine used by the adapter.

## Impact

- Affected crates: likely `alan` daemon Host Service surfaces, `alan-runtime`
  as the execution engine, and the Kernel crate for semantic type consumption.
- Affected current clients: existing daemon/TUI session behavior must remain
  compatible.
- Affected future apps: UPDF, Groove Master, and Alan Agent can request Agent
  Capability through a shared Host Service API.

