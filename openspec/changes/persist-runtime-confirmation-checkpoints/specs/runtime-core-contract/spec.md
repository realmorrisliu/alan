## ADDED Requirements

### Requirement: Runtime confirmation resumes persist checkpoint records
alan SHALL persist a rollout checkpoint record when a runtime confirmation
control resume resolves a `tool_escalation` or
`effect_replay_confirmation` checkpoint.

#### Scenario: Tool escalation approval is persisted
- **WHEN** a runtime confirmation resume approves a `tool_escalation`
  checkpoint
- **THEN** rollout persistence includes a checkpoint record with the matching
  checkpoint id and checkpoint type
- **AND** recovery can distinguish the synthetic control payload from an
  ordinary user turn

#### Scenario: Effect replay rejection is persisted
- **WHEN** a runtime confirmation resume rejects an
  `effect_replay_confirmation` checkpoint
- **THEN** rollout persistence includes a checkpoint record with the matching
  checkpoint id and checkpoint type
- **AND** the persisted checkpoint records the resolved choice

### Requirement: Runtime confirmation checkpoints link to current tape roots when available
alan SHALL attach the current namespace `machine/tape` checkpoint root hash to a
persisted runtime confirmation checkpoint when the runtime can read that root.

#### Scenario: Current tape checkpoint root is available
- **WHEN** the runtime can read the current namespace `machine/tape` checkpoint
  root while persisting a runtime confirmation checkpoint
- **THEN** the rollout checkpoint record includes that root hash as
  `knowledge_root`

#### Scenario: Current tape checkpoint root is unavailable
- **WHEN** the runtime cannot read the current namespace `machine/tape`
  checkpoint root while persisting a runtime confirmation checkpoint
- **THEN** the runtime still persists the checkpoint record without
  `knowledge_root`
- **AND** the resume flow does not fail solely because the root read failed
