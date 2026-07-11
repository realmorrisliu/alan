## MODIFIED Requirements

### Requirement: OpenRouter provider identity
alan SHALL expose the SDK-backed OpenRouter chat adapter under the canonical provider id
`openrouter` across runtime configuration, connection profiles, direct CLI descriptors, and Agent
Process connection bindings.

#### Scenario: Agent Process uses an OpenRouter profile
- **WHEN** an Agent Process is spawned with `connection_profile = "openrouter-main"`
- **THEN** the resolved provider identity is `openrouter`
- **AND** provider selection requires no background API catalog

### Requirement: OpenRouter connection settings
The OpenRouter descriptor SHALL expose its supported connection settings through the direct CLI and
owning connection metadata surfaces. Settings SHALL include model and supported endpoint/provider
options without exposing secrets.

#### Scenario: Operator lists OpenRouter settings
- **WHEN** an operator runs the direct connection-provider listing command
- **THEN** the OpenRouter descriptor reports supported non-secret settings
- **AND** the descriptor is resolved from the local provider and connection owners
