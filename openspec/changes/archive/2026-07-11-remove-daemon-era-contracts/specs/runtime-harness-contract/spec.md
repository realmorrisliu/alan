## REMOVED Requirements

### Requirement: Harness contracts live in OpenSpec

**Reason**: The requirement includes external bridge message delivery as a canonical harness semantic.

**Migration**: Keep normative harness behavior in OpenSpec without a transport-specific bridge contract.

### Requirement: Harness bridge delivery is explicit

**Reason**: The external daemon bridge is removed with no replacement.

**Migration**: Harnesses enter through surviving Process, AgentFS, CLI, or crate boundaries.

### Requirement: Harness bridge roles and planes are stable

**Reason**: The controller, relay, client, attach-Session, and app-server planes belong to the retired architecture.

**Migration**: None.

### Requirement: Harness bridge envelope supports replay and tracing

**Reason**: The bridge envelope is transport-only protocol surface.

**Migration**: File streams and rollout records own their own offsets and evidence identifiers.

### Requirement: Harness bridge reconnects without duplicating side effects

**Reason**: The reconnect and controller authorization lifecycle belongs to the removed bridge.

**Migration**: Side-effect idempotency remains with the Tool, policy, and persistence owners that execute it.

### Requirement: Harness bridge preserves app-server and capability-router semantics

**Reason**: App-server, compatibility Session, and bridge routing semantics are removed.

**Migration**: Harnesses assert Agent Execution Engine and AgentFS behavior at their native boundary.

### Requirement: Harness bridge security does not bypass target governance

**Reason**: The bridge identity and Session scope model no longer exists.

**Migration**: Process credentials, descriptors, policy, and execution backends remain authoritative.

### Requirement: Harness bridge exposes recovery and SLO signals

**Reason**: These metrics and recovery states describe the removed bridge transport.

**Migration**: Harnesses report failures from the specific surviving boundary they exercise.

### Requirement: Harness scenarios remain executable fixtures, not hidden contracts

**Reason**: The requirement still couples fixture governance to external bridge outputs.

**Migration**: Retain fixture-versus-contract separation without bridge terminology.

## ADDED Requirements

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
