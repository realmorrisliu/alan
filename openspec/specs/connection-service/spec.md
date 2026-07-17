# connection-service Specification

## Purpose
Defines Connection Service ownership of profile metadata, callable LLM trees,
and bounded native credential requests without exposing Host secrets.

## Requirements

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

### Requirement: Agent Execution Engine consumes mounted connections only
Connection Service SHALL exclusively own connection profile metadata, defaults,
selection, validation status, and publication. Agent Execution Engine MUST NOT
read, write, merge, or select connection profiles; it SHALL invoke only the
callable connection handle mounted into the Agent Process namespace by its
launch context.

#### Scenario: Agent Process begins a transition
- **WHEN** an Agent Process has a callable connection mounted in its namespace
- **THEN** Agent Execution Engine uses that mounted handle for generation
- **AND** it does not open a profile metadata store or resolve a default profile

#### Scenario: Profile selection changes
- **WHEN** an operator changes a default or Process-selected profile
- **THEN** Connection Service validates and publishes the selected callable tree
- **AND** Agent Execution Engine code and state remain unchanged

#### Scenario: Engine ownership validation runs
- **WHEN** repository architecture checks inspect Agent Execution Engine
- **THEN** no profile metadata persistence, merging, default selection, or Host
  credential lookup remains in the engine
