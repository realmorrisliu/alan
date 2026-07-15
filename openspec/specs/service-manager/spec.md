# service-manager Specification

## Purpose
Defines Service Manager as the first system Process and sole owner of bounded
Alan OS service and Root Agent lifecycle.
## Requirements
### Requirement: Service Manager is the first system Process
Alan OS Host SHALL create Kernel and start one Service Manager Process. Service
Manager SHALL be the sole owner of later system service and Root Agent Process
lifecycle; Host MUST NOT retain fallback boot or supervision.

#### Scenario: System boot begins
- **WHEN** Kernel is ready
- **THEN** Host starts Service Manager
- **AND** all other required system Processes are started by Service Manager

### Requirement: Boot Units use a bounded schema
Service Manager SHALL read system-package-owned units from `/lib/boot`. A unit
SHALL contain executable, descriptors/mounts, ordering, required flag, timeout,
restart enum, and published handles; arbitrary shell, environment templating,
user units, and dynamic reload MUST be rejected.

#### Scenario: Unit contains shell script
- **WHEN** a unit requests arbitrary shell execution
- **THEN** Service Manager rejects the unit before launch

### Requirement: Service readiness uses proc and srv
A File-Server Service SHALL be ready only while its Process is running in
`/proc` and all unit-declared handles are published in `/srv`. Service Manager
SHALL expose unit PID, attempts, status, errors, degraded state, and retry in
its own tree.

#### Scenario: Service exits after publication
- **WHEN** a ready service Process exits
- **THEN** its handles are invalidated
- **AND** Service Manager applies its restart policy

### Requirement: Restart policy is bounded
Service Manager SHALL support only `never`, `on-failure`, and `always`, with
bounded exponential backoff, restart budget, and stable reset window. Required
budget exhaustion before ready SHALL fail boot; afterward it SHALL mark the
system degraded and await explicit retry.

#### Scenario: Root Agent crash loops
- **WHEN** the `always` Root Agent unit exhausts its restart budget
- **THEN** the system becomes degraded
- **AND** Service Manager does not restart it without bound

### Requirement: Service Manager supervises Package Service

Service Manager SHALL start Package Service as a required File-Server Service,
grant only its channel System Store binding and required preinstalled package
sources, and require publication of `/srv/package` before package-dependent
Processes become ready. It SHALL compose only explicitly referenced immutable
package handles into child namespaces and descriptors. It MUST NOT translate a
package handle into a Host Mount grant for Agent Runtime Service.

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
