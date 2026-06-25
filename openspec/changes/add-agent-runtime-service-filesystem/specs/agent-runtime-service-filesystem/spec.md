## ADDED Requirements

### Requirement: Agent Runtime Service is a file-server service
Alan SHALL provide Agent Runtime Service as a system file-server service that
executes Agent Processes and serves AgentFS. It SHALL be managed by Service
Manager and SHALL post a mountable handle under `/srv/agent-runtime`.

#### Scenario: Agent Runtime Service starts
- **WHEN** the service starts
- **THEN** it can serve AgentFS at `/agent`
- **AND** it does not require an app-facing HTTP API to create agent work

### Requirement: AgentFS projects current runtime behavior
The first Agent Runtime Service implementation SHALL adapt the existing Agent
Execution Engine and compatibility session behavior into AgentFS files.

#### Scenario: Compatibility agent process is created
- **WHEN** current runtime starts or attaches to a session through the adapter
- **THEN** the adapter can project it as `/agent/<pid>` with status, IO,
  requests, actions, result, children, and machine files
- **AND** session ids remain native runtime references, not OS identity

### Requirement: Yields become request files
The compatibility projection SHALL translate confirmation, structured input,
dynamic tool, approval, and credential yields into request file trees.

#### Scenario: Request is answered
- **WHEN** a user or host writes `/agent/<pid>/requests/<id>/response`
- **THEN** compatibility resume behavior can deliver that response to the
  current runtime

### Requirement: Tool calls become action files
The compatibility projection SHALL translate current tool calls and external
effects into action file trees.

#### Scenario: Tool call completes
- **WHEN** the current engine reports a tool result
- **THEN** the action file tree records status, stdout/stderr/result where
  available, approval state, and process reference when a concrete process
  exists

### Requirement: Existing compatibility behavior remains compatible
The Agent Runtime Service filesystem adapter SHALL NOT break existing session
creation, attach, hydration, reconnect, submission, resume, interrupt,
compaction, rollback, or pending-yield behavior.

#### Scenario: Adapter is disabled
- **WHEN** the AgentFS projection is disabled or incomplete
- **THEN** existing Alan Shell compatibility clients continue through the legacy
  path without AgentFS projection
