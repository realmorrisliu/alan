## ADDED Requirements

### Requirement: Agent adapter isolates Alan protocol
The workbench agent adapter SHALL translate Alan agent daemon/session protocol
state into workbench semantics without making workbench core depend on
`alan-protocol`.

#### Scenario: Adapter crate dependencies are inspected
- **WHEN** workbench crates are built or audited
- **THEN** `workbench-agent` may depend on `alan-protocol` and Alan daemon
  client types
- **AND** `workbench-core` remains free of those dependencies

#### Scenario: Alan event enters the workbench
- **WHEN** an `alan_protocol::EventEnvelope` is received by an Alan session
  consumer
- **THEN** `workbench-agent` maps it into workbench task events, conversation
  updates, form state, artifacts, or evidence before it reaches renderer hosts

### Requirement: Agent sessions project as objects and buffers
The workbench agent adapter SHALL represent an Alan agent session as an
inspectable object with conversation and task-oriented buffers.

#### Scenario: Session is attached
- **WHEN** the TUI or another host creates or attaches to an Alan daemon session
- **THEN** the adapter creates or resolves an agent session object
- **AND** it exposes at least one conversation buffer and view for that session

#### Scenario: Session metadata is available
- **WHEN** profile, provider, model, durability, workspace, or session metadata
  is returned by daemon APIs
- **THEN** the adapter records it as object or buffer metadata without making it
  the workbench core identity format

### Requirement: Agent operations project as commands
The workbench agent adapter SHALL expose supported Alan session operations as
workbench command descriptors and invocations.

#### Scenario: User submits an agent turn
- **WHEN** a human or agent submits text or structured input to an Alan session
- **THEN** the adapter represents the operation as a workbench command
  invocation
- **AND** successful dispatch to the daemon starts or resumes a workbench task

#### Scenario: User interrupts active agent work
- **WHEN** the user requests interruption of active agent work
- **THEN** the adapter invokes a cancel or interrupt command against the active
  task
- **AND** daemon submission remains the adapter-owned execution detail

#### Scenario: Session control command is invoked
- **WHEN** compact, rollback, resume, or another supported Alan session control
  operation is requested
- **THEN** the adapter exposes it through command descriptors with target,
  argument, actor, and audit metadata

### Requirement: Agent events map to task lifecycle
The workbench agent adapter SHALL map Alan turns, tool calls, yields, child
runs, and stream progress into workbench task descriptors and task events.

#### Scenario: Turn starts and streams
- **WHEN** Alan emits turn and text or thinking stream events
- **THEN** the adapter maps the turn to a workbench task
- **AND** streamed text or thinking updates the conversation projection without
  requiring renderer hosts to parse Alan event variants

#### Scenario: Tool call starts and completes
- **WHEN** Alan emits tool call lifecycle events
- **THEN** the adapter maps the tool call to a child task of the active turn
- **AND** completion updates task status, artifacts, evidence, and conversation
  summaries as applicable

#### Scenario: Child run is observed
- **WHEN** Alan child-run records or child lifecycle events are available
- **THEN** the adapter maps them to workbench child tasks with parent links,
  status, progress, terminal outcome, and evidence references

### Requirement: Agent yields project as task pauses and forms
The workbench agent adapter SHALL map Alan yields into task yielded states and
semantic form or approval views.

#### Scenario: Confirmation yield is received
- **WHEN** Alan emits a confirmation yield
- **THEN** the adapter maps it to a workbench task yielded state
- **AND** it projects a form or approval view that can resume the task through a
  command invocation

#### Scenario: Structured input yield is received
- **WHEN** Alan emits a structured input yield with questions or schema-like
  payload
- **THEN** the adapter maps it to a semantic form view model
- **AND** form submission is represented as a resume command against the yielded
  task

#### Scenario: Dynamic tool yield is received
- **WHEN** Alan emits a dynamic tool yield
- **THEN** the adapter preserves the yielded task checkpoint and payload
- **AND** execution or rejection of the client-side tool result remains mediated
  by a workbench command path

### Requirement: Conversation views preserve agent semantics
The workbench agent adapter SHALL project Alan conversation state into typed
conversation view models rather than plain logs.

#### Scenario: Conversation snapshot is requested
- **WHEN** a renderer host requests the active Alan conversation view
- **THEN** the adapter-backed projection includes typed blocks for user text,
  assistant text, thinking, tool summaries, plans, yields, errors, artifacts,
  and linked tasks when available

#### Scenario: Persisted history is hydrated
- **WHEN** an Alan session is hydrated from daemon history and reconnect state
- **THEN** the adapter reconstructs conversation and pending-input projections
  without requiring the host renderer to know daemon hydration details

### Requirement: Agent adapter preserves compatibility with daemon behavior
The workbench agent adapter SHALL preserve existing daemon-backed session
creation, hydration, replay, reconnect, submission, resume, interrupt,
compaction, rollback, and pending-yield behavior during the first integration.

#### Scenario: Existing TUI reconnects
- **WHEN** the Ratatui TUI reconnects to an existing Alan daemon session through
  the workbench adapter path
- **THEN** it still reads persisted history or reconnect snapshots before
  consuming new events
- **AND** it does not lose buffered yield or stream events that the existing TUI
  path would preserve

#### Scenario: Adapter path is disabled or removed
- **WHEN** the workbench adapter integration is disabled during migration
- **THEN** existing daemon-backed TUI behavior can continue through the legacy
  path until semantic parity is proven
