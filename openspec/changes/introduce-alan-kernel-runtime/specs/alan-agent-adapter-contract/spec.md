## ADDED Requirements

### Requirement: Compatibility projection isolates current protocol details
The compatibility projection layer SHALL translate the current Agent Execution
Engine and session protocol into Agent Process file surfaces without making Alan
Kernel depend on `alan-protocol`, compatibility transport clients, provider
clients, memory stores, sandbox backends, or session lifecycle details.

Artifacts in this projection layer SHALL mean Agent/App interpretations over
Kernel Files, stream Files, output pointers, or native selectors. Evidence SHALL
mean an Agent/App interpretation over Kernel paths, stream offsets, process ids,
descriptors, service-owned stream file offsets, app artifact paths, or native
selectors. Alan Kernel SHALL NOT model Artifact, Evidence, produced-output
objects, or ProvenanceRef as durable primitives.

#### Scenario: Projection dependencies are inspected
- **WHEN** Alan Kernel crates are built or audited
- **THEN** optional app/projection modules may depend on `alan-protocol` and
  compatibility transport clients as internal details
- **AND** `alan-kernel` remains free of those dependencies

#### Scenario: Current event enters the projection path
- **WHEN** an `alan_protocol::EventEnvelope` is received by a compatibility
  session consumer
- **THEN** the projection layer maps it into Agent Process status, IO events,
  request files, action files, machine events, optional workspace buffers/views,
  artifacts, or Agent/App evidence before it reaches renderer hosts

### Requirement: Current sessions project into Agent Process surfaces
The compatibility projection layer SHALL represent current Alan sessions as
Agent Process file surfaces. Session metadata SHALL project to status,
conversation state SHALL project to Agent IO, runtime/tape state SHALL project
to Agent Machine, yields SHALL project to request files, tool calls SHALL
project to action files, and checkpoints SHALL project to machine checkpoints.

#### Scenario: Existing session is attached
- **WHEN** Alan Shell or another host attaches through the current compatibility
  session path
- **THEN** the projection layer creates or resolves an Agent Process projection
- **AND** it exposes status, IO, requests, actions, result, and machine files
  without changing the current runtime behavior

#### Scenario: Future file-native client attaches
- **WHEN** a future client attaches after AgentFS parity exists
- **THEN** it can open or watch `/agent/<pid>` files rather than calling a
  session API

### Requirement: Agent IO is the default conversation surface
The compatibility projection layer SHALL map user input, assistant output,
thinking summaries, yielded state, warnings, errors, and result readiness into
Agent IO files and events.

#### Scenario: Conversation output streams
- **WHEN** current Alan emits text or thinking deltas
- **THEN** the projection updates `/agent/<pid>/io/output` and
  `/agent/<pid>/io/events`
- **AND** renderer hosts do not need to parse raw protocol events by default

### Requirement: Runtime details map to Agent Machine
The compatibility projection layer SHALL map tape, rollout records, compaction,
memory flush observations, retries, guardrails, and checkpoints into Agent
Machine files where access rights allow inspection.

#### Scenario: Debug view inspects runtime state
- **WHEN** a permitted host opens `/agent/<pid>/machine`
- **THEN** it can inspect tape, state, machine events, and checkpoints
- **AND** those files remain runtime schema rather than Alan Kernel ontology

### Requirement: Yields map to request file trees
Current confirmation, structured input, dynamic tool, and approval yields SHALL
project into `/agent/<pid>/requests/<request-id>` file trees.

#### Scenario: Confirmation yield is received
- **WHEN** current Alan emits a confirmation yield
- **THEN** the projection creates a request tree with kind, prompt, options,
  status, and response files
- **AND** resume compatibility can be implemented by writing the response file

### Requirement: Tool calls map to action file trees
Current tool calls and other external effects SHALL project into
`/agent/<pid>/actions/<action-id>` file trees. If a concrete tool process is
spawned, the action SHALL link to the relevant `/proc/<tool-pid>` entry.

#### Scenario: Tool call starts and completes
- **WHEN** current Alan emits tool call lifecycle events
- **THEN** the projection creates or updates an action file tree with status,
  stdout/stderr/result where applicable, risk, approval, and process link when
  known

### Requirement: Alan Agent is optional workspace over the same files
The future Alan Agent app module SHALL be an optional workspace over Agent
Process file surfaces. It SHALL NOT own agent execution, Root Agent Process,
Service Manager, or Agent Runtime Service.

#### Scenario: User opens Alan Agent
- **WHEN** the user opens Alan Agent
- **THEN** it provides richer buffers and views over `/agent`, `/proc`,
  requests, actions, memory, evidence, and cross-app work
- **AND** the same work remains operable from Alan Shell through files and
  process syscalls

### Requirement: Compatibility transport remains temporary
The compatibility projection layer SHALL preserve current session creation,
hydration, replay, reconnect, submission, resume, interrupt, compaction,
rollback, and pending-yield behavior during migration, while documenting that
the target model is spawn/open/watch over files and processes.

#### Scenario: Existing TUI reconnects
- **WHEN** Alan Shell reconnects through the current compatibility path
- **THEN** it still reads persisted history or reconnect snapshots before
  consuming new events
- **AND** it does not lose buffered yield or stream events that the existing path
  would preserve

#### Scenario: Projection path is disabled
- **WHEN** the Agent Process projection path is disabled during migration
- **THEN** existing compatibility behavior can continue through the legacy path
  until file-surface parity is proven
