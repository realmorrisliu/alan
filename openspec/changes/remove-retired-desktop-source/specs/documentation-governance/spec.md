## ADDED Requirements

### Requirement: Removed desktop contracts are historical
After desktop source removal, Alan SHALL treat its desktop-only macOS,
shell-core and workspace contracts as retired history, not active maintenance
or deferred GUI delivery. Current plans SHALL preserve standalone CLI/Host
platform security and existing q Skill distribution. Generic executable
packaging SHALL require an independently justified consumer before new work.

#### Scenario: Removed desktop functionality is requested by an old plan
- **WHEN** an old draft references native windows, desktop panels or Swift UI tests
- **THEN** it is not executed as current work
- **AND** removed requirements are not restored by syncing that draft

#### Scenario: CLI uses macOS platform security
- **WHEN** standalone Alan accesses credentials or approved Host Mounts
- **THEN** existing Rust Host/runtime security contracts still apply
- **AND** desktop removal does not erase accounts, credentials, stores or installed apps
