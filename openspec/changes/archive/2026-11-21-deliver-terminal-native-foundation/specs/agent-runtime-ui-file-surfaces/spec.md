## ADDED Requirements

### Requirement: Usage is projected under `machine/ui/usage`
The Agent Runtime SHALL project a bounded usage snapshot at `machine/ui/usage` with
turn and session input, cached-input, output and reasoning token counts, the model
context window and an optional cost with provenance, and SHALL append an ordered
update record to the Agent event stream. The snapshot SHALL carry no authority.

#### Scenario: A turn completes
- **WHEN** a generation turn finishes with usage accounting
- **THEN** the usage file reflects the turn and session totals
- **AND** a renderer reading older surfaces continues to work unchanged
