## ADDED Requirements

### Requirement: App channel and Alan OS Host channel are paired
Alan for macOS stable and dev SHALL discover only the matching Alan OS Host
endpoint and System Store identity. App singleton lifetime SHALL remain
separate from Alan OS Host singleton lifetime.

#### Scenario: Stable app exits
- **WHEN** the stable app terminates
- **THEN** the stable Alan OS Host remains running
- **AND** the dev channel is unaffected
