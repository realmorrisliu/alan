## ADDED Requirements

### Requirement: Alan enters the system Shell
Running `alan` SHALL start or attach the matching dedicated Alan OS Host and
enter Alan Shell. It MUST NOT privately boot an Agent runtime or select an Agent
Definition as Host startup behavior.

#### Scenario: User runs alan with no subcommand
- **WHEN** the system Host is ready
- **THEN** the client attaches and presents Alan Shell
- **AND** Agent Processes are spawned or attached from inside the Shell
