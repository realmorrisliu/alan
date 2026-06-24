## 0. Alan OS Alignment

- [x] 0.1 Declare this change as an Alan Kernel implementation slice
  under `programmable-environment-product`, not the complete product or middleware
  between the current Agent Execution Engine and Alan TUI.
- [x] 0.2 Record which constitution criteria the first slice proves: objects,
  commands, buffers, views, queries, actors, tasks, artifacts, evidence, ledgers,
  native references, no-side-effect replay, and renderer-host snapshots.
- [x] 0.3 Record deferred constitution criteria: broad first-launch local work
  discovery, complete Alan App workflow, SwiftUI host implementation,
  WASM extension loading, and universal resource addressing.
- [x] 0.4 Keep the Alan Agent conversation path as the first built-in Alan App
  vertical slice while documenting that the current Agent Execution Engine
  and daemon-backed Host Service Implementation are implementation details, not
  Alan OS or Alan Kernel.
- [x] 0.5 Record the durable crate naming direction: `alan-kernel` for the
  substrate, future `alan-agent` for the built-in Alan Agent app module,
  `alan-terminal-ui` or a justified `alan-terminal-renderer` for terminal
  rendering, `alan-runtime` -> `alan-agent-engine` if renamed, and
  `alan-protocol` -> `alan-agent-protocol` if it remains session-specific.
- [x] 0.6 Record this change as the first Alan OS spine slice in the
  roadmap, with Alan Agent app projection, Alan TUI host integration, Alan for
  macOS host migration, Groove Master, and UPDF gated on usable Kernel contracts.
- [x] 0.7 Align with `define-agent-capability-os-model` by keeping Agent
  Capability semantic primitives in Kernel and deferring Agent Capability
  Service execution to Host Service APIs / compatibility adapters.

## 1. Core Crate Skeleton

- [x] 1.1 Add `alan-kernel` to the Cargo workspace with no dependency on `alan-protocol`, Ratatui, Crossterm, AppKit, SwiftUI, or Tokio task handles.
- [x] 1.2 Define typed opaque ids for actors, objects, buffers, views, commands, queries, subscriptions, tasks, artifacts, evidence, and events.
- [x] 1.3 Define descriptor types for actors, objects, buffers, views, commands, queries, subscriptions, tasks, artifacts, and evidence.
- [x] 1.4 Add native-reference descriptor support so files, Git repositories, agent sessions, terminal handles, and domain-owned resources keep external authority outside Alan Kernel ids.
- [x] 1.5 Add compile-time or focused tests proving `alan-kernel` remains independent from Alan protocol and renderer-host crates.
- [x] 1.6 Ensure the substrate crate/package uses the durable `alan-kernel`
  name before adding new durable app or renderer crates.

## 2. Command, Query, And Subscription Model

- [x] 2.1 Implement command descriptor and invocation types with target, args schema, actor, capability, risk, undo or recovery, and invocation-hint metadata.
- [x] 2.2 Implement query descriptor and invocation types with read-only result references and capability metadata.
- [x] 2.3 Implement subscription descriptors and update or invalidation messages for object, buffer, view, task, query, and command-availability dependencies.
- [x] 2.4 Add registry traits or lightweight in-memory registries for commands, queries, and subscriptions.
- [x] 2.5 Add tests proving mutation routes through command invocation while queries and subscriptions remain read-only or observational.

## 3. Ledger, Projection, And Task Runtime State

- [x] 3.1 Define `KernelEvent` with schema version, event id, sequence, timestamp, actor id, causation id, correlation id, and typed event kind.
- [x] 3.2 Define task events for started, progress, output appended, yielded, resumed, side-effect planned, side-effect committed, artifact created, evidence attached, completed, failed, and cancelled states.
- [x] 3.3 Implement in-memory activity ledger replay with no side effects.
- [x] 3.4 Implement JSONL activity ledger append and replay behind the same ledger trait.
- [x] 3.5 Implement an in-memory projection store for current objects, buffers, views, tasks, artifacts, evidence, command availability, and dirty-view invalidation.
- [x] 3.6 Add replay tests proving projections rebuild from ledger state without rerunning shell commands, agent turns, file writes, network calls, terminal input, or imports.

## 4. Semantic View Snapshots

- [x] 4.1 Define `ViewSnapshot` with view id, buffer id, version, view kind, semantic model, actions, diagnostics, selection, and focus state.
- [x] 4.2 Define strongly typed built-in view models for conversation, task tree, command palette, form, object list, text document read/review, diff, and log stream.
- [x] 4.3 Define schema-versioned dynamic extension view payload support using JSON for unknown or domain-specific views.
- [x] 4.4 Separate semantic view state from host render state in the type model and tests.
- [x] 4.5 Add snapshot tests for conversation, form, task tree, and command palette models as the first implementation surface.

## 5. Alan Agent App Module

- [x] 5.1 Add `alan-agent` to the workspace as the built-in Alan Agent app
  module/projection layer, with dependencies on `alan-kernel` and the agent
  protocol plus Host Service API surfaces needed for internal adaptation.
- [x] 5.2 Map Alan session metadata into an agent session object, conversation buffer, and initial conversation view descriptor.
- [x] 5.3 Register agent commands for submit turn, resume yielded task, interrupt or cancel active work, compact context, and rollback turn history.
- [x] 5.4 Map `alan_protocol::EventEnvelope` turn, text, thinking, tool, plan, warning, error, and yield events into Alan Kernel events and projections.
- [x] 5.5 Map Alan child-run records or lifecycle events into Alan Kernel child task descriptors and task events.
- [x] 5.6 Project Alan confirmation, structured input, and dynamic tool yields into yielded task state and semantic form or approval snapshots.
- [x] 5.7 Add fixture tests using representative Alan event envelopes to verify conversation, form, task tree, artifact, and evidence projections.
- [x] 5.8 Add Agent Capability compatibility-adapter fixtures proving Context
  Grants and Result Contracts can wrap current Agent Execution Engine behavior
  without moving provider execution, daemon clients, memory stores, or sandbox
  execution into Alan Kernel.

## 6. Alan TUI Renderer Host

- [x] 6.1 Implement Alan TUI renderer/input adaptation over `alan-kernel` inside
  `alan-terminal-ui`; split an `alan-terminal-renderer` crate only if the renderer
  needs an independent package boundary.
- [x] 6.2 Implement Ratatui renderers for conversation, form, task tree, and command palette semantic snapshots.
- [x] 6.3 Translate Crossterm key, paste, resize, and mouse events into host-local layout changes, semantic input intents, view-local input, or command invocations.
- [x] 6.4 Keep renderer-only state such as line wrapping, terminal cell cache, geometry, and frame timing out of Alan Kernel semantic state.
- [x] 6.5 Add Ratatui snapshot or test-backend coverage for the first built-in semantic view renderers.

## 7. Existing TUI Compatibility Integration

- [x] 7.1 Integrate `alan-agent` and the Alan TUI renderer path into `crates/tui`
  behind a compatibility-first path while preserving existing daemon wiring.
- [x] 7.2 Preserve session creation or attach, hydration, replay cursor handling, event stream reconnect, submission, resume, interrupt, compact, rollback, and pending-yield behavior.
- [x] 7.3 Run the semantic projection path in parallel with the existing reducer where useful and add parity tests before removing old reducer behavior.
- [x] 7.4 Migrate supported conversation, form, task-tree, and command-palette rendering to semantic snapshots after focused tests pass.
- [x] 7.5 Leave unsupported surfaces on the current TUI path until a semantic model and renderer are implemented.

## 8. Verification And Review

- [x] 8.1 Run focused `cargo test` coverage for `alan-kernel`, `alan-agent`,
  Alan TUI renderer code, and affected `alan-terminal-ui` tests.
- [x] 8.2 Run `cargo fmt --all` and the relevant clippy or workspace check target, or document any environment-blocked gate with focused passing tests.
- [x] 8.3 Run `openspec validate introduce-alan-kernel-runtime --strict`.
- [x] 8.4 Run `git diff --check -- openspec/changes/introduce-alan-kernel-runtime crates`.
- [x] 8.5 Perform a PR review pass against the Alan Kernel contracts, app projection
  boundaries, replay no-side-effects rule, and existing TUI compatibility.
- [ ] 8.6 After implementation is merged, sync accepted delta specs into `openspec/specs/` and prepare archive-readiness notes before archiving the change.
