## Why

Alan needs a higher-level Alan OS model for modern agent/editor/IDE-style work
without turning Ratatui into another app framework or collapsing Alan Kernel into
the current agent session protocol. The current
Alan Shell proves the pressure point: conversation, yields, tool calls, task-like
child runs, and streaming UI are hard-coded around Alan Agent events instead of
a reusable semantic Alan Kernel model.

This change is an incubation slice under the Alan OS constitution. It aims
to prove the first reusable Alan Kernel boundary while keeping the Kernel close
to UNIX's file-tree shape: namespace and mounts, paths, files, descriptors,
access rights, credentials, processes, and the process table are the underlying
substrate. Streams are file kinds, and process outputs are ordinary files or
stream files. Typed object, command, buffer, view, query, subscription,
capability, task, artifact, and evidence surfaces layer over that substrate as
app/service descriptors or interpretations.
It does not claim to implement the complete
Alan product, and it is not middleware between the current Agent Execution
Engine and Alan Shell.

## What Changes

- Introduce an `alan-kernel` crate from the `alan-kernel-contract` primitives:
  namespace and mounts, paths, files, descriptors, access rights, credentials,
  processes, and process-table semantics, plus compatibility hooks for
  app/service descriptors and projections.
- Define Alan Kernel as a semantic UNIX substrate whose canonical ontology is
  `Namespace`, `Mount`, `Path`, `File`, `Descriptor`, `AccessRights`,
  `Credential`, `Process`, and `ProcessTable`; current V1 capability/object/
  task/view/command/query/subscription names remain app/service compatibility
  surfaces rather than replacing that smaller model, while Artifact and
  Evidence move to the Agent/App interpretation layer over Kernel files,
  processes, descriptors, and stream offsets.
- Define Agent Process as the Kernel-visible process category for agent work,
  alongside ordinary Process. Agent Process identity, parentage, descriptors,
  lifecycle, and process table entries belong in Kernel; tape, model calls,
  requests, actions, and machine state are AgentFS surfaces served above Kernel.
- Define durable Kernel identity through namespace-qualified paths,
  process-table entries, mounted file trees, and native authority references;
  typed opaque ids remain runtime references or V1 surface ids rather than the
  canonical file identity.
- Define streams as file kinds that can be read, tailed, watched, and resumed
  from offsets; subscriptions remain watch operation surfaces over files,
  process endpoints, or stream files rather than a separate event system.
- Downgrade the former Activity Ledger / Kernel Journal concept into service-
  and app-owned stream files: there is no Kernel-owned semantic journal, and
  replay/recovery uses named service stream files, app stream files, and native
  file inspection.
- Treat Alan-owned names as the durable naming direction: `alan-kernel` for the
  substrate, future `alan-agent` for the built-in Alan Agent app module, and
  `alan-shell` for the Alan Shell interaction path.
- Declare this as an Alan Kernel implementation slice that is
  constrained by `programmable-environment-product`, while explicitly deferring
  full product MVP scope such as first-launch local work discovery, broad object
  browsing, WASM extension loading, SwiftUI hosting, and Alan Apps.
- Position this change as the first Alan OS spine implementation slice in
  the canonical roadmap: it prepares Alan Kernel and the first app/host
  contracts before broader Alan Agent, host, Groove Master, or UPDF migration.
- Align with `define-agent-process-os-model`'s
  `agent-process-os-model`: `alan-kernel` may define Process and Agent Process
  identity, Credentials, Paths, Files, stream Files, Descriptors, Access Rights,
  namespaces/mounts, `/proc`, `/srv`, service-mount anchors, and Access Checks.
  AgentFS schemas, tape, model calls, request files, action files, Tool
  manifests, Skill packages, memory storage, and policy evaluation belong above
  Kernel.
- Define operation-surface and replay boundaries so app/service executable
  command files spawn processes, queries inspect files or snapshots,
  subscriptions watch files/processes/stream files, and service-owned stream
  replay never re-executes side effects.
- Treat command/query/subscription registries as V1 descriptor discovery and
  compatibility caches over the namespace rather than as independent Alan
  Kernel ontology.
- Keep the Alan Kernel independent from `alan-protocol`, Ratatui, SwiftUI,
  Tokio handles, macOS shell `ContentInstance`, and any specific renderer.
- Define a compatibility projection contract that maps current Agent Execution
  Engine session metadata, `alan_protocol::EventEnvelope`, yields, tool calls,
  and child-run records into Agent Process file surfaces, AgentFS IO, request
  files, action files, machine events, optional workspace projections, artifacts,
  and Agent/App evidence.
- Define a renderer-host contract where Ratatui and SwiftUI consume semantic
  view snapshots and translate host input into semantic input intents or
  command invocations, without owning the application runtime.
- Scope the first implementation slice to the Agent Process spine and the Alan
  Shell compatibility path: conversation IO, request handling, action
  projection, command invocation, and compatibility-first integration with the
  existing `crates/tui` host and current service/transport implementation.
- Defer WASM runtime loading, renderer extensions, full terminal emulation,
  full text-editor behavior, generic object-browser breadth, and SwiftUI host
  implementation until the core contract and Ratatui slice are proven.

## Capabilities

### New Capabilities

- `alan-kernel-contract`: Defines the renderer-independent Alan Kernel
  model, including namespace/mount/path/file/descriptor/access-right/
  credential/process/process-table descriptors, namespace-shaped identity,
  app/service operation descriptors for command/query/subscription, stream
  files and offsets, process/task compatibility events, credential causation,
  service-owned stream file references, projection replay, native-authority
  boundaries, ordinary Process and Agent Process anchors, `/proc`, `/srv`, and
  service mount anchors without concrete agent execution.
- `alan-agent-adapter-contract`: Defines how the built-in Alan Agent
  app remains an optional workspace over Agent Process file surfaces while the
  existing Agent Execution Engine and session protocol remain internal
  compatibility details. It projects current behavior into buffers, views,
  request/action files, artifacts, and Agent/App evidence without making Alan
  Kernel depend on `alan-protocol`.
- `alan-renderer-host-contract`: Defines the boundary for Ratatui, SwiftUI,
  and future renderer hosts: semantic view snapshots, renderer adapters, host
  input adapters, view-local state, and host-local layout.

### Modified Capabilities

- None.

## Impact

- New crates are expected to use Alan-owned names. The substrate crate is
  `alan-kernel`; future Alan Agent app work should target `alan-agent`;
  Alan Shell rendering/input work should stay inside the Alan Shell boundary.
- `crates/tui` remains on the current compatibility transport and becomes the
  first compatibility-first Alan Shell path over the Agent Process projection.
- Existing Agent Execution Engine behavior, current service/transport
  implementation, `alan-protocol`, macOS shell `ContentInstance`, and accepted
  Ratatui behavior remain intact during the first slice as internal
  implementation or host compatibility paths.
- Agent Runtime Service, AgentFS, Tool/Skill package layout, and compatibility
  transport extraction are deferred to follow-up slices over the current Agent
  Execution Engine and session APIs.
- The design is compatible with later SwiftUI and WASM extension work, but this
  change does not implement either one.
- This change proves only a subset of the Alan OS constitution:
  Alan Kernel semantics, Agent Process projection, request/action projection modeling,
  operation-surface modeling for commands/queries/subscriptions, host snapshot
  rendering, and compatibility migration. Local-first first-launch discovery,
  complete app workflows, and WASM extensions are deferred to follow-up
  incubation changes.
- Roadmap impact: later changes should finish this spine before treating Groove
  Master or UPDF as implementation targets, and should migrate Alan Shell and Alan
  for macOS as hosts rather than as Alan Apps.
