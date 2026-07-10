## ADDED Requirements

### Requirement: Request controls compose with cognitive Connection selection
Alan SHALL select the cognitive-role llmfs Connection before resolving and
validating canonical reasoning-effort intent for that Generation. The normalized
control SHALL be written into the provider-neutral llmfs request document;
provider adapters SHALL NOT infer cognitive roles or routing precedence.

#### Scenario: System 1 has effort intent
- **WHEN** a System 1 attempt uses a Connection with configured reasoning-effort
  intent
- **THEN** Alan validates the effort against that Connection's model metadata and
  includes the normalized value in the Generation request

#### Scenario: System 2 uses a different model
- **WHEN** escalation selects a System 2 Connection with different supported
  effort values
- **THEN** request-control resolution runs against the System 2 Connection
- **AND** no System 1 effective control is copied blindly

#### Scenario: Provider adapter receives the request
- **WHEN** llmfs maps the committed provider-neutral document into
  `GenerationRequest`
- **THEN** the adapter receives normalized reasoning controls only
- **AND** it does not receive or decide the System 1/System 2 role
