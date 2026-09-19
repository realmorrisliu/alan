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
