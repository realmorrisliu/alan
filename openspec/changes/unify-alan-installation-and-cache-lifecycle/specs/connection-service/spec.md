## MODIFIED Requirements

### Requirement: Connection Service owns profile metadata
Connection Service SHALL own provider/model settings, profile identity,
defaults, Process selection, validation status, and publication of callable LLM
connection trees. Metadata SHALL belong to the explicit product or isolated-test System Store binding supplied by the Host; executable names MUST NOT select its storage.

#### Scenario: Agent selects a profile
- **WHEN** its launch context passes an installed Connection reference
- **THEN** the Agent Process receives the corresponding callable LLM tree
- **AND** no Host config file is read

