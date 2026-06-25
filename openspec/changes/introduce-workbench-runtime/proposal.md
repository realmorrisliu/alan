## Why

Alan needs a higher-level runtime model for modern agent/editor/IDE-style work
without turning Ratatui into another app framework or folding the future
programmable environment into the current agent session protocol. The current
Rust TUI proves the pressure point: conversation, yields, tool calls, task-like
child runs, and streaming UI are hard-coded around Alan agent events instead of
a reusable semantic workbench model.

This change is an incubation slice under the programmable environment
constitution. It aims to prove a reusable runtime substrate boundary for
objects, commands, buffers, views, queries, actors, tasks, artifacts, evidence,
ledgers, and renderer hosts. It does not claim to implement the complete
programmable environment product.

## What Changes

- Introduce a `workbench-core` contract for semantic runtime primitives:
  actors, objects, buffers, views, commands, queries, subscriptions, tasks,
  events, artifacts, evidence, activity ledgers, and projections.
- Declare workbench as an environment-core/runtime-substrate slice that is
  constrained by `programmable-environment-product`, while explicitly deferring
  full product MVP scope such as first-launch local work discovery, broad object
  browsing, WASM extension loading, SwiftUI hosting, and environment apps.
- Define command, query, task, and replay boundaries so mutations are mediated
  through commands, reads through queries, observation through subscriptions,
  and ledger replay never re-executes side effects.
- Keep the workbench core independent from `alan-protocol`, Ratatui, SwiftUI,
  Tokio handles, macOS shell `ContentInstance`, and any specific renderer.
- Define an Alan agent adapter contract that maps existing session metadata,
  `alan_protocol::EventEnvelope`, yields, tool calls, and child-run records into
  workbench tasks, conversation views, forms, artifacts, and evidence.
- Define a renderer-host contract where Ratatui and SwiftUI consume semantic
  view snapshots and translate host input into workbench input intents or
  command invocations, without owning the application runtime.
- Scope the first implementation slice to an Alan agent conversation vertical
  slice: conversation view, task tree, form/yield handling, command invocation,
  and compatibility-first integration with the existing `crates/tui` daemon
  wiring.
- Defer WASM runtime loading, renderer extensions, full terminal emulation,
  full text-editor behavior, generic object-browser breadth, and SwiftUI host
  implementation until the core contract and Ratatui slice are proven.

## Capabilities

### New Capabilities

- `workbench-core-contract`: Defines the renderer-independent semantic runtime
  model, including descriptors, command/query/subscription surfaces, task
  events, actor provenance, artifact/evidence provenance, activity ledgers,
  projection replay, and native-authority boundaries.
- `workbench-agent-adapter-contract`: Defines how the existing Alan agent
  daemon/session protocol is adapted into workbench objects, buffers, views,
  commands, task events, forms, artifacts, and evidence without making
  workbench core depend on `alan-protocol`.
- `workbench-renderer-host-contract`: Defines the boundary for Ratatui, SwiftUI,
  and future renderer hosts: semantic view snapshots, renderer adapters, host
  input adapters, view-local state, and host-local layout.

### Modified Capabilities

- None.

## Impact

- New crates are expected in a future implementation slice, initially shaped as
  `workbench-core`, `workbench-agent`, and `workbench-ratatui`.
- `crates/tui` remains daemon-backed and becomes the first compatibility-first
  consumer of the workbench agent and Ratatui adapters.
- Existing Alan agent runtime, daemon APIs, `alan-protocol`, macOS shell
  `ContentInstance`, and accepted Ratatui behavior remain intact during the
  first slice.
- The design is compatible with later SwiftUI and WASM extension work, but this
  change does not implement either one.
- This change proves only a subset of the programmable environment constitution:
  runtime semantics, agent adaptation, task/projection modeling, command/query
  surfaces, host snapshot rendering, and compatibility migration. Local-first
  first-launch discovery, complete app workflows, and WASM extensions are
  deferred to follow-up incubation changes.
