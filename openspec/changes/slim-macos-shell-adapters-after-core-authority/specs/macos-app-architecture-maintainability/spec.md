## ADDED Requirements

### Requirement: Post-core Swift legacy shell-domain code is cleaned up
After Rust shell core becomes authoritative, the Apple client SHALL remove
Swift implementations of Rust-owned shell-domain behavior from production
sources. Remaining script test helpers SHALL be FFI-backed builders or
platform-only fakes, not duplicate Swift implementations of Rust-owned behavior.

#### Scenario: Cleanup work starts
- **WHEN** a shell adapter slimming PR begins after shell-core authority
- **THEN** it records the Swift production files that still contain Rust-owned
  manifest, reducer/control, action registry, Terminal Profile, settings, or
  materialization behavior
- **AND** it declares which legacy implementations will be removed or moved to
  test support before implementation is considered complete

#### Scenario: Cleanup slice completes
- **WHEN** a cleanup slice moves Swift code that duplicates Rust-owned behavior
- **THEN** the normal macOS app target no longer compiles that implementation as
  production model/controller/service code
- **AND** any remaining Swift test helper is FFI-backed or platform-only support
- **AND** the slice does not add a new shell-core fallback path

#### Scenario: Cleanup boundary is missed
- **WHEN** a cleanup slice finishes while production Swift still carries the
  promised Rust-owned legacy implementation
- **THEN** the PR records the remaining blocker in `clients/apple/ARCHITECTURE.md`
- **AND** the OpenSpec task for that legacy cleanup remains incomplete

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

### Requirement: Obsolete Swift parity helpers are removed
The Apple client SHALL remove Swift implementations that only duplicate
Rust-owned portable shell-domain behavior for parity validation. Swift script
tests may keep explicit FFI-backed builders or platform-effect fakes, but they
must not keep a second Swift registry, manifest materializer, reducer, or split
tree implementation as fallback behavior.

#### Scenario: Action registry parity code is removed
- **WHEN** Rust action contract tests and FFI adapter tests cover action
  descriptors, target resolution, availability, shortcuts, and effects
- **THEN** Swift standard registry tables and resolver fixtures are removed from
  production and script-support sources
- **AND** `check-shell-contracts.sh` continues to reject production references
  to Swift registry implementations as shell-core fallback

#### Scenario: Manifest parity helpers are removed
- **WHEN** Swift manifest defaulting, pruning, migration, or materialization
  helpers duplicated Rust-owned behavior
- **THEN** they are removed from production and script-support sources
- **AND** Rust shell-core or FFI tests cover the portable behavior

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
- **WHEN** a cleanup or split removes or narrows a warning from the architecture report
- **THEN** `clients/apple/ARCHITECTURE.md` is updated in the same PR with the
  new warning count, remaining owners, and next follow-up boundary
