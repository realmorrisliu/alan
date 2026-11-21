## ADDED Requirements

### Requirement: Ephemeral invocations leave the root rollout pointer untouched
An invocation started as ephemeral SHALL NOT read or overwrite the product root
rollout pointer and SHALL be rejected when combined with explicit recovery.

#### Scenario: An ephemeral invocation ends
- **WHEN** an ephemeral invocation runs and exits
- **THEN** the next explicit recovery selects the same durable work it would have before
- **AND** the ephemeral invocation never became the recorded root
