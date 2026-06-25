## ADDED Requirements

### Requirement: Alan Agent is built-in but optional
Alan Agent SHALL be the built-in optional Agent Workspace for inspecting,
steering, and organizing Agent Processes, requests, actions, memory, evidence,
artifacts, plans, and cross-app agent work. Alan Agent SHALL NOT be required to
spawn, inspect, steer, or complete Agent Processes.

#### Scenario: User opens Alan Agent
- **WHEN** the user opens Alan Agent
- **THEN** they can inspect active and historical agent work through workspace
  views over `/agent`, `/proc`, requests, actions, memory, evidence, and
  commands
- **AND** Alan Agent is not treated as Root Agent Process, Agent Runtime Service,
  or Service Manager

#### Scenario: User does not open Alan Agent
- **WHEN** Alan Agent is not running
- **THEN** Alan Shell can still operate Agent Processes through files and
  syscalls

### Requirement: Current sessions project into Agent Process workspace semantics
Current compatibility sessions SHALL project into Agent Process workspace
objects, conversation buffers, request/action views, evidence views, and audit
records while remaining compatibility authority during migration.

#### Scenario: Existing session is attached
- **WHEN** Alan Shell attaches to an existing compatibility session
- **THEN** the workspace projection creates or resolves an Agent Process
  projection with status, IO, requests, actions, result, children, and machine
  state where available
- **AND** existing hydration and reconnect behavior remains compatible

### Requirement: Agent Processes are inspectable workspace work
Agent Processes SHALL be inspectable as Agent Workspace work items with owner,
target descriptors, context descriptors, policy descriptors, lifecycle, child
Agent Processes, actions, artifacts, evidence, result, and audit metadata.

#### Scenario: App-created Agent Process is promoted into Alan Agent
- **WHEN** an app or user promotes an Agent Process into Alan Agent
- **THEN** Alan Agent shows the process as an Agent Workspace item
- **AND** the originating app or parent process remains the owner unless
  explicitly transferred through governed action

### Requirement: Root Agent-raised work is not global chat
Root Agent Process work SHALL appear in Alan Agent as root-agent-raised
suggestions, requests, actions, or tasks, not as an unbounded resident root
conversation.

#### Scenario: Root Agent raises work
- **WHEN** Root Agent Process identifies useful cross-app work
- **THEN** Alan Agent can show bounded work with context descriptors, proposed
  next action, permission state, and audit trail
- **AND** the user can inspect, dismiss, delegate, or promote it without opening
  a global root session

### Requirement: Alan Shell is the first compatibility host
Alan Shell SHALL remain the first compatibility host for Agent Process
projections, while preserving existing compatibility session behavior.

#### Scenario: Semantic workspace path is incomplete
- **WHEN** an Agent Process projection or renderer is incomplete
- **THEN** Alan Shell continues to use the current compatibility path for that
  surface
- **AND** replacement happens only after parity tests cover the semantic path
