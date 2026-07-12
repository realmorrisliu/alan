## ADDED Requirements

### Requirement: Agent input records declare routing explicitly
An input operation written to an Agent Process SHALL use the canonical
`type: "input"` record and SHALL include an explicit `mode`. Alan SHALL NOT
accept the retired `type: "steer"` alias or infer a mode when the field is
absent.

#### Scenario: Explicit input is submitted
- **WHEN** a caller submits `type: "input"` with a supported explicit mode and
  valid parts
- **THEN** the Agent Process routes the input according to that mode

#### Scenario: Input mode is absent
- **WHEN** a caller submits `type: "input"` without `mode`
- **THEN** the record is rejected as malformed
- **AND** Alan does not infer `steer` or any other routing behavior

#### Scenario: Retired steer operation is submitted
- **WHEN** a caller submits an operation with `type: "steer"`
- **THEN** the record is rejected as an unsupported operation shape
- **AND** the caller must resubmit canonical explicit input
