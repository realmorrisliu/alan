## Context

`crates/tui` is currently a daemon-backed Ratatui application. It owns terminal
input, creates or attaches to an Alan daemon session, hydrates history, streams
`alan_protocol::EventEnvelope` values, reduces them into transcript state, and
renders the result with Ratatui. That shape is pragmatic for the current Alan
conversation UI, but it is not the right abstraction boundary for the Alan
Alan OS or for non-terminal hosts such as SwiftUI.

Alan already has several related concepts:

- The Agent Execution Engine owns sessions, tape, tool calls, yields, child
  runs, policy, and rollout/event persistence for Alan Agent work.
- The macOS shell owns native spaces, tabs, pane slots, `ContentInstance`
  mounting, and renderer-specific state.
- The Alan product direction names objects, commands,
  buffers, views, queries, humans, agents, and extensions as the durable product
  model.

This change introduces an Alan Kernel runtime contract as the first
implementation slice of Alan Kernel. It is not another TEA framework for
Ratatui, not a rewrite of the Agent Execution Engine, and not middleware whose
job is merely to sit between the existing Agent Execution Engine and Alan TUI.
It is a semantic Alan OS model that Alan Apps, hosts, agents, automation,
and future extensions can share.

## Alan OS Relationship

`introduce-alan-kernel-runtime` is an Alan Kernel implementation
slice under `programmable-environment-product`. It is intentionally
smaller than the full product constitution:

| Constitution criterion | Kernel slice stance |
| --- | --- |
| Object/command/buffer/view/query runtime | Proves the shared substrate model in `alan-kernel`. |
| Humans and agents as actors | Proves actor metadata and the Alan Agent app module/projection layer. |
| Commands as shared invocation model | Proves descriptors and command invocation for UI/agent/control surfaces. |
| Queryable runtime | Proves query descriptors and semantic snapshots at the runtime layer. |
| Local-first native authority | Proves native references and rebuildable projections, but does not build broad local work discovery. |
| Activity ledger and inspectable evidence | Proves task, artifact, evidence, and no-side-effect replay boundaries. |
| Host surfaces do not own runtime truth | Proves Ratatui as renderer host; SwiftUI is deferred. |
| Agent Capability as OS ability | Proves semantic descriptors, Agent Run ids, Context Grant shape, Result Contract shape, command risk, execution guard metadata, evidence, and audit only; Agent Capability Service execution is deferred to Host Service APIs. |
| Out-of-the-box product workflow | Deferred; first slice uses Alan Agent as a built-in Alan App rather than a complete standalone app workflow. |
| WASM extension model | Deferred; dynamic view payloads keep a schema-versioned opening without loading extensions. |
| Alan Apps | Deferred; Groove Master and other apps should adapt through this substrate later. |

This means Alan Kernel can be the first substrate implementation without becoming
the entire Alan product. Alan Agent is the first built-in app projected
onto that substrate, and Alan TUI is the first host/frame that renders it.
Follow-up changes should still prove first-launch local work discovery, a real app
or workspace workflow, SwiftUI host integration, and extension-shaped capabilities.

## Roadmap Position

This change is the first Alan OS spine implementation slice. Its first
priority is Alan Kernel: typed ids, descriptors, native references, command and
query surfaces, tasks, activity ledger, rebuildable projections, semantic view
snapshots, and adapter independence.

The migration order inside and after this change is gated:

1. Finish the Alan Kernel spine before treating app or renderer crates as
   durable API.
2. Add Alan Agent projection through `alan-agent` as the first Alan App after
   the spine is usable. The current Agent Execution Engine remains internal
   authority.
3. Integrate Alan TUI first as the compatibility host over semantic snapshots,
   because it is already a thin daemon-backed client.
4. Migrate Alan for macOS through the same host contract in a follow-up change;
   it is a host, not an Alan App.
5. Leave Groove Master and UPDF for later Alan App changes. Groove Master should
   validate the first domain app path; UPDF should validate complex content,
   document objects, multi-target views, comments, and publishing tasks.

This sequencing keeps the current Kernel slice from becoming a hidden
application framework or a one-off TUI rewrite. Each later migration should use
the same app, command, task, view snapshot, and Host Service API boundaries
rather than bypassing the OS spine.

## Goals / Non-Goals

**Goals:**

- Define the Alan Kernel substrate boundary before implementation starts.
- Make the Alan Kernel boundary traceable to the Alan OS
  constitution without claiming to implement the complete product.
- Keep the core independent from `alan-protocol`, Ratatui, SwiftUI, Tokio
  handles, macOS `ContentInstance`, and any future product name.
- Make commands, queries, subscriptions, actors, tasks, buffers, views,
  artifacts, evidence, event ledgers, and projections first-class runtime
  concepts.
- Define only the semantic Agent Capability primitives that belong in Alan
  Kernel, while keeping Agent Capability Service execution behind Host Service
  APIs.
- Preserve native source-of-truth boundaries for files, Git repositories,
  terminals, agent sessions, domain stores, and external systems.
- Make Ratatui a renderer host and input adapter rather than the application
  runtime.
- Use the existing Alan Agent conversation path as the first built-in Alan App
  vertical slice.
- Keep the migration compatibility-first so the current TUI keeps working while
  semantic projections are introduced.

**Non-Goals:**

- Implement a general-purpose Ratatui component framework.
- Replace the Agent Execution Engine, Host Service APIs, daemon-backed Host
  Service Implementations, or
  `alan-protocol`.
- Implement Agent Capability Service, System Agent Supervisor, provider
  execution, concrete memory storage, sandbox execution, or daemon session
  clients inside the Kernel slice.
- Make the macOS shell `ContentInstance` model the Alan Kernel model.
- Implement a full text editor, full terminal emulator, SwiftUI host, WASM
  component runtime, renderer extensions, or generic object browser breadth in
  the first slice.
- Define a universal URI/resource protocol or private object database.
- Prove first-launch local work discovery or a complete Alan App
  workflow in this slice.

## Decisions

### 1. Use Alan-owned crate and component names

The substrate crate is `alan-kernel`. New app and host work should use
Alan-owned component names rather than temporary incubation namespaces:

| Component or crate | Meaning |
| --- | --- |
| `alan-kernel` | Alan Kernel: objects, commands, buffers, views, queries, actors, ledgers, tasks, artifacts, evidence, and projections. |
| future `alan-agent` | Built-in Alan Agent app module and projection layer. Daemon/protocol adaptation is an implementation detail inside this module. |
| `alan-terminal-ui` or future `alan-terminal-renderer` | Alan TUI renderer/input path. Prefer folding into the existing `alan-terminal-ui`; split only if renderer code needs an independent crate. |
| `alan-runtime` today, future `alan-agent-engine` if renamed | Internal Agent Execution Engine used by Alan Agent. It is not Alan OS or Alan Kernel. |
| `alan-protocol`, or future `alan-agent-protocol` if narrowed | Agent session event/operation protocol. If a broader Alan environment protocol appears, it should be separate. |
| `alan daemon` | Current local Host Service Implementation and HTTP/WS gateway. Do not rename it to the OS boundary; extract service APIs only after their abstract scope is clear. |
| `alan` CLI | Public operator/client entrypoint for configuration, daemon control, local commands, and developer workflows. |

`alan-kernel` currently owns substrate descriptors, events, registries,
projections, and snapshot contracts. It must not depend on `alan-protocol`,
Ratatui, SwiftUI, or Tokio handles. Async implementation details may exist in
app, service, and host adapters, but public core state is model-driven and
executor-neutral.

Before adding the Alan Agent app module or terminal renderer as new crates, the
substrate crate has converged on `alan-kernel`. Future crate names should be
reviewed against this table before they become durable accepted APIs.

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

### 5. Keep Agent Capability execution out of Kernel

Alan Kernel may define Agent Capability descriptors, Agent Run ids, Context
Grant and Result Contract shapes, command risk, Effect Class, Execution Guard
metadata, yielded task states, evidence, and audit records. It must not start,
schedule, stream, yield, or complete concrete agent work by itself.

Those execution concerns belong to Agent Capability Service as a Host Service
API. The first implementation can be a compatibility adapter over the current
Agent Execution Engine and daemon-backed session APIs, but `alan-kernel` must
remain free of provider clients, daemon clients, concrete memory stores,
sandbox execution, and runtime supervision.

Alternative considered: put the Agent Capability Service directly in
`alan-kernel` because Agent Capability is an OS feature. That would make
Alan Kernel depend on the current agent runtime stack and would repeat the
mistake of treating Alan Agent internals as the OS substrate.

### 6. Use an activity ledger plus rebuildable projections

The Alan Kernel ledger records activity: command intent, policy decisions, task
lifecycle, yields, committed side effects, artifacts, evidence, and semantic
buffer/view lifecycle. It is not a universal object store.

Replay must never re-run side effects. Replay rebuilds Alan Kernel state and
refreshes native resources through their owning systems. Projection state is a
cache that can be rebuilt from the ledger plus native resource inspection.

First implementations should include an in-memory ledger and a JSONL ledger.
SQLite or graph storage can be added later if projection performance requires
it.

Alternative considered: store all object data and projection changes directly
in one database. That conflicts with local-first/native-authority boundaries.

### 7. Render semantic snapshots, not render patches

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

### 8. Keep view state semantic and render state host-local

Semantic view state includes selection, focused field, filter text, scroll
anchor, active mode, and other state that another renderer, restore path,
command, query, or agent would care about. Host render state includes measured
line wraps, terminal cell cache, pixel geometry, hover state, and animation
frames.

Alternative considered: keep all state in the renderer host. That would break
restore, SwiftUI parity, and agent inspection.

### 9. Adapt Alan Agent as a built-in Alan App instead of merging it into core

The durable app module should be `alan-agent`, depending on Alan Kernel and the
current agent protocol plus Host Service API surfaces it needs. The module is
the Alan Agent built-in app and projection layer: it maps existing
Alan session metadata, event envelopes, yields, tool lifecycle events,
child-run records, and operations into Alan Kernel objects, buffers, commands,
task events, forms, artifacts, and evidence.

The current Agent Execution Engine and daemon-backed Host Service Implementation
remain internal implementation details for the Alan Agent app during migration.
Alan Kernel does not learn about tape, provider continuation, compaction
internals, daemon endpoints, or tool orchestration.

Alternative considered: fold the Agent Execution Engine into Alan Kernel. That would
make non-agent workflows inherit agent-specific concepts and increase migration
risk. Another rejected shape is treating Alan Kernel as middleware between the
Agent Execution Engine and TUI; that would keep Alan Agent as the substrate
instead of making it an app running inside the environment.

### 10. Treat Ratatui and SwiftUI as renderer hosts

Ratatui should translate crossterm input into semantic input intents or command
invocations and render semantic snapshots. SwiftUI can do the same later with
native events and views. Hosts own physical layout; the Alan Kernel owns semantic
open buffers, open views, active view, task state, and command/query surfaces.

Alternative considered: make Ratatui the primary app framework and port other
hosts later. That would preserve the current terminal bias and undercut the
Alan OS boundary.

## Risks / Trade-offs

- [Risk] The Alan Kernel contract becomes too broad to implement. -> Keep the
  first implementation to an Alan Agent conversation vertical slice with
  conversation, form/yield, task tree, and command invocation.
- [Risk] The semantic model duplicates existing agent execution events. -> Keep
  agent events in the Alan Agent app module as implementation input; do not
  expose them from Alan Kernel.
- [Risk] A new core crate could drift into a private object store. -> Keep
  native resources authoritative and make projections rebuildable caches.
- [Risk] Renderer hosts may need capabilities not covered by the first snapshot
  model. -> Add strongly typed built-in view models only when a real host or
  workflow needs them; keep extension views schema-versioned.
- [Risk] Compatibility work doubles code temporarily. -> Use a parallel
  semantic path first, then retire old reducer/rendering code only after tests
  prove parity.

## Migration Plan

1. Add `alan-kernel` with descriptor, event, registry, ledger, projection,
   task, artifact, evidence, and view snapshot skeletons.
2. Use the Agent Capability migration map to decide which current Alan Agent
   concepts become Kernel semantic primitives, Host Service API concerns, Alan
   Agent App features, compatibility paths, or rewrite candidates.
3. Add `alan-agent` fixtures that map representative Alan event envelopes from
   the current execution engine and daemon-backed Host Service Implementation
   into the built-in Alan Agent
   app's task events and conversation/form/task-tree snapshots.
4. Add Agent Capability compatibility fixtures only after the Host Service API
   boundary is split, proving Context Grants and Result Contracts can wrap the
   current Agent Execution Engine without moving execution into Kernel.
5. Add Alan TUI renderers for conversation, form, task tree, and command palette
   snapshots inside `alan-terminal-ui`, splitting `alan-terminal-renderer` only
   if an independent crate is justified.
6. Integrate the semantic path into `crates/tui` behind a compatibility-first
   path while preserving daemon creation, hydration, reconnect, submission, and
   pending-yield behavior.
7. Replace the old TUI reducer/rendering pieces only after focused tests cover
   the semantic projection and Ratatui output.

Rollback for the first slice is removing the new substrate/app/renderer crates
and reverting the optional TUI integration path; existing daemon and agent
execution behavior remain untouched.

## Open Questions

- Whether the first command registry implementation should live entirely in the
  Alan Kernel or split descriptor storage from app-owned execution.
- Which JSON schema representation should be used for extension view payloads
  and command/query args before a WASM runtime exists.
- How much of the current TUI history/composer behavior should move into
  semantic view state versus remain host-local during the first slice.
