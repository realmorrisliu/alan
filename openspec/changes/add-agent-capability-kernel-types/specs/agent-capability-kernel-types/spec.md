## ADDED Requirements

### Requirement: Kernel defines Agent Capability semantic types
Alan Kernel SHALL define semantic types for Agent Capability descriptors, Agent
Runs, Context Grants, Result Contracts, Effect Classes, Command Risk, Execution
Guard metadata, evidence references, and audit references.

#### Scenario: Agent Capability request is modeled
- **WHEN** an app prepares to request AI-mediated work
- **THEN** Kernel types can represent the descriptor id, bounded Agent Run
  identity, Context Grant, expected Result Contract, command risk metadata,
  evidence requirement, and audit references
- **AND** those types do not require provider, daemon, session, sandbox, memory,
  or renderer dependencies

### Requirement: V1 descriptor taxonomy is explicit
Alan Kernel SHALL include descriptor definitions for the first implementation
set: `agent.explain`, `agent.summarize`, `agent.plan`,
`agent.propose_commands`, and `agent.delegate`.

#### Scenario: Domain app maps feature to descriptor
- **WHEN** UPDF requests reading assistance or Groove Master requests practice
  planning
- **THEN** the app can map the domain feature to one or more V1 Agent
  Capability descriptors without inventing an app-local agent protocol

### Requirement: Context Grants bound input authority
Context Grant types SHALL represent app identity, target references, view
references or selected ranges, task goals, allowed reads, allowed commands,
privacy policy, evidence requirements, and expected result shape.

#### Scenario: App grants selected context
- **WHEN** an app grants a selected document range or practice session state to
  an Agent Run
- **THEN** the Context Grant records the bounded target and permitted reads
- **AND** it does not imply full app-state access

### Requirement: Result Contracts request typed output
Result Contract types SHALL represent requested outputs such as answers,
summaries, plans, citations, evidence, proposed commands, follow-up questions,
uncertainty, and audit summaries.

#### Scenario: Agent Run completes
- **WHEN** an Agent Run completes through a future Host Service implementation
- **THEN** the result can be checked against the requested Result Contract
- **AND** the requesting app does not need to parse plain text to find
  citations, evidence, proposed commands, or audit metadata

### Requirement: Kernel records governance metadata but not execution
Kernel Agent Capability types SHALL model Effect Classes, Command Risk, and
Execution Guard metadata for Command Governance. Kernel SHALL NOT execute
commands, run providers, open daemon sessions, enforce concrete sandboxes, or
store agent memory.

#### Scenario: Dependency boundary is audited
- **WHEN** the Kernel crate dependencies are reviewed
- **THEN** Agent Capability semantic types do not introduce dependencies on
  `alan-runtime`, `alan-protocol`, daemon clients, provider clients, memory
  stores, sandbox implementations, Ratatui, SwiftUI, or Tokio task handles

