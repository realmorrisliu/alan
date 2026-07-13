## ADDED Requirements

### Requirement: Legacy connection metadata migrates once
Alan SHALL migrate non-secret legacy connection metadata into the channel
System Store, verify the service-readable result, and delete the legacy file.
Credential bytes SHALL remain in the owning Host credential store and no
compatibility reader SHALL remain.

#### Scenario: Legacy profile is valid
- **WHEN** upgrade finds a valid legacy profile and credential reference
- **THEN** the metadata is imported and verified before the old file is deleted
- **AND** secret bytes are never copied into System Store
