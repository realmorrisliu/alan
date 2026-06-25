## ADDED Requirements

### Requirement: Compatibility projection isolates current protocol details
The compatibility projection layer SHALL translate the current Agent Execution
Engine and session protocol into agent process file surfaces (per
`define-agent-file-layout-contract`) without making `alan-kernel` depend on
`alan-protocol`, compatibility transport clients, provider clients, memory
stores, sandbox backends, or session lifecycle details. The projection is a
user-space file server above the substrate, not part of the kernel.

#### Scenario: Projection dependencies are inspected
- **WHEN** the workspace crates are built or audited
- **THEN** the projection / adapter module may depend on `alan-protocol` and
  compatibility transport clients as internal details
- **AND** `alan-kernel` remains free of those dependencies

#### Scenario: Current event enters the projection path
- **WHEN** an `alan_protocol::EventEnvelope` is received by a compatibility
  session consumer
- **THEN** the projection layer maps it into the process's `status`, `io/events`,
  `requests/`, `actions/`, and `machine/` files
- **AND** it does so before the data reaches any shell or host client

### Requirement: Current sessions project into agent process file surfaces
The compatibility projection layer SHALL register current Alan sessions as real
kernel Processes first, and only then surface their agent overlay under `/agent`.
A session SHALL NOT appear under `/agent` without a backing `/proc` Process —
otherwise `/agent` would become an independent process table. It SHALL NOT
fabricate `/proc/<pid>` entries itself; `/proc` is the kernel's source of truth,
and `/agent/<pid>` is the overlay of that `/proc/<pid>` with the projected agent
surfaces.
Session metadata SHALL project to `status`, conversation state to `io/`,
runtime/tape state to `machine/`, yields to `requests/`, tool calls to
`actions/`, and recovery to `machine/` checkpoints. There SHALL be no separate
`Agent Process` kernel type; a registered session is an ordinary `Process` that
conforms to the agent file-layout convention.

#### Scenario: Existing session is attached
- **WHEN** Alan Shell or another host attaches through the current compatibility
  session path
- **THEN** the projection layer registers (or resolves) a real kernel Process —
  which the kernel renders in `/proc` — and serves its agent surfaces under
  `/agent`, without fabricating `/proc` entries itself
- **AND** it exposes `status`, `io/`, `requests/`, `actions/`, `machine/`,
  `context/`, `children/`, and the top-level aggregate `events` stream per
  `define-agent-file-layout-contract`
  (results are conveyed via `io/output` and per-action `actions/<id>/result`,
  not a top-level `result` file) without changing current runtime behavior

#### Scenario: Future file-native client attaches
- **WHEN** a future client attaches after file-surface parity exists
- **THEN** it opens or watches the process's files (for example `/agent/<pid>/io`
  or `requests/`) rather than calling a session API

### Requirement: Agent IO is the default conversation surface
The compatibility projection layer SHALL map user input, assistant output,
thinking summaries, yielded state, warnings, errors, and result readiness into
`io/` files and events.

#### Scenario: Conversation output streams
- **WHEN** current Alan emits text or thinking deltas
- **THEN** the projection updates `/agent/<pid>/io/output` and
  `/agent/<pid>/io/events`
- **AND** clients read those files by offset rather than parsing raw protocol
  events

### Requirement: Runtime details map to the machine directory
The compatibility projection layer SHALL map tape, rollout records, compaction,
memory flush observations, retries, guardrails, and checkpoints into `machine/`
files where access rights allow inspection. `machine/tape` SHALL be the truth;
the model context window SHALL be a view over it (compaction is a view, not a
hidden step).

#### Scenario: Debug view inspects runtime state
- **WHEN** a permitted client opens `/agent/<pid>/machine`
- **THEN** it can inspect tape, state, machine events, and checkpoints
- **AND** those files are owned by the runtime file server, not by kernel
  ontology

### Requirement: Yields map to request file trees
Current confirmation, structured input, dynamic tool, and approval yields SHALL
project into `/agent/<pid>/requests/<request-id>` file trees, answered by writing
the response file. New `requests/` entries SHALL be observable through the
`requests/` events stream (blocking read), not by polling.

#### Scenario: Confirmation yield is received
- **WHEN** current Alan emits a confirmation yield
- **THEN** the projection creates a request tree with kind, prompt, options,
  status, and response files
- **AND** resume compatibility is implemented by delivering the written response
  file

### Requirement: Tool calls map to action file trees
Current tool calls and other external effects SHALL project into
`/agent/<pid>/actions/<action-id>` file trees. If a concrete tool process is
spawned, the action SHALL link to its `/proc/<tool-pid>` entry rather than
duplicating it.

#### Scenario: Tool call starts and completes
- **WHEN** current Alan emits tool call lifecycle events
- **THEN** the projection creates or updates an action file tree with status,
  stdout/stderr/result where applicable, risk, approval, and process link when
  known

### Requirement: Compatibility transport remains temporary
The compatibility projection layer SHALL preserve current session creation,
hydration, replay, reconnect, submission, resume, interrupt, compaction,
rollback, and pending-yield behavior during migration, while documenting that
the target model is spawn / open / watch over files and processes.

#### Scenario: Existing TUI reconnects
- **WHEN** Alan Shell reconnects through the current compatibility path
- **THEN** it still reads persisted history or reconnect snapshots before
  consuming new events
- **AND** it does not lose buffered yield or stream events the existing path
  would preserve

#### Scenario: Projection path is disabled
- **WHEN** the file projection path is disabled during migration
- **THEN** existing compatibility behavior continues through the legacy path
  until file-surface parity is proven
