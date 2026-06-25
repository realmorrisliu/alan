## 0. Programmable Environment Alignment

- [x] 0.1 Declare workbench as an environment-core runtime substrate incubation
  slice under `programmable-environment-product`, not the complete product.
- [x] 0.2 Record which constitution criteria the first slice proves: objects,
  commands, buffers, views, queries, actors, tasks, artifacts, evidence, ledgers,
  native references, no-side-effect replay, and renderer-host snapshots.
- [x] 0.3 Record deferred constitution criteria: broad first-launch local work
  discovery, complete environment app workflow, SwiftUI host implementation,
  WASM extension loading, and universal resource addressing.
- [x] 0.4 Keep the Alan agent conversation path as the first vertical slice while
  documenting that it is a compatibility-first substrate proof, not the whole
  programmable environment MVP.

## 1. Core Crate Skeleton

- [ ] 1.1 Add `workbench-core` to the Cargo workspace with no dependency on `alan-protocol`, Ratatui, Crossterm, AppKit, SwiftUI, or Tokio task handles.
- [ ] 1.2 Define typed opaque ids for actors, objects, buffers, views, commands, queries, subscriptions, tasks, artifacts, evidence, and events.
- [ ] 1.3 Define descriptor types for actors, objects, buffers, views, commands, queries, subscriptions, tasks, artifacts, and evidence.
- [ ] 1.4 Add native-reference descriptor support so files, Git repositories, agent sessions, terminal handles, and domain-owned resources keep external authority outside workbench ids.
- [ ] 1.5 Add compile-time or focused tests proving `workbench-core` remains independent from Alan protocol and renderer-host crates.

## 2. Command, Query, And Subscription Model

- [ ] 2.1 Implement command descriptor and invocation types with target, args schema, actor, capability, risk, undo or recovery, and invocation-hint metadata.
- [ ] 2.2 Implement query descriptor and invocation types with read-only result references and capability metadata.
- [ ] 2.3 Implement subscription descriptors and update or invalidation messages for object, buffer, view, task, query, and command-availability dependencies.
- [ ] 2.4 Add registry traits or lightweight in-memory registries for commands, queries, and subscriptions.
- [ ] 2.5 Add tests proving mutation routes through command invocation while queries and subscriptions remain read-only or observational.

## 3. Ledger, Projection, And Task Runtime State

- [ ] 3.1 Define `WorkbenchEvent` with schema version, event id, sequence, timestamp, actor id, causation id, correlation id, and typed event kind.
- [ ] 3.2 Define task events for started, progress, output appended, yielded, resumed, side-effect planned, side-effect committed, artifact created, evidence attached, completed, failed, and cancelled states.
- [ ] 3.3 Implement in-memory activity ledger replay with no side effects.
- [ ] 3.4 Implement JSONL activity ledger append and replay behind the same ledger trait.
- [ ] 3.5 Implement an in-memory projection store for current objects, buffers, views, tasks, artifacts, evidence, command availability, and dirty-view invalidation.
- [ ] 3.6 Add replay tests proving projections rebuild from ledger state without rerunning shell commands, agent turns, file writes, network calls, terminal input, or imports.

## 4. Semantic View Snapshots

- [ ] 4.1 Define `ViewSnapshot` with view id, buffer id, version, view kind, semantic model, actions, diagnostics, selection, and focus state.
- [ ] 4.2 Define strongly typed built-in view models for conversation, task tree, command palette, form, object list, text document read/review, diff, and log stream.
- [ ] 4.3 Define schema-versioned dynamic extension view payload support using JSON for unknown or domain-specific views.
- [ ] 4.4 Separate semantic view state from host render state in the type model and tests.
- [ ] 4.5 Add snapshot tests for conversation, form, task tree, and command palette models as the first implementation surface.

## 5. Alan Agent Adapter

- [ ] 5.1 Add `workbench-agent` to the workspace with dependencies on `workbench-core` and the Alan protocol/runtime client surfaces needed for adaptation.
- [ ] 5.2 Map Alan session metadata into an agent session object, conversation buffer, and initial conversation view descriptor.
- [ ] 5.3 Register agent commands for submit turn, resume yielded task, interrupt or cancel active work, compact context, and rollback turn history.
- [ ] 5.4 Map `alan_protocol::EventEnvelope` turn, text, thinking, tool, plan, warning, error, and yield events into workbench events and projections.
- [ ] 5.5 Map Alan child-run records or lifecycle events into workbench child task descriptors and task events.
- [ ] 5.6 Project Alan confirmation, structured input, and dynamic tool yields into yielded task state and semantic form or approval snapshots.
- [ ] 5.7 Add fixture tests using representative Alan event envelopes to verify conversation, form, task tree, artifact, and evidence projections.

## 6. Ratatui Renderer Host

- [ ] 6.1 Add `workbench-ratatui` to the workspace as a renderer host and input adapter over `workbench-core`.
- [ ] 6.2 Implement Ratatui renderers for conversation, form, task tree, and command palette semantic snapshots.
- [ ] 6.3 Translate Crossterm key, paste, resize, and mouse events into host-local layout changes, semantic input intents, view-local input, or command invocations.
- [ ] 6.4 Keep renderer-only state such as line wrapping, terminal cell cache, geometry, and frame timing out of workbench semantic state.
- [ ] 6.5 Add Ratatui snapshot or test-backend coverage for the first built-in semantic view renderers.

## 7. Existing TUI Compatibility Integration

- [ ] 7.1 Integrate `workbench-agent` and `workbench-ratatui` into `crates/tui` behind a compatibility-first path while preserving existing daemon wiring.
- [ ] 7.2 Preserve session creation or attach, hydration, replay cursor handling, event stream reconnect, submission, resume, interrupt, compact, rollback, and pending-yield behavior.
- [ ] 7.3 Run the semantic projection path in parallel with the existing reducer where useful and add parity tests before removing old reducer behavior.
- [ ] 7.4 Migrate supported conversation, form, task-tree, and command-palette rendering to semantic snapshots after focused tests pass.
- [ ] 7.5 Leave unsupported surfaces on the current TUI path until a semantic model and renderer are implemented.

## 8. Verification And Review

- [ ] 8.1 Run focused `cargo test` coverage for `workbench-core`, `workbench-agent`, `workbench-ratatui`, and affected `alan-terminal-ui` tests.
- [ ] 8.2 Run `cargo fmt --all` and the relevant clippy or workspace check target, or document any environment-blocked gate with focused passing tests.
- [ ] 8.3 Run `openspec validate introduce-workbench-runtime --strict`.
- [ ] 8.4 Run `git diff --check -- openspec/changes/introduce-workbench-runtime crates`.
- [ ] 8.5 Perform a PR review pass against the workbench contracts, adapter boundaries, replay no-side-effects rule, and existing TUI compatibility.
- [ ] 8.6 After implementation is merged, sync accepted delta specs into `openspec/specs/` and prepare archive-readiness notes before archiving the change.
