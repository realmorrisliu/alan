## ADDED Requirements

### Requirement: Alan Agent is the Agent Workspace
Alan Agent SHALL be the built-in Alan App and user-visible Agent Workspace for
inspecting, steering, and organizing compatibility sessions, bounded Agent Runs,
supervisor-raised tasks, memory, evidence, artifacts, plans, and cross-app
agent work.

#### Scenario: User opens Alan Agent
- **WHEN** the user opens Alan Agent
- **THEN** they can inspect active and historical agent work through workspace
  objects, buffers, views, tasks, forms, evidence, and commands
- **AND** Alan Agent is not treated as the System Agent Supervisor itself

### Requirement: Current sessions project into workspace semantics
Current daemon-backed Alan Agent sessions SHALL project into Agent Workspace
objects, conversation buffers, task trees, approval forms, evidence views, and
audit records while remaining compatibility authority during migration.

#### Scenario: Existing session is attached
- **WHEN** Alan TUI attaches to an existing daemon-backed session
- **THEN** the workspace projection creates or resolves a compatibility session
  object, conversation buffer, active task state, pending yields, evidence, and
  available commands
- **AND** existing hydration and reconnect behavior remains compatible

### Requirement: Agent Runs are inspectable workspace work
Bounded Agent Runs SHALL be inspectable as Agent Workspace work items with
owner app, target object or task, Context Grant summary, Result Contract
summary, lifecycle, child tasks, artifacts, evidence, and audit metadata.

#### Scenario: Agent Capability run is promoted into Alan Agent
- **WHEN** an app or user promotes an Agent Capability run into Alan Agent
- **THEN** Alan Agent shows the run as an Agent Workspace item
- **AND** the originating app remains the run owner unless explicitly
  transferred by a governed command

### Requirement: Supervisor-raised tasks are not global chat
System Agent Supervisor work SHALL appear in Alan Agent as supervisor-raised
tasks or suggestions, not as an unbounded resident root conversation.

#### Scenario: Supervisor raises work
- **WHEN** the System Agent Supervisor identifies useful cross-app work
- **THEN** Alan Agent can show a bounded task with context, proposed next
  action, permission state, and audit trail
- **AND** the user can inspect, dismiss, delegate, or promote it without opening
  a global root session

### Requirement: Alan TUI is the first Agent Workspace host
Alan TUI SHALL be the first compatibility host for Agent Workspace projections,
while preserving existing daemon-backed session behavior.

#### Scenario: Semantic workspace path is incomplete
- **WHEN** an Agent Workspace projection or renderer is incomplete
- **THEN** Alan TUI continues to use the current compatibility path for that
  surface
- **AND** replacement happens only after parity tests cover the semantic path

