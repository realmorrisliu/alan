## ADDED Requirements

### Requirement: Public reasoning configuration has one canonical form
Alan SHALL expose `model_reasoning_effort` as the only agent-facing reasoning
configuration control. Configuration, protocol, API, and client documents SHALL
reject `thinking_budget_tokens` as an unknown or unsupported field and SHALL NOT
migrate it to a named effort.

#### Scenario: Canonical reasoning effort is configured
- **WHEN** a valid `model_reasoning_effort` is present in agent configuration
- **THEN** Alan resolves and validates that effort through the canonical request
  control path

#### Scenario: Retired thinking budget is configured
- **WHEN** agent configuration contains `thinking_budget_tokens`
- **THEN** configuration loading fails and identifies the retired field
- **AND** Alan does not translate the numeric value into a reasoning effort

#### Scenario: Provider needs a numeric wire budget
- **WHEN** a provider adapter must send a numeric budget for a validated named
  reasoning effort
- **THEN** the adapter derives that provider-native value internally
- **AND** the derived wire field does not create a public budget-token input
