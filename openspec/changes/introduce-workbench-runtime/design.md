## Context

`crates/tui` is currently a daemon-backed Ratatui application. It owns terminal
input, creates or attaches to an Alan daemon session, hydrates history, streams
`alan_protocol::EventEnvelope` values, reduces them into transcript state, and
renders the result with Ratatui. That shape is pragmatic for the current Alan
conversation UI, but it is not the right abstraction boundary for a future
programmable environment or for non-terminal hosts such as SwiftUI.

Alan already has several related concepts:

- The agent runtime owns sessions, tape, tool calls, yields, child runs, policy,
  and rollout/event persistence.
- The macOS shell owns native spaces, tabs, pane slots, `ContentInstance`
  mounting, and renderer-specific state.
- The programmable environment product direction names objects, commands,
  buffers, views, queries, humans, agents, and extensions as the durable product
  model.

This change introduces a workbench runtime contract that sits between those
systems. It is not another TEA framework for Ratatui, and it is not a rewrite of
the Alan agent runtime. It is a semantic workbench model that can be consumed by
Ratatui, SwiftUI, agents, automation, and future extensions.

## Programmable Environment Relationship

`introduce-workbench-runtime` is an environment-core/runtime-substrate
incubation slice under `programmable-environment-product`. It is intentionally
smaller than the full product constitution:

| Constitution criterion | Workbench slice stance |
| --- | --- |
| Object/command/buffer/view/query runtime | Proves the shared semantic model in `workbench-core`. |
| Humans and agents as actors | Proves actor metadata and Alan agent adaptation. |
| Commands as shared invocation model | Proves descriptors and command invocation for UI/agent/control surfaces. |
| Queryable runtime | Proves query descriptors and semantic snapshots at the runtime layer. |
| Local-first native authority | Proves native references and rebuildable projections, but does not build broad local work discovery. |
| Activity ledger and inspectable evidence | Proves task, artifact, evidence, and no-side-effect replay boundaries. |
| Host surfaces do not own runtime truth | Proves Ratatui as renderer host; SwiftUI is deferred. |
| Out-of-the-box product workflow | Deferred; first slice uses Alan agent conversation rather than a complete product/app workflow. |
| WASM extension model | Deferred; dynamic view payloads keep a schema-versioned opening without loading extensions. |
| Environment apps | Deferred; Groove Master and other apps should adapt through this substrate later. |

This means workbench can be the first substrate implementation without becoming
the entire programmable environment. Follow-up changes should still prove
first-launch local work discovery, a real app or workspace workflow, SwiftUI host
integration, and extension-shaped capabilities.

## Goals / Non-Goals

**Goals:**

- Define the workbench runtime boundary before implementation starts.
- Make the workbench boundary traceable to the programmable environment
  constitution without claiming to implement the complete product.
- Keep the core independent from `alan-protocol`, Ratatui, SwiftUI, Tokio
  handles, macOS `ContentInstance`, and any future product name.
- Make commands, queries, subscriptions, actors, tasks, buffers, views,
  artifacts, evidence, event ledgers, and projections first-class runtime
  concepts.
- Preserve native source-of-truth boundaries for files, Git repositories,
  terminals, agent sessions, domain stores, and external systems.
- Make Ratatui a renderer host and input adapter rather than the application
  runtime.
- Use the existing Alan agent conversation path as the first vertical slice.
- Keep the migration compatibility-first so the current TUI keeps working while
  semantic projections are introduced.

**Non-Goals:**

- Implement a general-purpose Ratatui component framework.
- Replace the Alan agent runtime, daemon APIs, or `alan-protocol`.
- Make the macOS shell `ContentInstance` model the workbench core model.
- Implement a full text editor, full terminal emulator, SwiftUI host, WASM
  component runtime, renderer extensions, or generic object browser breadth in
  the first slice.
- Define a universal URI/resource protocol or private object database.
- Prove first-launch local work discovery or a complete environment-app
  workflow in this slice.

## Decisions

### 1. Use `workbench-*` crates and keep the core renderer-independent

The implementation should start with `workbench-core`, `workbench-agent`, and
`workbench-ratatui`.

`workbench-core` owns semantic descriptors, events, registries, projections, and
snapshot contracts. It must not depend on `alan-protocol`, Ratatui, SwiftUI, or
Tokio handles. Async implementation details may exist in adapters, but public
core state is model-driven and executor-neutral.

Alternative considered: put the abstraction directly inside `crates/tui`. That
would be faster initially but would bias the model toward daemon session events,
Ratatui frames, terminal input, and the current transcript reducer.

### 2. Treat objects, buffers, and views as separate concepts

Objects represent inspectable runtime resources with identity, metadata,
capabilities, and native authority references. Buffers represent active work
contexts over objects, tasks, query results, or domain state. Views represent a
presentation of a buffer.

The core should use typed opaque ids such as `ObjectId`, `BufferId`, `ViewId`,
`TaskId`, and `CommandId`. External authority stays in `native_ref` fields. For
example, a file object references a path, a Git object references a worktree and
revision, an agent object references a session id, and a domain app object
references app-owned state.

Alternative considered: introduce a universal URI model. That is too early and
would obscure the simpler local-first boundary.

### 3. Make commands, queries, and subscriptions distinct

Commands mutate or initiate work. Queries read semantic state. Subscriptions
observe changes and invalidate or update views. Mutation must not happen through
queries or subscriptions.

Command descriptors are shared by UI controls, command palette, modal grammar,
automation, and agent tool projections. Permission and audit metadata belong on
commands; tools are only one possible execution backend.

Alternative considered: keep command handling as Rust callbacks owned by each
view. That would hide actions from agents, command palettes, modal grammar, and
audit surfaces.

### 4. Model actors and causation from the start

Every command, task event, artifact, and evidence record should carry actor and
causation metadata. Actor kinds include humans, agents, extensions, and system
actors. Causation and correlation ids connect chains such as user input, command
invocation, agent turn, tool task, child run, artifact, and evidence.

Alternative considered: add actor/audit metadata later. That would make early
events and tasks ambiguous and would weaken the "humans and agents share the
same runtime" premise.

### 5. Use an activity ledger plus rebuildable projections

The workbench ledger records activity: command intent, policy decisions, task
lifecycle, yields, committed side effects, artifacts, evidence, and semantic
buffer/view lifecycle. It is not a universal object store.

Replay must never re-run side effects. Replay rebuilds workbench state and
refreshes native resources through their owning systems. Projection state is a
cache that can be rebuilt from the ledger plus native resource inspection.

First implementations should include an in-memory ledger and a JSONL ledger.
SQLite or graph storage can be added later if projection performance requires
it.

Alternative considered: store all object data and projection changes directly
in one database. That conflicts with local-first/native-authority boundaries.

### 6. Render semantic snapshots, not render patches

Renderer hosts should consume `ViewSnapshot` values. The core marks views dirty
through subscription updates; the host pulls a semantic snapshot and renders it
using its own layout/cache/diff strategy.

The core should define strongly typed built-in view models for the first slice:
conversation, task tree, object list, command palette, form, text document
read/review, diff, and log stream. Dynamic extension views may use schema id,
version, and JSON payloads. The first slice should not promise a full terminal
semantic renderer.

Alternative considered: emit render patches or a generic UI element tree. That
would become a UI toolkit and make Ratatui/SwiftUI/agent inspection harder to
keep aligned.

### 7. Keep view state semantic and render state host-local

Semantic view state includes selection, focused field, filter text, scroll
anchor, active mode, and other state that another renderer, restore path,
command, query, or agent would care about. Host render state includes measured
line wraps, terminal cell cache, pixel geometry, hover state, and animation
frames.

Alternative considered: keep all state in the renderer host. That would break
restore, SwiftUI parity, and agent inspection.

### 8. Adapt Alan agent sessions instead of merging them into core

`workbench-agent` should depend on both `workbench-core` and `alan-protocol`.
It maps Alan session metadata, event envelopes, yields, tool lifecycle events,
child-run records, and operations into workbench objects, buffers, commands,
task events, forms, artifacts, and evidence.

The agent runtime remains the execution backend for agent work. The workbench
core does not learn about tape, provider continuation, compaction internals, or
tool orchestration.

Alternative considered: fold the agent runtime into workbench core. That would
make non-agent workflows inherit agent-specific concepts and increase migration
risk.

### 9. Treat Ratatui and SwiftUI as renderer hosts

Ratatui should translate crossterm input into semantic input intents or command
invocations and render semantic snapshots. SwiftUI can do the same later with
native events and views. Hosts own physical layout; the workbench owns semantic
open buffers, open views, active view, task state, and command/query surfaces.

Alternative considered: make Ratatui the primary app framework and port other
hosts later. That would preserve the current terminal bias and undercut the
programmable environment boundary.

## Risks / Trade-offs

- [Risk] The workbench contract becomes too broad to implement. -> Keep the
  first implementation to an Alan agent conversation vertical slice with
  conversation, form/yield, task tree, and command invocation.
- [Risk] The semantic model duplicates existing agent runtime events. -> Keep
  agent events in `workbench-agent` as adapter input; do not expose them from
  `workbench-core`.
- [Risk] A new core crate could drift into a private object store. -> Keep
  native resources authoritative and make projections rebuildable caches.
- [Risk] Renderer hosts may need capabilities not covered by the first snapshot
  model. -> Add strongly typed built-in view models only when a real host or
  workflow needs them; keep extension views schema-versioned.
- [Risk] Compatibility work doubles code temporarily. -> Use a parallel
  semantic path first, then retire old reducer/rendering code only after tests
  prove parity.

## Migration Plan

1. Add `workbench-core` with descriptor, event, registry, ledger, projection,
   task, artifact, evidence, and view snapshot skeletons.
2. Add `workbench-agent` fixtures that map representative Alan event envelopes
   into workbench task events and conversation/form/task-tree snapshots.
3. Add `workbench-ratatui` renderers for conversation, form, task tree, and
   command palette snapshots.
4. Integrate the semantic path into `crates/tui` behind a compatibility-first
   path while preserving daemon creation, hydration, reconnect, submission, and
   pending-yield behavior.
5. Replace the old TUI reducer/rendering pieces only after focused tests cover
   the semantic projection and Ratatui output.

Rollback for the first slice is removing the new workbench crates and reverting
the optional TUI integration path; existing daemon and agent runtime behavior
remain untouched.

## Open Questions

- Whether the first command registry implementation should live entirely in
  `workbench-core` or split descriptor storage from adapter-owned execution.
- Which JSON schema representation should be used for extension view payloads
  and command/query args before a WASM runtime exists.
- How much of the current TUI history/composer behavior should move into
  semantic view state versus remain host-local during the first slice.
