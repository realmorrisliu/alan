## ADDED Requirements

### Requirement: Alan Kernel is the first OS spine slice
The Alan Kernel implementation SHALL be treated as an OS spine slice under
`programmable-environment-product`, not as the complete Alan product and not as
middleware between the current Agent Execution Engine and host surfaces.

#### Scenario: Alan Kernel scope is reviewed
- **WHEN** an Alan Kernel implementation or architecture change is reviewed
- **THEN** it identifies the programmable-environment constitution criteria it
  proves, such as objects, commands, buffers, views, queries, actors, ledgers,
  tasks, artifacts, evidence, native references, and host snapshots
- **AND** it explicitly defers complete product concerns such as broad
  first-launch local work discovery, complete Alan Apps, SwiftUI hosting,
  WASM extension loading, and universal resource addressing unless those scopes
  are added by separate future changes
- **AND** it identifies Alan Agent as a built-in Alan App and agent actor projected
  onto the substrate, rather than treating the Agent Execution Engine as the substrate itself

#### Scenario: Roadmap position is reviewed
- **WHEN** Alan Kernel implementation is sequenced against the Alan OS roadmap
- **THEN** the Alan Kernel is treated as the first OS spine slice
- **AND** Alan Agent app projection, Alan TUI host integration, Alan for macOS
  host migration, Groove Master, and UPDF remain gated on the Kernel contracts
  becoming usable enough for real app and host integration

### Requirement: Alan Kernel models Agent Capability semantics only
The Alan Kernel SHALL be allowed to model Agent Capability semantic
primitives that belong in Alan Kernel, including Agent Capability descriptors,
Agent Run identity, Context Grant shape, Result Contract shape, command risk,
effect classes, execution guard metadata, yielded task state, evidence, and
audit. It SHALL NOT implement Agent Capability Service execution.

#### Scenario: Agent Capability primitives are added
- **WHEN** Alan Kernel adds Agent Capability-related types
- **THEN** those types describe semantic identity, descriptors, context,
  results, task state, command governance metadata, evidence, or audit
- **AND** they do not start, schedule, stream, yield, resume, or complete
  concrete model/provider execution

#### Scenario: Kernel dependencies are inspected for agent execution
- **WHEN** `alan-kernel` dependencies are reviewed
- **THEN** they do not include provider clients, daemon session clients,
  concrete LLM runtime dependencies, memory storage backends, sandbox execution
  backends, or runtime supervision handles for Agent Capability execution
- **AND** those implementation concerns remain in Agent Capability Service,
  Host Service APIs, Host Service Implementations, or Alan Agent app adapters

### Requirement: Crate names converge on Alan-owned names
Crate names SHALL use Alan-owned names before the Alan Kernel surface becomes a
durable accepted API.

The durable crate naming direction SHALL be:

- `alan-kernel` for the substrate crate/package;
- future `alan-agent` for the built-in Alan Agent app module;
- `alan-terminal-ui`, or `alan-terminal-renderer` only when an independent
  renderer crate is justified, for terminal renderer work;
- `alan-agent-engine` if the current `alan-runtime` crate is renamed;
- `alan-agent-protocol` if `alan-protocol` remains specific to agent session
  events and operations.

#### Scenario: Durable API is prepared
- **WHEN** the Alan Kernel substrate contract is promoted from incubation toward a
  durable accepted API
- **THEN** `alan-kernel` is the substrate crate/package name
- **AND** new app or terminal renderer crates use Alan-owned names rather than
  adding temporary incubation namespaces

### Requirement: Alan Kernel remains adapter independent
The Alan Kernel SHALL define renderer-independent and agent-protocol-
independent runtime primitives for Alan Kernel behavior.

#### Scenario: Core dependencies are inspected
- **WHEN** the Alan Kernel crate is built or audited
- **THEN** it has no dependency on `alan-protocol`, Ratatui, Crossterm, SwiftUI,
  AppKit, macOS shell `ContentInstance` types, or Tokio task handles
- **AND** adapter crates own dependencies on those systems

#### Scenario: App, execution, or host-specific event is handled
- **WHEN** an Alan Agent execution event, terminal host event, SwiftUI event, or macOS
  shell content event enters the system
- **THEN** it is translated by an app, backend, or host adapter before it affects
  Alan Kernel state

### Requirement: Runtime entities use typed opaque identity
The Alan Kernel SHALL identify runtime entities with typed opaque ids and
SHALL represent external source-of-truth resources through native references.

#### Scenario: File is represented as an object
- **WHEN** a local file is represented in the Alan Kernel
- **THEN** the Alan Kernel object has a typed object id
- **AND** its descriptor records a native file reference rather than requiring a
  universal Alan Kernel URI or private object-store copy

#### Scenario: Alan Agent session is represented as an object
- **WHEN** an Alan Agent session is represented in the Alan Kernel
- **THEN** the Alan Kernel object has a typed object id
- **AND** its descriptor records the adapter-owned session reference as native
  authority

### Requirement: Objects, buffers, and views are distinct
The Alan Kernel SHALL separate inspectable resources, active work contexts,
and presentations.

#### Scenario: Object is opened for work
- **WHEN** a command opens an object
- **THEN** the Alan Kernel creates or resolves a buffer as the active work context
- **AND** it creates or resolves a view snapshot for presentation without making
  the object itself own presentation state

#### Scenario: Query result is opened
- **WHEN** a query result is opened for inspection
- **THEN** the Alan Kernel can represent it as a buffer even when no external
  object owns the result data

### Requirement: Commands mutate, queries read, subscriptions observe
The Alan Kernel SHALL expose commands for mutation or work initiation,
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
- **THEN** the Alan Kernel answers through a query or snapshot
- **AND** the operation does not mutate runtime or native resource state

#### Scenario: View observes changes
- **WHEN** a view depends on an object, buffer, task, or query result
- **THEN** a subscription can notify the renderer host that the view is dirty or
  updated without owning business logic

### Requirement: Command descriptors are shared invocation contracts
The Alan Kernel SHALL describe commands through queryable descriptors used by
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
The Alan Kernel SHALL record actor, causation, and correlation metadata for
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
The Alan Kernel SHALL model command execution and long-running work as tasks
with lifecycle, hierarchy, cancellation, yields, output, artifacts, and
evidence.

#### Scenario: Command starts asynchronous work
- **WHEN** a command invocation starts work that may stream, block, yield, or
  produce outputs
- **THEN** the Alan Kernel creates a task descriptor and task events for the
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
The Alan Kernel SHALL distinguish command intent, policy decisions, task
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
The Alan Kernel SHALL replay activity ledgers only to rebuild Alan Kernel state
and SHALL NOT re-execute external side effects during replay.

#### Scenario: Activity ledger is replayed
- **WHEN** an Alan Kernel ledger is replayed after restart
- **THEN** replay rebuilds tasks, buffers, views, artifacts, evidence, and
  projection indexes
- **AND** it does not rerun shell commands, resubmit agent turns, rewrite files,
  send terminal input, call networks, or restart imports

#### Scenario: Native resource state is stale
- **WHEN** a replayed projection references a native resource whose current
  state may have changed
- **THEN** the Alan Kernel refreshes or marks the resource through its native
  authority path rather than trusting replay to recreate external state

### Requirement: Projections are rebuildable caches
The Alan Kernel SHALL treat projection state as rebuildable cache derived
from ledgers and native resource inspection.

#### Scenario: Projection cache is missing
- **WHEN** a projection cache is absent or invalid
- **THEN** the Alan Kernel can rebuild semantic task, object, buffer, view,
  artifact, evidence, and command-availability state from authoritative ledger
  events and native resource inspection

#### Scenario: Projection checkpoint exists
- **WHEN** a projection checkpoint is used to accelerate startup
- **THEN** it is identified as a cache with source event metadata
- **AND** it does not replace the activity ledger as authority

### Requirement: Semantic view snapshots are typed
The Alan Kernel SHALL expose renderer-independent semantic view snapshots
with strongly typed built-in view models and schema-versioned dynamic extension
payloads.

#### Scenario: Built-in conversation view is requested
- **WHEN** a renderer host requests a conversation view snapshot
- **THEN** the Alan Kernel returns a typed conversation model rather than raw
  terminal lines or renderer-specific widgets

#### Scenario: Extension view is requested
- **WHEN** a view is provided by an extension or domain-specific adapter outside
  the built-in set
- **THEN** the Alan Kernel returns a schema id, schema version, and JSON payload
  that renderer hosts can render with a fallback or future host-specific
  adapter

### Requirement: Artifacts and evidence preserve provenance
The Alan Kernel SHALL model produced results as artifacts and supporting
observations as evidence.

#### Scenario: Task produces a patch
- **WHEN** a task produces a patch, report, screenshot, search result, or other
  result
- **THEN** the Alan Kernel records an artifact linked to the producing task and
  available object or buffer references

#### Scenario: Task result is justified
- **WHEN** a task claims success, failure, or partial completion
- **THEN** the Alan Kernel can attach evidence such as command status, file hash,
  event id, approval decision, screenshot capture, or external response
  metadata
