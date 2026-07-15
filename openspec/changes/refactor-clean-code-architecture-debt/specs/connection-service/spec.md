## ADDED Requirements

### Requirement: Agent Execution Engine consumes mounted connections only
Connection Service SHALL exclusively own connection profile metadata, defaults,
selection, validation status, and publication. Agent Execution Engine MUST NOT
read, write, merge, or select connection profiles; it SHALL invoke only the
callable connection handle mounted into the Agent Process namespace by its
launch context.

#### Scenario: Agent Process begins a transition
- **WHEN** an Agent Process has a callable connection mounted in its namespace
- **THEN** Agent Execution Engine uses that mounted handle for generation
- **AND** it does not open a profile metadata store or resolve a default profile

#### Scenario: Profile selection changes
- **WHEN** an operator changes a default or Process-selected profile
- **THEN** Connection Service validates and publishes the selected callable tree
- **AND** Agent Execution Engine code and state remain unchanged

#### Scenario: Engine ownership validation runs
- **WHEN** repository architecture checks inspect Agent Execution Engine
- **THEN** no profile metadata persistence, merging, default selection, or Host
  credential lookup remains in the engine
