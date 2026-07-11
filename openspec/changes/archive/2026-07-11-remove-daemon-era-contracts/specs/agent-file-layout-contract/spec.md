## ADDED Requirements

### Requirement: Agent Machine persistence is Process-owned
Agent Runtime Service SHALL persist and restore Agent Machine tape, transition state, checkpoints,
requests, actions, and renderer projections through files owned by the Agent Process and its
durable backing stores. The Process path and durable record identifiers SHALL be sufficient to
locate and interpret that state.

#### Scenario: An Agent Process resumes durable machine state
- **WHEN** Agent Runtime Service restores an Agent Machine from durable rollout or checkpoint files
- **THEN** the restored state is associated with the concrete Agent Process and AgentFS layout
- **AND** the rollout or checkpoint retains the durable provenance needed to interpret the state

### Requirement: Agent Machine confirmation state is file-backed
Runtime confirmation requests and decisions SHALL be recorded through Agent Machine checkpoint,
request, action, and tape files with Process-visible provenance.

#### Scenario: A confirmation decision resumes execution
- **WHEN** an authorized client writes a confirmation decision through the owning request or control
  file
- **THEN** Agent Runtime Service records the decision against the current tape/checkpoint state
- **AND** execution resumes from the recorded decision and Agent Machine state
