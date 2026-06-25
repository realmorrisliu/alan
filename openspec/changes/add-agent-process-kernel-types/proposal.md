## Why

Alan Kernel needs minimal Agent Process anchors before Agent Runtime Service and
AgentFS can project current runtime behavior into the file/process model. The
target model is defined by `define-agent-process-os-model`; this change keeps
runtime execution, model providers, memory storage, policy evaluation, tool
execution, and compatibility transport outside Kernel.

## What Changes

- Add Kernel anchors for ordinary Process and Agent Process identity, parentage,
  credentials, descriptors, access rights, lifecycle, status, and process-table
  entries.
- Add namespace/file anchors needed for `/proc`, `/srv`, service mounts, and
  future AgentFS attachment points.
- Add compatibility references for current session/runtime ids only as
  temporary runtime references, not durable Kernel identity.
- Keep AgentFS schemas, machine tape, request/action files, Tool manifests,
  Skill packages, Memory Stores, policy descriptors, and Agent Runtime Service
  execution above Kernel.
- Add dependency-boundary tests proving Kernel types do not depend on
  `alan-runtime`, `alan-protocol`, compatibility transport clients, providers,
  memory stores, or sandbox implementations.

## Capabilities

### New Capabilities

- `agent-process-kernel-types`: Owns minimal Alan Kernel anchors for Agent
  Processes and service-mounted file trees.

### Modified Capabilities

- `alan-kernel-contract`: Advances the Kernel incubation contract by adding
  Process / Agent Process anchors and service mount anchors only, not agent
  runtime execution.

## Impact

- Affected crates: `alan-kernel`.
- Affected future work: unblocks Agent Runtime Service, AgentFS projection, Alan
  Shell file-native workflows, and optional Alan Agent workspace projection.
- Affected current behavior: none; this change adds semantic model types and
  tests only.
