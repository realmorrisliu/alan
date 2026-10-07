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
