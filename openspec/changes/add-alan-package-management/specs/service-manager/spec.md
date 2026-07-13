# service-manager Delta

## ADDED Requirements

### Requirement: Service Manager supervises Package Service

Service Manager SHALL start Package Service as a required File-Server Service,
grant only its channel System Store binding and required preinstalled package
sources, and require publication of `/srv/package` before package-dependent
Processes become ready. It SHALL compose only explicitly referenced immutable
package projections into child namespaces.

#### Scenario: Package Service becomes ready

- **WHEN** its Process is running, required first-party packages are seeded,
  and the `package` handle is published
- **THEN** Service Manager marks the unit ready
- **AND** package-dependent Shell and Agent Processes may start

#### Scenario: Package Service fails during boot

- **WHEN** Package Service exhausts its restart budget before ready
- **THEN** required boot fails
- **AND** Service Manager does not start package-dependent Processes with a
  compatibility package source

#### Scenario: Package Service exits after readiness

- **WHEN** the Package Service Process exits
- **THEN** `/srv/package` and new package resolution become unavailable
- **AND** Service Manager applies the declared restart policy

### Requirement: Service Manager provides the Quartermaster Process image

Service Manager SHALL bind `q` at `/bin/q` and launch it through the ordinary
Process runner. The `q` Process SHALL receive the Package Service file handle
and only the invoking Process namespace authority needed to import an explicit
source path. Service Manager MUST NOT add a Host package-management API.

#### Scenario: Alan Shell runs q

- **WHEN** Alan Shell spawns `/bin/q` through `/proc/clone`
- **THEN** Service Manager runs the `q` Process image with Package Service
  connectivity
- **AND** status, output, and exit state remain ordinary `/proc/<pid>` files
