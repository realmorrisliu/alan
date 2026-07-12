## MODIFIED Requirements

### Requirement: The request and events use an independent versioned wire DTO
`alan-llmfs` SHALL define its own versioned wire DTO for the request document and
for the stream-event records, decoupled from `alan-llm` internal types. It SHALL
accept only the current explicitly versioned request document and SHALL reject
unversioned or unknown-version request shapes without migration or fallback. It
SHALL map the DTO to and from `alan-llm` (`GenerationRequest` / `StreamChunk`)
internally. The events stream SHALL be a byte-stream record convention (for
example one JSON record per line) per the aP stream model.

#### Scenario: The wire DTO is versioned
- **WHEN** the request or event wire format is defined
- **THEN** it is a versioned DTO owned by `alan-llmfs`, not a re-export of
  `alan-llm` internal structs
- **AND** an `alan-llm` internal refactor does not change the wire format unless
  the DTO version changes

#### Scenario: Current request document is committed
- **WHEN** a caller commits a valid current-version request document to a
  Generation
- **THEN** `alan-llmfs` maps the full document to `GenerationRequest`
- **AND** generation starts only after normal commit validation succeeds

#### Scenario: Unversioned request document is committed
- **WHEN** a caller commits the retired unversioned `{system,user}` request
  shape or a document without the required version discriminator
- **THEN** the Generation commit is rejected before provider dispatch
- **AND** no legacy DTO fallback or migration is attempted

#### Scenario: Unknown request version is committed
- **WHEN** a caller commits a request document with an unsupported version
- **THEN** the Generation commit is rejected with an unsupported-version error
- **AND** no provider request starts
