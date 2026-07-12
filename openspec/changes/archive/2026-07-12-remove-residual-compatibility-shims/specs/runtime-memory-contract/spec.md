## ADDED Requirements

### Requirement: Compaction configuration uses explicit dual thresholds
Alan SHALL configure context compaction with
`compaction_soft_trigger_ratio` and `compaction_hard_trigger_ratio`. The retired
single-threshold `compaction_trigger_ratio` field SHALL be unavailable and SHALL
fail normal unknown-field validation.

#### Scenario: Dual thresholds are configured
- **WHEN** valid soft and hard compaction ratios are present in agent
  configuration
- **THEN** Alan validates and applies the two thresholds to compaction
  coordination

#### Scenario: Retired single threshold is configured
- **WHEN** agent configuration contains `compaction_trigger_ratio`
- **THEN** configuration loading fails and identifies the unknown field
- **AND** Alan does not copy that value into either current threshold

#### Scenario: Thresholds are omitted
- **WHEN** neither current compaction threshold is configured
- **THEN** Alan uses the current soft and hard defaults
- **AND** no deprecated single-threshold default participates in resolution
