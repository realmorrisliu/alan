# Spec Delta

## ADDED Requirements

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
