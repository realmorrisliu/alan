## ADDED Requirements

### Requirement: Request controls compose with operation and Connection selection
Alan SHALL select a reachable Connection and supported operation before
validating operation-specific request controls. Provider adapters SHALL NOT
decide cognitive roles, grant authority or silently reinterpret generation
controls as evaluation controls.

#### Scenario: Generation uses reasoning effort
- **WHEN** a generation operation requests reasoning effort
- **THEN** Alan validates and normalizes it against that Connection's metadata
- **AND** a control from a previous Connection is not copied blindly

#### Scenario: Evaluation does not support a control
- **WHEN** an evaluation request includes an unsupported generation control
- **THEN** validation rejects it before dispatch
- **AND** the adapter does not silently downgrade or reinterpret the operation

### Requirement: TypeSafe finite-choice adapter preserves bounded evaluation authority
The TypeSafe evaluation adapter SHALL map the caller's finite candidate set to a typed Choice with a distinct abstention option. It SHALL validate returned model identity, selection, distribution and usage, enforce bounded transport without retries or redirects, and keep credentials and response bodies out of errors. This adapter SHALL NOT implement generation by reinterpreting an evaluation request.

#### Scenario: Valid finite-choice result
- **WHEN** the pinned model returns a valid choice from the submitted wire criteria
- **THEN** the adapter returns the original caller candidate ID or NoMatch for abstention
- **AND** original input is preserved and no Tool or command is dispatched

#### Scenario: Untrusted or unavailable provider result
- **WHEN** response provenance, type, candidate identity, distribution, usage or size is invalid, or transport fails
- **THEN** evaluation fails without retry, fallback generation or exposed credential/body diagnostics

#### Scenario: Live qualification is incomplete
- **WHEN** only local HTTP fixtures have passed and no authenticated live probe has passed
- **THEN** ordinary Connection profile publication remains unavailable
- **AND** fixture results do not establish service support or routing qualification
