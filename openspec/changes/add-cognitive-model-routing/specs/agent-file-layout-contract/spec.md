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


### Requirement: Work results project acknowledged Machine completion
AgentFS SHALL expose `machine/work` as a read-only version-1 document with
`work` null before any observation and the latest acknowledged work snapshot
otherwise. A snapshot SHALL contain `version`, `work_id`, `source_rollout_id`,
`request_sha256`, `state` and spent model attempt counts. The full validated
request and candidate evidence SHALL remain in the owning rollout snapshot. State SHALL distinguish started, completed, waiting, failed,
cancelled and interrupted. Completed work SHALL include `owner` and citations
with namespace path, inclusive line range and SHA-256 of the captured range bytes;
waiting work SHALL include an owned request reference and boundary reason.
Completed result SHALL be at most 8 KiB; the bounded nonterminal recovery snapshot
MAY retain the validated request and candidate evidence within the existing 1 MiB
document ceiling. Machine SHALL acknowledge rollout evidence before publication;
AgentFS SHALL neither validate work history nor acquire a second write authority.
`machine/evaluation` SHALL retain separate model advice provenance; Action results
SHALL remain Tool completion metadata. Work completion SHALL NOT imply Process exit
or manufacture an assistant message in `io/output`.

#### Scenario: A client attempts to alter a work result
- **WHEN** a client opens `machine/work` with write intent or directly writes it
- **THEN** AgentFS rejects the write and preserves the Machine-owned snapshot

#### Scenario: Completed work is recovered
- **WHEN** explicit recovery reconstructs an acknowledged completed work under a new PID
- **THEN** its work UUID, source rollout identity, owner and citations remain unchanged
- **AND** recovery publishes no synthetic answer and dispatches no model call
