# runtime-harness-contract Specification

## Purpose
Defines normative harness contracts for scenario semantics, runner pass/fail
criteria, KPI expectations, self-eval boundaries, and external bridge delivery.
## Requirements
### Requirement: Harness contracts live in OpenSpec and use native boundaries

Alan SHALL specify reusable harness semantics, pass/fail criteria, KPI meanings, and self-evaluation governance in OpenSpec. A harness SHALL exercise a surviving crate API, CLI command, Process boundary, AgentFS file, Tool executable, or Memory Store surface directly.

#### Scenario: Harness behavior changes

- **WHEN** a change modifies reusable scenario semantics, runner criteria, KPI meanings, or self-evaluation governance
- **THEN** the behavior is captured in this or a more specific OpenSpec capability
- **AND** the harness identifies the native product boundary it exercises

#### Scenario: Harness starts an Agent Process

- **WHEN** a scenario needs live agent execution
- **THEN** the harness launches an Agent Executable or invokes a supported Process-oriented test fixture
- **AND** it observes output, requests, actions, machine state, and exit state through Process and AgentFS surfaces

### Requirement: Harness fixtures remain executable evidence rather than hidden contracts

Scenario files, runner scripts, and harness docs SHALL remain executable fixtures and operator guidance aligned with OpenSpec-owned behavior.

#### Scenario: Fixture adds a reusable assertion

- **WHEN** a fixture begins asserting reusable product behavior
- **THEN** an active OpenSpec capability owns that behavior before the fixture becomes required
- **AND** output distinguishes passed, failed, mocked, skipped, and environment-blocked checks
