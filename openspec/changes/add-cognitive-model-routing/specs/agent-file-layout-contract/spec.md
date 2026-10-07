## ADDED Requirements

### Requirement: Evaluation observations project existing Machine evidence
AgentFS SHALL expose evaluation identity, schema, captured Connection/model and terminal or waiting outcome as read-only Machine state. Agent Machine SHALL own transition updates and rollout/checkpoint SHALL retain durable evidence. Evaluation SHALL NOT introduce a second Tape, mutable result authority or Process kind.

#### Scenario: An evaluation completes without prose
- **WHEN** the Machine receives a validated structured evaluation result
- **THEN** the result and provenance are observable through its Machine projection and durable execution evidence
- **AND** io/output need not manufacture an assistant response
- **AND** Process exit remains distinct from evaluation completion

#### Scenario: External writer attempts to change advice
- **WHEN** a client writes to the evaluation observation
- **THEN** the write is rejected
- **AND** only Machine-owned controls can advance the current transition
