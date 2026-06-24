## ADDED Requirements

### Requirement: Alan Agent app module isolates agent protocol details
The Alan Agent app module SHALL model Alan Agent as a built-in Alan App and
agent actor while translating the current Agent Execution Engine and current
daemon-backed Host Service Implementation session protocol details into Alan App
semantics without making Alan Kernel depend on `alan-protocol` or
daemon client types.

#### Scenario: App module dependencies are inspected
- **WHEN** Alan Kernel crates are built or audited
- **THEN** the future `alan-agent` app module may depend on `alan-protocol` and
  daemon client types as internal implementation details
- **AND** `alan-kernel` remains free of
  those dependencies

#### Scenario: Alan event enters the app projection path
- **WHEN** an `alan_protocol::EventEnvelope` is received by an Alan session
  consumer
- **THEN** the Alan Agent app module maps it into Alan Agent app objects,
  buffers, views, command state, task events, conversation updates, form state,
  artifacts, or evidence before it reaches renderer hosts

### Requirement: Alan Agent app module can host Agent Capability compatibility
The Alan Agent app module SHALL be the first compatibility adapter candidate for
Agent Capability Service over the current Agent Execution Engine and
daemon-backed session APIs. This adapter SHALL translate Context Grants and
Result Contracts into current execution inputs and outputs without making Alan
Kernel depend on `alan-protocol`, provider clients, daemon clients, memory
stores, sandbox backends, or session lifecycle details.

#### Scenario: App requests an Agent Capability through compatibility path
- **WHEN** a domain app requests agent work before a durable Agent Capability
  Service implementation exists
- **THEN** the compatibility adapter can start or attach to current Alan Agent
  execution using the bounded Context Grant and Result Contract
- **AND** the resulting Agent Run, task lifecycle, yielded checkpoints,
  artifacts, evidence, and audit metadata are projected into Alan OS
  semantics

#### Scenario: Agent Capability work is inspected
- **WHEN** the user wants to inspect, steer, or promote that work into a full
  workspace
- **THEN** Alan Agent can present it as part of the Agent Workspace
- **AND** the requesting app did not need to create or focus an Alan Agent
  conversation as the primary app feature

### Requirement: Alan Agent sessions project as app objects and buffers
The Alan Agent app module SHALL represent an Alan Agent session as an
inspectable Alan App object with conversation and task-oriented buffers.

#### Scenario: Session is attached
- **WHEN** the TUI or another host creates or attaches to an Alan session through
  Host Service APIs
- **THEN** the app module creates or resolves an agent session object
- **AND** it exposes at least one conversation buffer and view for that session

#### Scenario: Session metadata is available
- **WHEN** profile, provider, model, durability, workspace, or session metadata
  is returned by Host Service APIs
- **THEN** the app module records it as object or buffer metadata without making
  it the Alan Kernel identity format

### Requirement: Alan Agent operations project as commands
The Alan Agent app module SHALL expose supported Alan Agent session operations as
Alan Kernel command descriptors and invocations.

#### Scenario: User submits an agent turn
- **WHEN** a human or agent submits text or structured input to an Alan session
- **THEN** the app module represents the operation as an Alan Kernel command
  invocation
- **AND** successful dispatch through Host Service APIs starts or resumes an
  Alan Kernel task

#### Scenario: User interrupts active agent work
- **WHEN** the user requests interruption of active agent work
- **THEN** the app module invokes a cancel or interrupt command against the active
  task
- **AND** Host Service API submission remains an app-module implementation detail

#### Scenario: Session control command is invoked
- **WHEN** compact, rollback, resume, or another supported Alan session control
  operation is requested
- **THEN** the app module exposes it through command descriptors with target,
  argument, actor, and audit metadata

### Requirement: Alan Agent execution events map to task lifecycle
The Alan Agent app module SHALL map Alan turns, tool calls, yields, child runs,
and stream progress into Alan Kernel task descriptors and task events.

#### Scenario: Turn starts and streams
- **WHEN** Alan emits turn and text or thinking stream events
- **THEN** the app module maps the turn to an Alan Kernel task
- **AND** streamed text or thinking updates the conversation projection without
  requiring renderer hosts to parse Alan event variants

#### Scenario: Tool call starts and completes
- **WHEN** Alan emits tool call lifecycle events
- **THEN** the app module maps the tool call to a child task of the active turn
- **AND** completion updates task status, artifacts, evidence, and conversation
  summaries as applicable

#### Scenario: Child run is observed
- **WHEN** Alan child-run records or child lifecycle events are available
- **THEN** the app module maps them to Alan Kernel child tasks with parent links,
  status, progress, terminal outcome, and evidence references

### Requirement: Alan Agent yields project as task pauses and forms
The Alan Agent app module SHALL map Alan yields into task yielded states and
semantic form or approval views.

#### Scenario: Confirmation yield is received
- **WHEN** Alan emits a confirmation yield
- **THEN** the app module maps it to an Alan Kernel task yielded state
- **AND** it projects a form or approval view that can resume the task through a
  command invocation

#### Scenario: Structured input yield is received
- **WHEN** Alan emits a structured input yield with questions or schema-like
  payload
- **THEN** the app module maps it to a semantic form view model
- **AND** form submission is represented as a resume command against the yielded
  task

#### Scenario: Dynamic tool yield is received
- **WHEN** Alan emits a dynamic tool yield
- **THEN** the app module preserves the yielded task checkpoint and payload
- **AND** execution or rejection of the client-side tool result remains mediated
  by an Alan Kernel command path

### Requirement: Conversation views preserve Alan Agent app semantics
The Alan Agent app module SHALL project Alan Agent conversation state into typed
conversation view models rather than plain logs.

#### Scenario: Conversation snapshot is requested
- **WHEN** a renderer host requests the active Alan conversation view
- **THEN** the app-module projection includes typed blocks for user text,
  assistant text, thinking, tool summaries, plans, yields, errors, artifacts,
  and linked tasks when available

#### Scenario: Persisted history is hydrated
- **WHEN** an Alan session is hydrated from daemon-backed history and
  reconnect state
- **THEN** the app module reconstructs conversation and pending-input projections
  through Host Service API results without requiring the host renderer to know
  daemon hydration details

### Requirement: Alan Agent app module preserves compatibility with Host Service APIs
The Alan Agent app module SHALL preserve existing daemon-backed session creation,
hydration, replay, reconnect, submission, resume, interrupt, compaction,
rollback, and pending-yield behavior during the first integration.

#### Scenario: Existing TUI reconnects
- **WHEN** Alan TUI reconnects to an existing Alan daemon session through
  the Alan Agent app module path
- **THEN** it still reads persisted history or reconnect snapshots before
  consuming new events
- **AND** it does not lose buffered yield or stream events that the existing TUI
  path would preserve

#### Scenario: App module path is disabled or removed
- **WHEN** the Alan Agent app module integration is disabled during migration
- **THEN** existing daemon-backed TUI behavior can continue through the legacy
  path until semantic parity is proven
