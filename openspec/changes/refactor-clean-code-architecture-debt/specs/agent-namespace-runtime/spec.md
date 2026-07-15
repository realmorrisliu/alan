## ADDED Requirements

### Requirement: Agent Runtime Service owns Agent Process assembly
Agent Runtime Service SHALL own Process clone inputs, namespace mount assembly,
AgentFS lifecycle wiring, and the handles passed to an Agent Process. Agent
Execution Engine SHALL receive the assembled namespace and transition-owned
file handles and MUST NOT construct Kernel, AgentFS, LLMFS, RouteFS, or child
Process namespace infrastructure.

#### Scenario: Agent Process starts
- **WHEN** Agent Runtime Service starts an Agent Process
- **THEN** it assembles the Process namespace and AgentFS lifecycle before the
  transition loop begins
- **AND** Agent Execution Engine receives only the namespace and files needed to
  execute transitions

#### Scenario: Child Agent Process starts
- **WHEN** an Agent Process requests a child with a narrower capability set
- **THEN** Agent Runtime Service clones and assembles the child Process namespace
- **AND** Agent Execution Engine does not instantiate file servers or Kernel
  Process infrastructure for the child

#### Scenario: Dependency validation runs
- **WHEN** an assembly responsibility has moved out of Agent Execution Engine
- **THEN** its corresponding normal dependency is removed from
  `alan-agent-engine`
- **AND** the repository dependency gate rejects its reintroduction
