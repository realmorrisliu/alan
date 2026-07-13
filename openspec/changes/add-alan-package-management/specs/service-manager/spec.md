## ADDED Requirements

### Requirement: Service Manager supervises Package Service
Service Manager SHALL start Package Service as a required File-Server Service,
grant only its channel System Store binding and required base package artifacts,
and require publication of `/srv/packages` before package-dependent Processes
start. It SHALL mount the normal management tree at `/mnt/packages` and compose
only explicitly selected package and Tool projections into child namespaces.

#### Scenario: Package Service becomes ready
- **WHEN** its Process is running, required first-party packages are installed,
  and the `packages` handle is published
- **THEN** Service Manager marks the unit ready
- **AND** package-dependent Processes may start

#### Scenario: Package Service fails during boot
- **WHEN** Package Service exhausts its restart budget before ready
- **THEN** required boot fails
- **AND** Service Manager does not start package-dependent Processes with a
  compatibility package source

#### Scenario: Package Service exits after readiness
- **WHEN** the Package Service Process exits
- **THEN** `/srv/packages` and dependent mounts are invalidated
- **AND** Service Manager applies the declared restart policy
