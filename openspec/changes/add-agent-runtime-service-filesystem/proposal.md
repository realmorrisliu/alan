## Why

Alan Apps and Alan Shell need a file/process-native way to run agents. The
target is not an HTTP/API adapter; it is Agent Runtime Service serving AgentFS
and executing Agent Processes while preserving current runtime behavior during
migration.

## What Changes

- Define Agent Runtime Service as a file-server service managed by Service
  Manager and posted under `/srv/agent-runtime`.
- Define the first AgentFS projection over current `alan-runtime`: status, IO,
  requests, actions, result, children, policy/context projection, and machine
  files.
- Adapt current session creation/attach behavior into Agent Process projection
  without making session ids the durable OS identity.
- Map current events, yields, tool calls, child runs, rollout evidence, and
  terminal outcomes into AgentFS files.
- Keep Alan Kernel free of provider execution, compatibility transport clients,
  concrete memory stores, sandbox backends, and runtime supervision.

## Capabilities

### New Capabilities

- `agent-runtime-service-filesystem`: Defines the first file-server service and
  AgentFS compatibility projection over the existing Agent Execution Engine.

### Modified Capabilities

- `runtime-core-contract`: Current runtime remains the internal Agent Execution
  Engine used by Agent Runtime Service.
- `daemon-api-contract`: Existing HTTP/WS routes remain compatibility transport
  and may later project onto AgentFS behavior.

## Impact

- Affected crates: likely current service/transport surfaces, `alan-runtime`,
  and projection code; `alan-kernel` remains limited to anchors.
- Affected current clients: existing session behavior must remain compatible.
- Affected future apps: Alan Shell, Alan Agent, UPDF, and Groove Master can move
  toward spawning Agent Processes and opening AgentFS files.
