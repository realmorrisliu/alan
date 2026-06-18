## ADDED Requirements

### Requirement: Post-core shell adapter debt is measured and burned down
After Rust shell core becomes authoritative, the Apple client SHALL treat the
remaining large Swift shell adapter files as measurable architecture debt with a
documented baseline, target warning budget, and per-slice owner boundary.

#### Scenario: Slimming work starts
- **WHEN** a shell adapter slimming PR begins after shell-core authority
- **THEN** it records the current `check-architecture-maintainability.sh`
  warning count and the named files responsible for shell-core adapter,
  controller, model, profile, settings, manifest, and control-command debt
- **AND** it declares the target warning count or named warnings that the slice
  will remove before implementation is considered complete

#### Scenario: Slimming slice completes
- **WHEN** a slimming slice moves code out of a large Swift shell file
- **THEN** the architecture warning count decreases or the debt ledger narrows
  the remaining warning to a more precise owner and follow-up boundary
- **AND** the slice does not add a new architecture warning

#### Scenario: Warning budget is missed
- **WHEN** a slimming slice finishes without reducing the promised warning
  count or named warning
- **THEN** the PR records the remaining blocker in `clients/apple/ARCHITECTURE.md`
- **AND** the OpenSpec task for that warning remains incomplete

### Requirement: Shell-core FFI adapter is split into narrow operation owners
The Swift shell-core bridge SHALL keep a small public facade while moving
operation-family implementation details into focused internal owners for
dynamic loading, envelope send/receive, portable state materialization,
manifest operations, reducer operations, control commands, action registry,
settings summaries, and Terminal Profile resolution.

#### Scenario: FFI operation implementation changes
- **WHEN** a developer changes shell-core request encoding, response decoding,
  error mapping, or materialization for one operation family
- **THEN** the change is located in that operation family's adapter owner or a
  shared envelope/materialization helper
- **AND** the public `ShellCoreFFIAdapter` surface remains a thin facade for
  existing macOS call sites

#### Scenario: FFI adapter split is validated
- **WHEN** an operation-family adapter is moved out of the coarse bridge file
- **THEN** the shell-core FFI adapter script and any affected operation-family
  script run successfully
- **AND** shell contract validation still rejects optional Swift domain fallback
  for core-owned operations

### Requirement: Shell host controller narrows to orchestration
`ShellHostController.swift` SHALL stop owning shell-core-backed domain routing
and large platform helper implementations directly. It SHALL coordinate
observable state and delegate manifest startup, persistence scheduling, action
dispatch, reducer command routing, control response adoption, and platform
metadata preservation to named collaborators.

#### Scenario: Manifest startup is extracted
- **WHEN** workspace manifest startup or persistence behavior changes
- **THEN** the shell host controller delegates loading, shell-core materializing,
  pruning, failure diagnostics, and write scheduling to a manifest/startup
  collaborator
- **AND** the controller does not regain a Swift implementation of portable
  manifest defaulting, pruning, migration, or materialization

#### Scenario: Shell action or reducer command routing is extracted
- **WHEN** shell action dispatch, reducer-backed commands, or control response
  adoption changes
- **THEN** shell-core invocation and result projection live in a named
  coordinator or service rather than in unrelated controller methods
- **AND** Swift platform effects remain explicit and separate from portable
  shell-domain validation

#### Scenario: Controller split is validated
- **WHEN** a controller-owned shell-core path is moved
- **THEN** the affected focused shell script runs with shell contract
  validation and architecture maintainability validation

### Requirement: Fixture-only Swift shell-domain helpers are test-support only
The Apple client SHALL move Swift implementations that remain only for parity
fixtures or script tests out of production-facing model/controller files, or
guard them so the normal macOS app target cannot use them as runtime fallback
behavior.

#### Scenario: Action registry fixture code remains
- **WHEN** a Swift action registry table is still needed for fixture export or
  parity comparison
- **THEN** it lives in explicit test support or is guarded from production
  runtime use
- **AND** `check-shell-contracts.sh` continues to reject production references
  to the Swift registry as a shell-core fallback

#### Scenario: Manifest parity helpers remain
- **WHEN** Swift manifest defaulting, pruning, migration, or materialization
  helpers remain only for parity fixtures
- **THEN** they are excluded from normal app builds or moved into script test
  support
- **AND** Rust shell-core or FFI tests cover the portable behavior before the
  production-facing helper is removed or hidden

### Requirement: Behavior-preserving splits keep focused verification
Each shell adapter slimming slice SHALL pair moved ownership with focused tests
for the behavior being moved, plus architecture validation and shell-core
authority guards.

#### Scenario: Manifest or runtime metadata owner moves
- **WHEN** manifest startup, persistence, or runtime metadata preservation is
  moved to a new owner
- **THEN** the workspace-manifest, runtime-metadata, shell-contract, and
  architecture-maintainability scripts run successfully

#### Scenario: Action, settings, profile, or control owner moves
- **WHEN** action registry, settings summary, Terminal Profile, local control,
  or shell-core adapter projection code moves to a new owner
- **THEN** the corresponding focused Swift script runs successfully
- **AND** Rust shell-core or shell-core-ffi tests still cover portable domain
  behavior

#### Scenario: Warning ledger changes
- **WHEN** a split removes or narrows a warning from the architecture report
- **THEN** `clients/apple/ARCHITECTURE.md` is updated in the same PR with the
  new warning count, remaining owners, and next follow-up boundary
