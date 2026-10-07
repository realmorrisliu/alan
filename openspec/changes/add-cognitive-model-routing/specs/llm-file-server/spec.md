## ADDED Requirements

### Requirement: Connections expose finite-choice evaluation independently of generation
A reachable Connection SHALL advertise finite-choice evaluation only when its captured provider callable supports it. Its evaluation allocation SHALL reuse the Connection operation lifecycle with independent versioned request/result DTOs and quota. Unsupported evaluation SHALL fail before provider dispatch and SHALL NOT fall back to generation.

#### Scenario: An evaluation is submitted
- **WHEN** a caller opens a supported Connection's evaluate allocation file ReadWrite and commits a valid choice.v1 document through data clunk
- **THEN** exactly one operation evaluates the original input against unique submitted candidate IDs
- **AND** events returns Selected of a submitted ID or NoMatch with schema and captured provider/model provenance
- **AND** the operation emits no assistant prose, Tool call or command

#### Scenario: Invalid or unsupported request
- **WHEN** schema/version, candidate IDs, document bounds, controls or requested capability are invalid
- **THEN** no provider call starts and the operation is rejected or unavailable through its owning lifecycle
- **AND** generation's current DTO remains unchanged

#### Scenario: Provider response evidence is malformed
- **WHEN** a completed evaluation response has invalid JSON, captured model provenance, candidate distribution or bounded response content
- **THEN** the adapter and llmfs preserve a bounded malformed-evaluation failure distinct from provider unavailability
- **AND** no response body, secret, selection or invalid usage is published as successful evidence
- **AND** the operation is not retried or replaced implicitly

#### Scenario: Evaluation cancellation or deadline
- **WHEN** cancellation or the bounded deadline occurs while waiting for the provider lock or while evaluation is in flight
- **THEN** the operation terminates without later result publication or implicit retry/fallback
- **AND** duplicate commits and Connection replacement cannot cause a second dispatch or change captured authority

#### Scenario: Metering has no billing evidence
- **WHEN** token usage or billing provenance is unavailable
- **THEN** the affected measurement remains unknown rather than an invented zero
- **AND** evaluation uses its own allocation quota without changing generation counts

#### Scenario: Evaluation-only callable rejects generation allocation
- **WHEN** a captured callable supports evaluation but not generation
- **THEN** its capability projection exposes that distinction
- **AND** generation allocation fails before provider dispatch or generation quota reservation

### Requirement: Evaluation profiles preserve Host credential and generation ownership
TypeSafe Connection profiles SHALL use the existing Host credential store and forward finite-choice evaluation through callable wrappers. Each new request SHALL resolve current credential authority. Evaluation-only profiles SHALL NOT replace generation defaults, Process selections or captured generation bindings. Callable publication SHALL retain pinned provider/model provenance without exposing secrets.

#### Scenario: Credential revoked after callable capture
- **WHEN** a Host secret is removed after an evaluation callable was captured
- **THEN** a subsequent evaluation fails before provider dispatch
- **AND** the revoked secret is not recovered from the captured configuration

#### Scenario: Evaluation profile is published
- **WHEN** a valid pinned TypeSafe profile with an available Host credential is published
- **THEN** llmfs evaluate calls reach the evaluation adapter and return pinned provenance
- **AND** choosing that profile as generation default or Process generation selection fails without changing the prior binding
- **AND** saved Agent configuration and Connection metadata omit secret bytes
