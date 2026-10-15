## ADDED Requirements

### Requirement: Responses function Tools preserve declared parameter optionality
Official and managed Responses adapters SHALL preserve the supplied Tool
parameter schema and its declared required fields. Their function-Tool wire
definitions SHALL explicitly disable implicit strict normalization that would
turn optional arguments into required fields. Engine-side argument schema
validation SHALL remain enforced before Tool execution. This MUST NOT change
typed evaluation/output constraints, provider credentials, profile/model/effort,
Process authority or native grant/range validation.

#### Scenario: Tool has optional alternative ranges
- **WHEN** a Tool parameter schema has required path and optional line or byte range fields
- **THEN** streaming and non-streaming Responses requests retain the supplied schema with explicit strict false
- **AND** omitted optional fields remain permitted without silently accepting mixed or invalid ranges locally

#### Scenario: A failed native task prompts adapter repair
- **WHEN** the old wire adapter and repeated mixed arguments are observed during qualification
- **THEN** the failure remains retained and the causal interpretation is distinguished from a direct server trace
- **AND** wire tests and a fresh unchanged-task native retry are required before claiming a qualified completion
