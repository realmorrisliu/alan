## MODIFIED Requirements

### Requirement: Provider and connection vocabulary is Process-shaped
Alan SHALL distinguish provider family, provider descriptor, credential reference,
connection profile, default profile, resolved connection, and Process connection
binding. A Process connection binding SHALL associate one Agent Process with its
resolved provider/profile/credential identity and SHALL contain no secret material.
Later default changes SHALL NOT mutate a running Process binding. An explicit
user-requested model selection MAY change the model for subsequently admitted
inputs only after Connection Service validates the selection and Agent Runtime
Service confirms installation at a serialized admission boundary. Already-admitted
inputs, including queued inputs, SHALL retain their captured callable binding and
resolved request controls through dispatch and explicit recovery.

#### Scenario: An Agent Process resolves a connection
- **WHEN** Alan spawns an Agent Process whose definition, launch request, or operator default selects a connection profile
- **THEN** it resolves one concrete provider/model/credential reference for that Process
- **AND** later default changes do not mutate the running Process binding

#### Scenario: User selects a model with earlier admitted work
- **WHEN** Connection Service validates an explicit model selection and Agent Runtime Service confirms its installation at a serialized admission boundary
- **THEN** subsequently admitted inputs capture the confirmed model binding
- **AND** inputs admitted before that boundary retain their earlier callable binding and resolved controls
- **AND** selection does not dispatch, discard, or resume earlier queued work

#### Scenario: Steering retains its admitted binding
- **WHEN** steering joins active work with the same captured callable binding and resolved controls
- **THEN** it remains active-loop replanning under that captured binding despite a later next-input selection

#### Scenario: Incompatible steering cannot mix bindings
- **WHEN** an admitted steering input has a different captured binding or resolved controls from active work
- **THEN** Alan rejects it with its exact submission identity through the existing durable removal and failed settlement
- **AND** active work, Tape, pending Tools and ordinary queue state remain unchanged
- **AND** Alan does not remap either binding, convert steering into future queued work or resume paused work

#### Scenario: Recovered binding cannot be restored
- **WHEN** an explicitly recovered input refers to a previously captured callable binding that cannot be restored
- **THEN** Alan reports the unavailable binding without dispatching that input through a different model
- **AND** current project authority remains a separate execution prerequisite

### Requirement: Connection authority is file-service owned
Connection Service SHALL be the only owner of profile metadata, defaults,
selection, validation status, and callable connection publication. Host adapters
SHALL own only native login and secret storage. Process model selection SHALL use
an authorized Connection-owned catalog and validation path without mutating the
profile default or credential identity. Agent Runtime Service SHALL install the
confirmed callable binding before publishing successful selection. A renderer
SHALL NOT independently resolve profiles, invent catalog entries, recompute
request-control defaults, or treat a changed label as binding success.

#### Scenario: Host adapter restarts
- **WHEN** Connection Service remains running
- **THEN** profile identity and non-secret settings remain authoritative
- **AND** the adapter can reconnect without reconstructing profiles

#### Scenario: Catalog or model selection is unavailable
- **WHEN** the active Connection catalog is unavailable or model validation fails
- **THEN** selection reports an actionable failure and preserves the last confirmed binding
- **AND** no provider request or profile/default mutation occurs to simulate success

#### Scenario: Callable installation fails
- **WHEN** a validated selection cannot be installed at its serialized admission boundary
- **THEN** the last confirmed callable remains effective for new input
- **AND** Alan publishes a failed selection instead of a success label
