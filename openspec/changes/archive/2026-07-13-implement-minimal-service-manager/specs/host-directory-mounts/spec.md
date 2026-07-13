## ADDED Requirements

### Requirement: Host Mount projection is service-mediated
Alan OS SHALL route all runtime Host directory authorization, hostfs export,
live namespace projection, revocation, and sandbox derivation through Host Mount
Service. Host renderers MAY answer native authorization requests but MUST NOT
maintain a second grant registry.

#### Scenario: CLI authorizes a path
- **WHEN** CLI Host Command Plane approves a Host directory
- **THEN** its adapter returns the export to Host Mount Service
- **AND** only the service publishes Alan OS-visible grant state
