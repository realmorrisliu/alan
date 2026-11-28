# service-manager Specification

## Purpose
Defines Service Manager as the first system Process and sole owner of bounded
alan9 service and Root Agent lifecycle.

## Requirements

### Requirement: Service Manager is the first system Process
The foreground alan9 composition SHALL create Kernel and start one Service
Manager Process per invocation. Service Manager SHALL be the sole owner of later
system service and Root Agent Process lifecycle; the renderer and Host adapters
MUST NOT retain fallback boot or supervision. File-Server Services SHALL NOT require
separate operating-system background processes.

#### Scenario: System boot begins
- **WHEN** an invocation's Kernel is ready
- **THEN** its composition starts Service Manager
- **AND** all other required system Processes are started by that Service Manager

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
system degraded and await explicit retry. The foreground Root Agent SHALL use
`always` only within its live invocation. A replacement SHALL recover the latest
rollout path held by that instance's Agent Runtime Service, and MUST NOT reread
the product-wide selector. A new invocation SHALL read that selector only when
the user explicitly requests `alan --resume`; no background Host survives the
foreground invocation. If an instance has no durable rollout, its replacement
SHALL start fresh without consulting the product-wide selector. Before loading
a replacement from a prior Root rollout, Agent Runtime Service SHALL wait for
the prior Agent Machine's recorder to flush and terminate.

#### Scenario: Root Agent crash loops
- **WHEN** the foreground Root Agent fails after readiness
- **THEN** Service Manager replaces it from that invocation's latest durable
  rollout while restart attempts remain within budget
- **AND** failed recovery leaves the selected durable evidence available for
  retry or explicit recovery

#### Scenario: Root replacement waits for pending rollout writes
- **WHEN** Root exits with rollout records still queued by non-blocking writes
- **THEN** Agent Runtime Service flushes and terminates the prior recorder
  before replacement recovery reads that rollout
- **AND** a recorder flush failure prevents replacement recovery from that rollout

#### Scenario: Root Agent restarts while another invocation is active
- **WHEN** the foreground Root Agent fails after readiness while another
  invocation has published a different product-wide rollout
- **THEN** Service Manager replaces it from its own invocation's latest durable
  rollout
- **AND** its recovery is independent of the other invocation's selector write
- **AND** each replacement remains subject to the bounded restart budget

#### Scenario: Root Agent has no durable rollout
- **WHEN** best-effort startup succeeds without creating a durable rollout and
  the Root Agent later restarts
- **THEN** its replacement starts without restoring any product-wide rollout
- **AND** a later successful durable startup becomes that invocation's recovery source

#### Scenario: Another required service exhausts its restart budget
- **WHEN** a required non-Root service exceeds its configured retry budget
- **THEN** boot fails before readiness or the running instance becomes degraded
- **AND** retries remain bounded within that invocation

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

### Requirement: Service handles isolate restarts through generation-tracked SupervisedHandle
All `/srv/*` service mounts supervised by `ServiceManager` SHALL sit behind a
generation-tracked `SupervisedHandle` rather than a `package`-only special case.
Rebinding after restart SHALL increment `generation`, and operations on
pre-restart fids SHALL fail cleanly with `ErrorCode::Io`.

#### Scenario: Pre-restart fid fails cleanly after service rebound
- **WHEN** a client holds a fid walked under `/srv/<name>` prior to a unit restart and uses it after `SupervisedHandle` rebinds to generation `N + 1`
- **THEN** the stale fid call returns `ErrorCode::Io` instead of mutating the new service lifetime
- **AND** a fresh `walk` resolves cleanly to the restarted service

### Requirement: Service Manager uses event-driven supervision, reverse shutdown, and opt-in RestForOne
`ServiceManager` SHALL observe unit exits via `/proc/<pid>/wait` instead of
`25ms`/`10ms` polling, shut down units on `stop()` in strict reverse `after`
dependency order, default runtime recovery to `OneForOne`, and support opt-in
`RestForOne` per `BootUnit`.

#### Scenario: Reverse-topological shutdown stops dependents before prerequisites
- **WHEN** `ServiceManager::stop()` executes across a `BootManifest` dependency graph
- **THEN** downstream units (such as `local-entry` and `agent-runtime`) stop before their upstream `after` prerequisites (such as `package` and `connection`)

#### Scenario: OneForOne self-heals a leaf service without killing live sessions
- **WHEN** a `OneForOne` unit behind `SupervisedHandle` (such as `package`) crashes within its restart budget
- **THEN** `ServiceManager` restarts only that unit cleanly without terminating active `root-agent` or `local-entry` sessions
- **AND** a unit configured with `supervision = "rest-for-one"` restarts its downstream dependents in topological order

### Requirement: Unit supervision telemetry is projected under /mnt/service-manager/units/<name>/
`ServiceManager` SHALL project live supervision state (`generation`, `restarts`,
`backoff_until`, and `last_exit_reason`) as readable files under
`/mnt/service-manager/units/<name>/` while preserving existing unit files such
as `pid` and `status`.

#### Scenario: Unit files reflect restart generation and exit reason
- **WHEN** a supervised unit crashes and restarts cleanly
- **THEN** reading `/mnt/service-manager/units/<name>/generation`, `restarts`, `backoff_until`, and `last_exit_reason` reflects the incremented generation, count, and recorded exit cause
