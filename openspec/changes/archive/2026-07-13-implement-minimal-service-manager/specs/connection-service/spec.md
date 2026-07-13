## ADDED Requirements

### Requirement: Connection Service owns profile metadata
Connection Service SHALL own provider/model settings, profile identity,
defaults, Process selection, validation status, and publication of callable LLM
connection trees. Metadata SHALL be channel-scoped in System Store.

#### Scenario: Agent selects a profile
- **WHEN** its launch context passes an installed Connection reference
- **THEN** the Agent Process receives the corresponding callable LLM tree
- **AND** no Host config file is read

### Requirement: Host adapters own credential secrets
Browser/device login and secret storage SHALL remain in platform adapters.
Connection Service SHALL receive only opaque credential references and MUST NOT
expose secret bytes through its namespace.

#### Scenario: Browser login completes
- **WHEN** a Host adapter stores credentials successfully
- **THEN** it returns an opaque reference to Connection Service
- **AND** profile files reveal no credential material

### Requirement: Native actions are request files
Connection Service SHALL expose pending native login/credential operations and
their bounded responses as files; a renderer adapter MUST NOT become profile
authority.

#### Scenario: No renderer can answer login
- **WHEN** a native request remains unanswered
- **THEN** the profile reports pending/unavailable status
- **AND** no hidden Host-side profile is created
