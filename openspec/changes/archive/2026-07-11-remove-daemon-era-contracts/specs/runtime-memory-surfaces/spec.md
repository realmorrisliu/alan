## REMOVED Requirements

### Requirement: Substantive Current Goal Selection
**Reason**: The scenario uses Session emptiness as the selection boundary.
**Migration**: Select the current goal from Agent Machine plan state and substantive turns.

### Requirement: Skill-Authored Semantic Memory Surfaces
**Reason**: Durable change detection is expressed through turn-or-Session state.
**Migration**: Use turns, Agent Process activity, and durable project evidence.

### Requirement: Coherent Fallback Memory Truncation
**Reason**: Truncation provenance permits a source Session identity.
**Migration**: Use source Agent Process, rollout/checkpoint, or file evidence.

### Requirement: Terminal Plan State Refresh
**Reason**: Generated surfaces include Session summary ownership.
**Migration**: Refresh working, episodic, handoff, and daily-note surfaces after terminal Process state is known.

## ADDED Requirements

### Requirement: Runtime memory surfaces use Agent Process evidence
Current-goal, semantic-memory, fallback, handoff, and daily-note surfaces SHALL derive from Agent
Machine plan state, substantive turns, Agent Process activity, and rollout/checkpoint or file
evidence.

#### Scenario: Memory surfaces refresh after terminal state
- **WHEN** a turn or Agent Process changes durable project state and reaches a known terminal state
- **THEN** generated memory surfaces refresh from that state and its evidence
- **AND** truncation provenance names the source Process, rollout/checkpoint, or file
