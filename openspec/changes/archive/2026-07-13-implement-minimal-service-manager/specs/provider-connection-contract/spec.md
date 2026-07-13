## ADDED Requirements

### Requirement: Connection authority is file-service owned
Connection Service SHALL be the only owner of profile metadata, defaults,
selection, validation status, and callable connection publication. Host
adapters SHALL own only native login and secret storage.

#### Scenario: Host adapter restarts
- **WHEN** Connection Service remains running
- **THEN** profile identity and non-secret settings remain authoritative
- **AND** the adapter can reconnect without reconstructing profiles
