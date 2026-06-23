## ADDED Requirements

### Requirement: Workbench is a runtime-substrate incubation slice
The workbench core SHALL be treated as an environment-core runtime substrate
slice under `programmable-environment-product`, not as the complete
programmable environment product.

#### Scenario: Workbench scope is reviewed
- **WHEN** a workbench implementation or architecture change is reviewed
- **THEN** it identifies the programmable-environment constitution criteria it
  proves, such as objects, commands, buffers, views, queries, actors, ledgers,
  tasks, artifacts, evidence, native references, and host snapshots
- **AND** it explicitly defers complete product concerns such as broad
  first-launch local work discovery, complete environment apps, SwiftUI hosting,
  WASM extension loading, and universal resource addressing unless those scopes
  are added by separate future changes

### Requirement: Workbench core remains adapter independent
The workbench core SHALL define renderer-independent and agent-protocol-
independent runtime primitives for workbench behavior.

#### Scenario: Core dependencies are inspected
- **WHEN** the workbench core crate is built or audited
- **THEN** it has no dependency on `alan-protocol`, Ratatui, Crossterm, SwiftUI,
  AppKit, macOS shell `ContentInstance` types, or Tokio task handles
- **AND** adapter crates own dependencies on those systems

#### Scenario: Adapter-specific event is handled
- **WHEN** an Alan agent event, terminal host event, SwiftUI event, or macOS
  shell content event enters the system
- **THEN** it is translated by an adapter before it affects workbench core state

### Requirement: Runtime entities use typed opaque identity
The workbench core SHALL identify runtime entities with typed opaque ids and
SHALL represent external source-of-truth resources through native references.

#### Scenario: File is represented as an object
- **WHEN** a local file is represented in the workbench
- **THEN** the workbench object has a typed object id
- **AND** its descriptor records a native file reference rather than requiring a
  universal workbench URI or private object-store copy

#### Scenario: Agent session is represented as an object
- **WHEN** an Alan agent session is represented in the workbench
- **THEN** the workbench object has a typed object id
- **AND** its descriptor records the adapter-owned session reference as native
  authority

### Requirement: Objects, buffers, and views are distinct
The workbench core SHALL separate inspectable resources, active work contexts,
and presentations.

#### Scenario: Object is opened for work
- **WHEN** a command opens an object
- **THEN** the workbench creates or resolves a buffer as the active work context
- **AND** it creates or resolves a view snapshot for presentation without making
  the object itself own presentation state

#### Scenario: Query result is opened
- **WHEN** a query result is opened for inspection
- **THEN** the workbench can represent it as a buffer even when no external
  object owns the result data

### Requirement: Commands mutate, queries read, subscriptions observe
The workbench core SHALL expose commands for mutation or work initiation,
queries for read-only semantic inspection, and subscriptions for observing
changes.

#### Scenario: Mutating action is requested
- **WHEN** a human, agent, extension, or system actor requests a mutation or
  side effect
- **THEN** the request is represented as a command invocation
- **AND** it is not performed through a query or subscription path

#### Scenario: Semantic state is inspected
- **WHEN** a client asks for open buffers, available commands, blocked tasks, or
  artifacts
- **THEN** the workbench answers through a query or snapshot
- **AND** the operation does not mutate runtime or native resource state

#### Scenario: View observes changes
- **WHEN** a view depends on an object, buffer, task, or query result
- **THEN** a subscription can notify the renderer host that the view is dirty or
  updated without owning business logic

### Requirement: Command descriptors are shared invocation contracts
The workbench core SHALL describe commands through queryable descriptors used by
UI controls, command palettes, modal grammar, automation, and agent projections.

#### Scenario: Command appears in multiple surfaces
- **WHEN** a command is exposed through a UI button, command palette, modal
  grammar, automation, or agent-facing schema
- **THEN** each surface preserves the same command identity, target semantics,
  argument schema, permission metadata, and audit behavior

#### Scenario: Command availability is computed
- **WHEN** a renderer host or agent asks what actions are available for a target
- **THEN** the answer is derived from command descriptors, target state, actor
  authority, and policy decisions rather than hidden view-local callbacks

### Requirement: Actor and causation metadata are first-class
The workbench core SHALL record actor, causation, and correlation metadata for
commands, tasks, artifacts, evidence, and activity events.

#### Scenario: Agent invokes a command
- **WHEN** an agent invokes a command
- **THEN** the command invocation identifies the agent actor
- **AND** subsequent task events, artifacts, and evidence remain linked through
  causation or correlation ids

#### Scenario: Human action triggers agent work
- **WHEN** a human action causes an agent task that creates tool tasks and
  artifacts
- **THEN** the task tree can preserve the human-originating correlation while
  identifying the agent and tool actors that performed each step

### Requirement: Tasks are first-class runtime state
The workbench core SHALL model command execution and long-running work as tasks
with lifecycle, hierarchy, cancellation, yields, output, artifacts, and
evidence.

#### Scenario: Command starts asynchronous work
- **WHEN** a command invocation starts work that may stream, block, yield, or
  produce outputs
- **THEN** the workbench creates a task descriptor and task events for the
  lifecycle

#### Scenario: Work has nested activity
- **WHEN** a task launches tool calls, child runs, shell commands, or extension
  work
- **THEN** child tasks preserve parent task links and can be rendered as a task
  tree

#### Scenario: Task waits for input
- **WHEN** running work needs human, agent, or extension input before continuing
- **THEN** the task emits a yielded state with a resumable request
- **AND** presentation layers can project that yield as a form or approval
  surface

### Requirement: Activity ledger records intent and committed effects
The workbench core SHALL distinguish command intent, policy decisions, task
lifecycle, yielded checkpoints, planned side effects, committed side effects,
artifacts, and evidence.

#### Scenario: Command is denied
- **WHEN** a command invocation is denied by policy
- **THEN** the ledger records the invocation and decision
- **AND** no committed side-effect event is recorded

#### Scenario: External mutation succeeds
- **WHEN** a command performs an external mutation such as writing a file
- **THEN** the ledger records the committed side effect only after the mutation
  succeeds
- **AND** evidence identifies the native resource state that was observed or
  changed

### Requirement: Ledger replay has no side effects
The workbench core SHALL replay activity ledgers only to rebuild workbench state
and SHALL NOT re-execute external side effects during replay.

#### Scenario: Activity ledger is replayed
- **WHEN** a workbench ledger is replayed after restart
- **THEN** replay rebuilds tasks, buffers, views, artifacts, evidence, and
  projection indexes
- **AND** it does not rerun shell commands, resubmit agent turns, rewrite files,
  send terminal input, call networks, or restart imports

#### Scenario: Native resource state is stale
- **WHEN** a replayed projection references a native resource whose current
  state may have changed
- **THEN** the workbench refreshes or marks the resource through its native
  authority path rather than trusting replay to recreate external state

### Requirement: Projections are rebuildable caches
The workbench core SHALL treat projection state as rebuildable cache derived
from ledgers and native resource inspection.

#### Scenario: Projection cache is missing
- **WHEN** a projection cache is absent or invalid
- **THEN** the workbench can rebuild semantic task, object, buffer, view,
  artifact, evidence, and command-availability state from authoritative ledger
  events and native resource inspection

#### Scenario: Projection checkpoint exists
- **WHEN** a projection checkpoint is used to accelerate startup
- **THEN** it is identified as a cache with source event metadata
- **AND** it does not replace the activity ledger as authority

### Requirement: Semantic view snapshots are typed
The workbench core SHALL expose renderer-independent semantic view snapshots
with strongly typed built-in view models and schema-versioned dynamic extension
payloads.

#### Scenario: Built-in conversation view is requested
- **WHEN** a renderer host requests a conversation view snapshot
- **THEN** the workbench returns a typed conversation model rather than raw
  terminal lines or renderer-specific widgets

#### Scenario: Extension view is requested
- **WHEN** a view is provided by an extension or domain-specific adapter outside
  the built-in set
- **THEN** the workbench returns a schema id, schema version, and JSON payload
  that renderer hosts can render with a fallback or future host-specific
  adapter

### Requirement: Artifacts and evidence preserve provenance
The workbench core SHALL model produced results as artifacts and supporting
observations as evidence.

#### Scenario: Task produces a patch
- **WHEN** a task produces a patch, report, screenshot, search result, or other
  result
- **THEN** the workbench records an artifact linked to the producing task and
  available object or buffer references

#### Scenario: Task result is justified
- **WHEN** a task claims success, failure, or partial completion
- **THEN** the workbench can attach evidence such as command status, file hash,
  event id, approval decision, screenshot capture, or external response
  metadata
