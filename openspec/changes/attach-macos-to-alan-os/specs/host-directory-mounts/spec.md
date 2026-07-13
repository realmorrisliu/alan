## ADDED Requirements

### Requirement: macOS is a Host Mount native adapter
Alan for macOS SHALL observe Host Mount Service requests, present native
directory authorization, and return bounded hostfs export results. Raw Host OS
paths and security-scoped handles SHALL remain in the platform adapter and MUST
NOT appear in Agent-visible grant files.

#### Scenario: Agent requests a read-only directory
- **WHEN** the user approves it in macOS
- **THEN** Host Mount Service receives a read-only export result
- **AND** the Agent sees only its Alan OS mount path and grant metadata
