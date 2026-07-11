# runtime-memory-surfaces Specification

## Purpose
Define runtime memory and handoff surface behavior: agent-authored semantic
continuation summaries, substantive fallback goal selection, coherent fallback
truncation, terminal turn-state refresh, and rollout references when compact
surfaces omit detail.
## Requirements
### Requirement: Rollout Remains Source Of Truth
Memory surfaces SHALL stay compact continuation aids and point to rollouts when detail is omitted.

#### Scenario: Detail exceeds memory budget
- **WHEN** recent conversation detail exceeds the memory surface budget
- **THEN** the memory surface keeps a coherent summary and identifies where to inspect the raw rollout for full detail

### Requirement: Runtime memory surfaces use Agent Process evidence
Current-goal, semantic-memory, fallback, handoff, and daily-note surfaces SHALL derive from Agent
Machine plan state, substantive turns, Agent Process activity, and rollout/checkpoint or file
evidence.

#### Scenario: Memory surfaces refresh after terminal state
- **WHEN** a turn or Agent Process changes durable project state and reaches a known terminal state
- **THEN** generated memory surfaces refresh from that state and its evidence
- **AND** truncation provenance names the source Process, rollout/checkpoint, or file
