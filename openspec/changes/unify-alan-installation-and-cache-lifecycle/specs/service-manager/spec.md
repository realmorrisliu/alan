## MODIFIED Requirements

### Requirement: Root recovery belongs to its foreground instance
Root replacement SHALL use its instance's latest durable rollout without rereading
the product-store recovery selector. A new invocation SHALL read that selector only for explicit
`alan --resume`. An instance without a durable rollout SHALL restart fresh.
Agent Runtime Service SHALL flush and terminate the prior Machine recorder before
loading its replacement; a flush failure SHALL prevent that recovery.

#### Scenario: Root restarts while another invocation publishes a rollout
- **WHEN** Root fails after readiness and another invocation has changed the
  product-store recovery selector
- **THEN** replacement uses its own instance's latest durable rollout
- **AND** replacement remains subject to the bounded restart policy

#### Scenario: Root exits with pending rollout writes
- **WHEN** the prior Root recorder has pending writes
- **THEN** recovery waits for its successful flush and termination
- **AND** flush failure prevents loading a replacement from that rollout

#### Scenario: Root has no durable rollout
- **WHEN** best-effort startup creates no durable rollout and Root later restarts
- **THEN** replacement starts fresh without consulting the product-store recovery selector
- **AND** a later successful durable startup becomes the instance's recovery source

### Requirement: Service Manager supervises Package Service

Service Manager SHALL start Package Service as a required File-Server Service,
grant only its explicitly supplied System Store binding and required preinstalled package
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
