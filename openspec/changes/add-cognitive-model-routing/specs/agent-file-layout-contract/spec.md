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

### Requirement: Evaluation snapshots cannot become a second write authority
The `machine/evaluation` file SHALL project the latest acknowledged Machine observation as a bounded versioned snapshot. Agent Runtime Service SHALL publish it through the owning AgentFS in-process update path. The public aP node SHALL reject write-intent opens and direct writes. Historical evidence SHALL remain in rollout/checkpoint storage.

#### Scenario: No observation exists
- **WHEN** a fresh Machine has no acknowledged evaluation evidence
- **THEN** its projection reports unknown or no observation rather than successful selection

#### Scenario: Advice is recovered into another Process
- **WHEN** explicit recovery reconstructs evaluation evidence under a new PID
- **THEN** the projection retains the source observation and captured Connection identity
- **AND** publication neither dispatches an evaluator nor settles ordinary queued input
