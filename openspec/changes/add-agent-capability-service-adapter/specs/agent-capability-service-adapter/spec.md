## ADDED Requirements

### Requirement: Agent Capability Service is a Host Service API
Alan SHALL provide Agent Capability Service as a Host Service API that can
start, schedule, stream, yield, resume, cancel, and complete bounded Agent Runs
from Agent Capability descriptors, Context Grants, and Result Contracts.

#### Scenario: App starts an Agent Run
- **WHEN** an Alan App requests a V1 Agent Capability with a Context Grant and
  Result Contract
- **THEN** Agent Capability Service starts or schedules a bounded Agent Run
- **AND** returns Agent Run identity, task identity, stream handles or polling
  handles, and audit metadata

### Requirement: Compatibility adapter wraps existing execution
The first Agent Capability Service implementation SHALL adapt the existing Agent
Execution Engine and daemon-backed session APIs rather than replacing them.

#### Scenario: Compatibility run is started
- **WHEN** the compatibility adapter receives an Agent Capability request
- **THEN** it can create or attach to current execution/session machinery as a
  native implementation detail
- **AND** it exposes the OS Agent Run lifecycle instead of raw session
  protocol as the app-facing contract

### Requirement: Context Grants are translated internally
The compatibility adapter SHALL translate Context Grants into current execution
inputs internally. It SHALL NOT expose raw prompt dumps as the OS Host
Service API.

#### Scenario: Context Grant contains selected app state
- **WHEN** an app grants a selected target, allowed reads, allowed commands, and
  evidence requirements
- **THEN** the adapter passes only that bounded context to the current execution
  engine
- **AND** records the grant and any unsupported grant fields in audit metadata

### Requirement: Result Contracts are reported structurally
The compatibility adapter SHALL report Agent Run output according to the
requested Result Contract, including partial or unsupported fields when current
execution cannot yet satisfy them.

#### Scenario: Current execution returns text and evidence
- **WHEN** the current engine produces assistant text, tool results, child-run
  outcomes, artifacts, or rollout evidence
- **THEN** the adapter maps those outputs into answer, citation, evidence,
  proposed command, uncertainty, follow-up, artifact, and audit fields where
  possible
- **AND** unsupported requested fields are explicit rather than hidden in plain
  text

### Requirement: Existing session behavior remains compatible
The compatibility adapter SHALL NOT break existing daemon-backed session
creation, attach, hydration, reconnect, submission, resume, interrupt,
compaction, rollback, or pending-yield behavior.

#### Scenario: Adapter is disabled
- **WHEN** the Agent Capability Service adapter is disabled or incomplete
- **THEN** existing Alan TUI and daemon session clients continue through the
  compatibility path without semantic Agent Run projection

