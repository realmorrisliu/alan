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

### Requirement: Child Agent Processes preserve the selected Connection profile
Child Agent Process launch SHALL preserve the effective explicit Connection
profile unless the child definition or launch request selects a different one.
It MUST NOT silently reselect the Connection Service default.

#### Scenario: Parent uses a non-default explicit profile
- **GIVEN** a parent Agent Process uses an explicit profile that is not the service default
- **WHEN** it launches a child without a Connection override
- **THEN** child setup and runtime startup use the same explicit profile
- **AND** absence of a service default does not make child startup fail

#### Scenario: Child definition selects a different profile
- **GIVEN** a child Agent definition selects a profile different from its parent's profile
- **WHEN** the Agent Runtime Service launches the child
- **THEN** it resolves the child-selected profile before constructing the child's LLM client
- **AND** child setup and runtime startup use the same resolved provider settings
