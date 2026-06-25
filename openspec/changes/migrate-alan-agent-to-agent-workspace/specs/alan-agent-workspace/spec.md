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

### Requirement: Alan Agent reads current sessions as agent files
Alan Agent SHALL present current compatibility sessions by reading the agent file
surfaces (`status`, `io/`, `requests/`, `actions/`, `machine/`, `context/`,
`children/`, `events`) as a client. It SHALL NOT reintroduce the retired
object/buffer/view/evidence projection ontology; conversation is `io/`, requests
and actions are their file trees, and evidence is interpreted from files and
per-action records above the kernel.

#### Scenario: Existing session is attached
- **WHEN** Alan Agent attaches to an existing compatibility session
- **THEN** it reads the agent-conforming process surface (`status`, `io/`,
  `requests/`, `actions/`, `machine/`, `context/`, `children/`, `events`); there
  is no top-level `result` file (results are `io/output` plus per-action
  `actions/<id>/result`)
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
