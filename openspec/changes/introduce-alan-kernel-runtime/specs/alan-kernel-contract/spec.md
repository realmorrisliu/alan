## ADDED Requirements

### Requirement: Alan Kernel is the first OS spine slice
The Alan Kernel implementation SHALL be treated as an OS spine slice under
`programmable-environment-product`, not as the complete Alan product and not as
middleware between the current Agent Execution Engine and host surfaces.

#### Scenario: Alan Kernel scope is reviewed
- **WHEN** an Alan Kernel implementation or architecture change is reviewed
- **THEN** it identifies the programmable-environment constitution criteria it
  proves, such as namespace and mounts, paths, files, descriptors, access
  rights, credentials, processes, the process table, stream file kinds, native
  references, and host snapshots
- **AND** it explicitly defers complete product concerns such as broad
  first-launch local work discovery, complete Alan Apps, SwiftUI hosting,
  WASM extension loading, and universal resource addressing unless those scopes
  are added by separate future changes
- **AND** it identifies Alan Agent as a built-in Alan App and agent actor projected
  onto the substrate, rather than treating the Agent Execution Engine as the substrate itself

#### Scenario: Roadmap position is reviewed
- **WHEN** Alan Kernel implementation is sequenced against the Alan OS roadmap
- **THEN** the Alan Kernel is treated as the first OS spine slice
- **AND** Alan Agent app projection, Alan Shell host integration, Alan for macOS
  host migration, Groove Master, and UPDF remain gated on the Kernel contracts
  becoming usable enough for real app and host integration

### Requirement: Alan Kernel follows semantic UNIX primitives
The Alan Kernel SHALL keep its durable ontology close to a semantic UNIX model:
namespaces, mounts, paths, files, descriptors, access rights, credentials,
processes, and the process table. Streams SHALL be modeled as file kinds, and
process output SHALL be modeled as ordinary files or stream files. Higher-level
Alan terms such as object, buffer, view, task, command, query, subscription,
artifact, evidence, and semantic snapshot SHALL be
expressible as app/service/host descriptors or conventions over those
primitives rather than durable Kernel primitives.

Agent/App artifacts SHALL be modeled above Kernel as an interpretation over
files produced by processes, stream files, output pointers, or native selectors
rather than as a separate Kernel primitive.

Agent/App evidence SHALL be modeled above Kernel as an interpretation over
paths, stream offsets, process-table entries, descriptors, service-owned stream
file offsets, app artifact paths, or native selectors rather than as a separate
Kernel primitive.

#### Scenario: Kernel ontology is reviewed
- **WHEN** Alan Kernel concepts are promoted from incubation toward durable API
- **THEN** each Kernel concept can be explained as a namespace, mount,
  path, file, descriptor, access-right value, credential, process, Agent
  Process, or
  process-table entry
- **AND** the Kernel does not introduce a separate private object-store ontology
  when a file or stream-file representation would preserve composability

#### Scenario: V1 typed names are retained
- **WHEN** V1 code or specs use names such as object, task, view, command,
  query, subscription, compatibility artifact surface, or compatibility evidence
  surface
- **THEN** object/task/view/command/query/subscription names are treated
  as typed app/service/host surfaces for compatibility and inspection
- **AND** compatibility artifact and evidence names are treated as Agent/App
  adapter surfaces over Kernel files, process ids, descriptors, offsets, and
  pointers rather than durable
  Kernel ontology
- **AND** they do not supersede the smaller namespace, path, file, descriptor,
  access-right, credential, process, and process-table model

### Requirement: Alan Kernel models Agent Process anchors only
The Alan Kernel SHALL model Agent Process anchors that belong in Kernel,
including process identity, parentage, Credentials, Paths, Files, stream Files,
Descriptors, Access Rights, namespaces/mounts, `/proc`, `/srv`, Access Checks,
and process-table state. AgentFS schemas, tape, model calls, request files,
action files, Tool manifests, Skill packages, memory storage, policy
evaluation, and Agent Execution Guard metadata SHALL belong to Agent Runtime
Service, Agent/App layers, or concrete file-server services rather than Alan
Kernel. Alan Kernel SHALL NOT implement agent model/provider execution.

#### Scenario: Agent Process primitives are added
- **WHEN** Alan Kernel adds agent-related types
- **THEN** those types describe only Kernel anchors such as Agent Process
  identity, credentials, paths, files, stream files, descriptors, access rights,
  Access Checks, `/proc`, `/srv`, or process-table compatibility state
- **AND** they do not model durable AgentFS request/action/tape schemas
- **AND** they do not model durable Tool manifests, Skill packages, memory
  stores, or agent policy schemas
- **AND** they do not start, schedule, stream, yield, resume, or complete
  concrete model/provider execution

#### Scenario: Kernel dependencies are inspected for agent execution
- **WHEN** `alan-kernel` dependencies are reviewed
- **THEN** they do not include provider clients, compatibility session clients,
  concrete LLM runtime dependencies, memory storage backends, sandbox execution
  backends, or runtime supervision handles for Agent Process execution
- **AND** those implementation concerns remain in Agent Runtime Service,
  file-server services, compatibility transports, or optional Alan Agent
  workspace adapters

### Requirement: Crate names converge on Alan-owned names
Crate names SHALL use Alan-owned names before the Alan Kernel surface becomes a
durable accepted API.

The durable crate naming direction SHALL be:

- `alan-kernel` for the substrate crate/package;
- future `alan-agent` for the built-in Alan Agent app module;
- future `alan-shell` for the Alan Shell interaction path, with current
  implementation work remaining in `crates/tui` during migration;
- `alan-agent-engine` if the current `alan-runtime` crate is renamed;
- `alan-agent-protocol` if `alan-protocol` remains specific to agent session
  events and operations.

#### Scenario: Durable API is prepared
- **WHEN** the Alan Kernel substrate contract is promoted from incubation toward a
  durable accepted API
- **THEN** `alan-kernel` is the substrate crate/package name
- **AND** new app or shell renderer crates use Alan-owned names rather than
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

### Requirement: Kernel identity is namespace-shaped
The Alan Kernel SHALL treat namespace-qualified Paths, mounted file trees,
Process Table entries, stream Files, Credentials, Descriptors, Access Rights,
and native authority references as canonical semantic identity. Typed opaque
ids MAY exist as runtime references, projection keys, or V1 compatibility
surface ids, but they SHALL NOT replace namespace/path identity,
Descriptor/Access Rights authority, or native authority.

#### Scenario: File is represented as a resource
- **WHEN** a local file is represented in the Alan Kernel
- **THEN** the Alan Kernel File has a namespace-qualified Path
- **AND** its descriptor identifies the file kind and Descriptor/Access Rights boundary
- **AND** its descriptor records a native file reference rather than requiring a
  global Alan Kernel URI or private object-store copy

#### Scenario: Alan Agent session is represented as a process/file surface
- **WHEN** an Alan Agent session is represented in the Alan Kernel
- **THEN** the Alan Kernel Process appears in the Process Table with owner,
  lifecycle, stream Files, descriptors, and cancellation or signal targets
- **AND** its descriptor records the adapter-owned session reference as native
  authority

#### Scenario: Runtime reference is used
- **WHEN** an opaque runtime reference such as a V1 object, task, command, query,
  subscription, buffer, or view id is used in a projection or invocation
- **THEN** it resolves back to a Path, Process Table entry, Descriptor, stream
  File, capability descriptor, namespace, or native authority reference where
  durable semantics are required
- **AND** the runtime reference alone is not treated as the permission or authority
  boundary

#### Scenario: App or host resources are mounted
- **WHEN** an Alan App, host, service, or native system exposes resources to the
  Kernel
- **THEN** those resources are mounted into the Alan OS namespace as a file tree
- **AND** object-like V1 surfaces may index or inspect those mounted resources
  without becoming the canonical source of identity

### Requirement: Streams are named file kinds
The Alan Kernel SHALL model streams as named Files in the Alan OS namespace, not
as hidden event transport. Streams SHALL support read, tail, watch, offset
resume, and durable offset/range references where the backing source supports
them.

#### Scenario: Process emits output
- **WHEN** a process emits text, reasoning, tool lifecycle, audit, or domain output
- **THEN** the output is associated with a named stream File under the process
  or relevant namespace path
- **AND** consumers can read or tail that stream File from an offset without depending
  on renderer-private state

#### Scenario: Resource changes are observed
- **WHEN** a resource exposes change observations
- **THEN** those observations can be represented as a named stream File
- **AND** replay, host recovery, or Agent/App evidence interpretation can
  reference stream records by Path and offset

#### Scenario: Subscription is added
- **WHEN** a client wants live updates for a file, process, stream File, or view
- **THEN** the subscription is a watch operation surface over the relevant
  file, process endpoint, or stream File
- **AND** it does not introduce a second event system or become the source of
  truth for stream history

### Requirement: Objects, buffers, and views stay above Kernel
Object, buffer, and view descriptors SHALL remain app/host surfaces over the
smaller file/process/stream-file model rather than Alan Kernel primitives.

#### Scenario: Object is opened for work
- **WHEN** a command opens an object
- **THEN** the object is understood as a typed file/resource surface
- **AND** an app or host creates or resolves a buffer as the active work context
- **AND** an app or host creates or resolves a view snapshot for presentation
  without making the object itself own presentation state

#### Scenario: Query result is opened
- **WHEN** a query result is opened for inspection
- **THEN** an app or host can represent it as a buffer even when no external
  object owns the result data
- **AND** that buffer remains a typed surface over read-only file or stream-file
  data rather than a new source of native authority

### Requirement: Operation surfaces spawn, read, and observe
Commands, queries, and subscriptions SHALL remain app/service descriptors over
Kernel primitives: commands spawn processes or request side effects, queries
inspect files or snapshots, and subscriptions watch files, processes, or stream
files.

Command, query, and subscription descriptors SHALL be explainable as operation
surfaces over paths, files, processes, stream files, descriptors, access rights,
or namespaces. Registries or descriptor indexes MAY exist for V1 discovery and
compatibility, but they SHALL NOT become a separate durable Kernel ontology.

#### Scenario: Mutating action is requested
- **WHEN** a human, agent, extension, or system actor requests a mutation or
  side effect
- **THEN** the request is represented as a command invocation
- **AND** the invocation is explainable as executing a file or spawning a
  process under Descriptor/Access Rights checks and relevant service/app
  governance rules
- **AND** it is not performed through a query or subscription path

#### Scenario: Semantic state is inspected
- **WHEN** a client asks for open buffers, available commands, blocked tasks, or
  produced outputs
- **THEN** the Alan Kernel answers through a query or snapshot
- **AND** the operation does not mutate runtime or native resource state

#### Scenario: View observes changes
- **WHEN** a view depends on a file/resource, buffer, process/task surface, or query
  result
- **THEN** a subscription can be modeled as watching the relevant file, process,
  or stream File
- **AND** it can notify the renderer host that the view is dirty or updated
  without owning business logic

#### Scenario: Operation descriptor registries are reviewed
- **WHEN** a registry or index stores command, query, or subscription
  descriptors
- **THEN** it is treated as a discovery and compatibility cache over the
  namespace or projection state
- **AND** the descriptor remains grounded in the underlying path, file,
  process, stream File, Descriptor, Access Rights, or namespace semantics
- **AND** the registry does not become the source of truth for Alan Kernel
  ontology

### Requirement: Command descriptors stay above Kernel
Command descriptors SHALL remain app/service invocation contracts used by UI
controls, command palettes, modal grammar, automation, and agent projections.
They MAY reference Kernel paths, files, processes, stream Files, Credentials,
Descriptors, Access Rights, and namespaces.

#### Scenario: Command appears in multiple surfaces
- **WHEN** a command is exposed through a UI button, command palette, modal
  grammar, automation, or agent-facing schema
- **THEN** each surface preserves the same command identity, target semantics,
  argument schema, permission metadata, and service/app audit references

#### Scenario: Command availability is computed
- **WHEN** a renderer host or agent asks what actions are available for a target
- **THEN** the answer is derived from command descriptors, target state, actor
  authority, Access Checks, and app state rather than hidden view-local
  callbacks

### Requirement: Credential and causation anchors are first-class
The Alan Kernel SHALL expose Credentials, causation, and correlation anchors that
service/app event streams and projections can reference for command invocations,
processes, V1 task surfaces, produced Files, and activity events.

#### Scenario: Agent invokes a command
- **WHEN** an agent invokes a command
- **THEN** the command invocation identifies the agent actor
- **AND** subsequent process/task events, produced Files, and Agent/App artifact
  or evidence projections can remain linked through causation or correlation ids

#### Scenario: Human action triggers agent work
- **WHEN** a human action causes an agent task that creates tool tasks and
  produced Files
- **THEN** the process/task tree can preserve the human-originating correlation
  while identifying the agent and tool actors that performed each step

### Requirement: Tasks are V1 process surfaces
The Alan Kernel SHALL model command execution and long-running work as Processes
with lifecycle, hierarchy, cancellation, yields, output, and produced Files,
while exposing tasks as the V1 typed surface for those Processes.

#### Scenario: Command starts asynchronous work
- **WHEN** a command invocation starts work that may stream, block, yield, or
  produce outputs
- **THEN** the Alan Kernel creates a task descriptor and task events as the V1
  representation of the process lifecycle

#### Scenario: Work has nested activity
- **WHEN** a task launches tool calls, child processes, shell commands, or extension
  work
- **THEN** child process/task surfaces preserve parent links and can be rendered as
  a task tree

#### Scenario: Task waits for input
- **WHEN** running work needs human, agent, or extension input before continuing
- **THEN** the task emits a yielded state with a resumable request
- **AND** presentation layers can project that yield as a form or approval
  surface

### Requirement: Alan Kernel has no universal journal
The Alan Kernel SHALL NOT maintain a universal Kernel-owned semantic journal for
authority, causality, governance decisions, process/task lifecycle, yielded
checkpoints, planned side effects, committed side effects, produced Files, or
projection facts.

Services and apps MAY expose named stream Files for activity, audit, recovery,
replay, and projection rebuilds. Those stream Files SHALL remain owned by the
service/app that understands the records and SHALL NOT become Kernel ontology.

#### Scenario: Command is denied
- **WHEN** a command invocation is denied by policy
- **THEN** the owning service or app may record the invocation and decision in
  its event stream
- **AND** no committed side-effect event is recorded

#### Scenario: External mutation succeeds
- **WHEN** a command performs an external mutation such as writing a file
- **THEN** the owning service or app records the committed side effect only
  after the mutation succeeds when replay or audit requires that record
- **AND** the record identifies the native file/resource state that was observed or
  changed through Kernel pointers or native selectors

#### Scenario: Ordinary process stream emits output
- **WHEN** a process emits ordinary output, logs, text, or domain observations
- **THEN** those records remain in their named process or file stream unless an
  owning service/app stream File references them for audit, recovery, or
  projection rebuild
- **AND** Alan Kernel does not duplicate every ordinary stream record

### Requirement: Event-stream replay has no side effects
Alan services and apps SHALL replay service/app stream Files only to rebuild
projection state and SHALL NOT re-execute external side effects during replay.

#### Scenario: Service stream file is replayed
- **WHEN** a service/app stream File is replayed after restart
- **THEN** replay rebuilds tasks, buffers, views, produced-file indexes, and
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
from service/app stream Files, referenced stream Files, and native file/resource
inspection.

#### Scenario: Projection cache is missing
- **WHEN** a projection cache is absent or invalid
- **THEN** the Alan Kernel can rebuild semantic task, object, buffer, view,
  produced-file indexes, and command-availability state from authoritative
  service/app stream File records, referenced stream Files, and native file/resource
  inspection

#### Scenario: Projection checkpoint exists
- **WHEN** a projection checkpoint is used to accelerate startup
- **THEN** it is identified as a cache with source event metadata
- **AND** it does not replace the owning service/app stream File, file stream,
  or native authority as the source of truth

### Requirement: Semantic view snapshots stay above Kernel
Semantic view snapshots SHALL be renderer-independent host-facing projections
over file, process, and stream-file state. They MAY use strongly typed built-in view
models and schema-versioned dynamic extension payloads, but they SHALL NOT
become Alan Kernel state authority.

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

### Requirement: Produced results are ordinary files
The Alan Kernel SHALL model produced results as ordinary Files, stream Files, or
output pointers linked to producing Processes and underlying Files. It SHALL NOT
model Agent/App artifacts, evidence, or separate produced-output objects as
Kernel primitives.

#### Scenario: Task produces a patch
- **WHEN** a task produces a patch, report, screenshot, search result, or other
  result
- **THEN** the Alan Kernel records a File, stream File, or output pointer linked
  to the producing process/task surface and available file/resource or buffer
  references

#### Scenario: App presents an artifact
- **WHEN** an Agent/App wants to present a produced patch, report, screenshot,
  search result, or other result as an artifact
- **THEN** the Agent/App adapter maps the underlying File, stream File, or
  output pointer into an app-facing artifact projection
- **AND** Alan Kernel stores and validates only the underlying File, pointer,
  producing Process, and native authority

#### Scenario: Agent result is justified
- **WHEN** an Agent/App result needs to support a claim, memory, command
  proposal, or decision
- **THEN** the Agent/App adapter may interpret paths, stream offsets, process
  ids, descriptors, service-owned stream file offsets, app artifact paths, or
  native selectors as evidence
- **AND** Alan Kernel stores and validates only those underlying primitives
