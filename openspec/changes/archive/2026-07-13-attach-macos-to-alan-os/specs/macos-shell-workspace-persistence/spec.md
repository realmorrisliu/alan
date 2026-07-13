## ADDED Requirements

### Requirement: Shell persistence stores Agent Attachments narrowly
The macOS shell manifest SHALL store Agent Process boot ID, PID, caller-held
stream offsets, and presentation needed to restore an Agent ContentInstance. It
MUST NOT store Agent runtime state, Host socket objects, raw Host paths, secrets,
or claim Process continuity.

#### Scenario: Manifest restores after app restart
- **WHEN** Alan OS boot identity and Process still match
- **THEN** the ContentInstance reattaches from its stored offsets

#### Scenario: Manifest refers to a prior Host boot
- **WHEN** the boot identity no longer matches
- **THEN** the restored content is marked unavailable
- **AND** the manifest does not redirect it to a new PID
