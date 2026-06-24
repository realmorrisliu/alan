## Why

Alan needs a higher-level Alan OS model for modern agent/editor/IDE-style work
without turning Ratatui into another app framework or collapsing Alan Kernel into
the current agent session protocol. The current
Alan TUI proves the pressure point: conversation, yields, tool calls, task-like
child runs, and streaming UI are hard-coded around Alan Agent events instead of
a reusable semantic Alan Kernel model.

This change is an incubation slice under the Alan OS constitution. It aims
to prove the first reusable Alan Kernel boundary across
objects, commands, buffers, views, queries, actors, tasks, artifacts, evidence,
ledgers, and renderer hosts. It does not claim to implement the complete
Alan product, and it is not middleware between the current Agent Execution Engine
and Alan TUI.

## What Changes

- Introduce an `alan-kernel` crate from the `alan-kernel-contract` primitives:
  actors, objects, buffers, views, commands, queries, subscriptions, tasks,
  events, artifacts, evidence, activity ledgers, and projections.
- Treat Alan-owned names as the durable naming direction: `alan-kernel` for the
  substrate, future `alan-agent` for the built-in Alan Agent app module, and
  `alan-terminal-ui` or a justified `alan-terminal-renderer` for the terminal
  renderer path.
- Declare this as an Alan Kernel implementation slice that is
  constrained by `programmable-environment-product`, while explicitly deferring
  full product MVP scope such as first-launch local work discovery, broad object
  browsing, WASM extension loading, SwiftUI hosting, and Alan Apps.
- Position this change as the first Alan OS spine implementation slice in
  the canonical roadmap: it prepares Alan Kernel and the first app/host
  contracts before broader Alan Agent, host, Groove Master, or UPDF migration.
- Align with `define-agent-capability-os-model`: `alan-kernel` may
  define semantic Agent Capability primitives such as descriptors, Agent Run
  identity, Context Grant shape, Result Contract shape, command risk, execution
  guard metadata, evidence, and audit, but it does not implement Agent
  Capability Service execution.
- Define command, query, task, and replay boundaries so mutations are mediated
  through commands, reads through queries, observation through subscriptions,
  and ledger replay never re-executes side effects.
- Keep the Alan Kernel independent from `alan-protocol`, Ratatui, SwiftUI,
  Tokio handles, macOS shell `ContentInstance`, and any specific renderer.
- Define an Alan Agent app module/projection contract that maps current Agent
  Execution Engine and daemon-backed Host Service Implementation session metadata,
  `alan_protocol::EventEnvelope`, yields, tool calls, and child-run records into
  Alan Kernel tasks, conversation buffers/views, commands, forms, artifacts, and
  evidence.
- Define a renderer-host contract where Ratatui and SwiftUI consume semantic
  view snapshots and translate host input into semantic input intents or
  command invocations, without owning the application runtime.
- Scope the first implementation slice to the Alan Agent built-in app conversation
  workflow: conversation view, task tree, form/yield handling, command invocation,
  and compatibility-first integration with the existing `crates/tui` host and
  daemon-backed Host Service Implementation wiring.
- Defer WASM runtime loading, renderer extensions, full terminal emulation,
  full text-editor behavior, generic object-browser breadth, and SwiftUI host
  implementation until the core contract and Ratatui slice are proven.

## Capabilities

### New Capabilities

- `alan-kernel-contract`: Defines the renderer-independent Alan Kernel
  model, including descriptors, command/query/subscription surfaces, task
  events, actor provenance, artifact/evidence provenance, activity ledgers,
  projection replay, native-authority boundaries, and semantic Agent Capability
  primitives without concrete agent execution.
- `alan-agent-adapter-contract`: Defines how the built-in Alan Agent
  app uses the existing Agent Execution Engine and daemon-backed Host Service
  Implementation session protocol as internal implementation details and
  projects them into Alan Kernel objects, buffers, views, commands, task events,
  forms, artifacts, evidence, and Agent Capability compatibility paths without
  making Alan Kernel depend on `alan-protocol`.
- `alan-renderer-host-contract`: Defines the boundary for Ratatui, SwiftUI,
  and future renderer hosts: semantic view snapshots, renderer adapters, host
  input adapters, view-local state, and host-local layout.

### Modified Capabilities

- None.

## Impact

- New crates are expected to use Alan-owned names. The substrate crate is
  `alan-kernel`; future Alan Agent app work should target `alan-agent`;
  terminal renderer work should stay inside `alan-terminal-ui` unless an
  extracted `alan-terminal-renderer` is needed.
- `crates/tui` remains daemon-backed and becomes the first compatibility-first
  host/frame consumer of the Alan Agent app module and Alan TUI renderer path.
- Existing Agent Execution Engine behavior, Host Service APIs and the current
  daemon-backed Host Service Implementation,
  `alan-protocol`, macOS shell `ContentInstance`, and accepted Ratatui behavior
  remain intact during the first slice as internal implementation or host
  compatibility paths.
- The first Agent Capability Service implementation is deferred to a follow-up
  Host Service API / compatibility adapter over the current Agent Execution
  Engine and daemon-backed session APIs.
- The design is compatible with later SwiftUI and WASM extension work, but this
  change does not implement either one.
- This change proves only a subset of the Alan OS constitution:
  Alan Kernel semantics, agent projection, task/projection modeling, command/query
  surfaces, host snapshot rendering, and compatibility migration. Local-first
  first-launch discovery, complete app workflows, and WASM extensions are
  deferred to follow-up incubation changes.
- Roadmap impact: later changes should finish this spine before treating Groove
  Master or UPDF as implementation targets, and should migrate Alan TUI and Alan
  for macOS as hosts rather than as Alan Apps.
