# macos-app-architecture-maintainability Specification

## Purpose
Define maintainable native Apple client source organization, SwiftUI/AppKit
boundaries, service/model ownership, and validation expectations for macOS app
architecture changes.
## Requirements
### Requirement: SwiftUI scene roots compose focused feature views
SwiftUI scene and root view files SHALL primarily compose stable layout,
selection, and feature views. They MUST NOT accumulate unrelated design tokens,
window coordination, command routing, inspector/debug panels, service clients,
or platform bridge implementations.

#### Scenario: macOS shell root is edited
- **WHEN** a developer changes the default macOS shell layout
- **THEN** the root view remains a readable composition of sidebar, workspace,
  command, and optional utility surfaces, with feature-specific UI implemented
  in dedicated view files

#### Scenario: App commands are edited
- **WHEN** a developer changes menu or keyboard command ownership
- **THEN** command definitions and command routing live in app or shell command
  files rather than being buried in unrelated view body code

### Requirement: AppKit bridges are narrow and named
The Apple client SHALL isolate AppKit bridge code behind small, named wrappers
or coordinators for the specific desktop behavior they own, while keeping
unrelated SwiftUI views free of ambient `NSWindow`, `NSView`, `NSApp`, socket,
or process-management details.

#### Scenario: Window placement changes
- **WHEN** hidden-titlebar placement, minimum size, traffic-light metrics, or
  primary-window focusing behavior changes
- **THEN** the implementation is owned by an app/window support component rather
  than by the macOS shell root view file

#### Scenario: Material background changes
- **WHEN** a SwiftUI view needs native material rendering
- **THEN** the `NSVisualEffectView` bridge is isolated behind a reusable material
  wrapper or support component

#### Scenario: Terminal host bridge changes
- **WHEN** terminal first-responder, hit-testing, IME, pointer, keyboard, or
  Ghostty attachment behavior changes
- **THEN** the AppKit terminal host keeps those behaviors behind the terminal
  host boundary and does not leak AppKit ownership through unrelated SwiftUI
  views

### Requirement: Terminal host collaborators have explicit ownership
The terminal host implementation SHALL separate runtime attachment, overlay
presentation, input routing, window observation, metadata publishing, and
surface coordination into explicit collaborators when those responsibilities
become non-trivial.

#### Scenario: Terminal input routing changes
- **WHEN** keyboard, IME, paste, pointer, scroll, or terminal search routing is
  modified
- **THEN** the change is reviewable in terminal input/surface collaborators
  without requiring a full audit of overlay layout or window observation code

#### Scenario: Runtime snapshot publication changes
- **WHEN** terminal runtime metadata publication changes
- **THEN** the owning component clearly distinguishes snapshot construction from
  AppKit layout and visible overlay presentation

### Requirement: Control-plane implementation separates IPC, execution, and persistence
The shell control-plane implementation SHALL keep protocol DTOs, local command
execution, socket serving, file polling, state merging, event persistence, and
diagnostics in reviewable ownership units.

#### Scenario: Socket transport changes
- **WHEN** local socket request size, timeout, accept loop, or client response
  behavior changes
- **THEN** the change is isolated to the socket transport owner and does not
  require reviewing shell mutation semantics

#### Scenario: Local command execution changes
- **WHEN** a shell control command changes how it mutates shell state or applies
  side effects
- **THEN** the local command executor owns the behavior separately from socket
  read/write and file-polling code

#### Scenario: Persistence diagnostics change
- **WHEN** state, event, command, or binding persistence diagnostics change
- **THEN** the persistence/event owner can be reviewed independently from IPC
  request parsing

### Requirement: Large files have planned ownership boundaries
The Apple client SHALL avoid large multi-responsibility Swift files as the
stable end state. When a file remains large or in a transitional owner during
migration, the owning change SHALL document the intended split and avoid adding
unrelated responsibilities to that file.

#### Scenario: Large file receives new behavior
- **WHEN** a developer adds behavior to an existing large Apple client file
- **THEN** the change either places the behavior in the target owner file or
  documents why the temporary location is still compatible with the migration
  plan

#### Scenario: Refactor slice completes
- **WHEN** a behavior-preserving architecture refactor slice is completed
- **THEN** the resulting file ownership makes future changes narrower to review
  than the previous large-file organization

### Requirement: Architecture migration debt is explicit and bounded
The Apple client SHALL keep known architecture-maintainability warnings visible
as tracked migration debt until they are resolved by focused refactor slices.
Known debt MUST identify the affected owner or file, the intended boundary, and
whether the current architecture gate treats it as non-blocking.

#### Scenario: Architecture report has warnings
- **WHEN** `check-architecture-maintainability.sh` completes in report mode with
  warnings
- **THEN** `clients/apple/ARCHITECTURE.md` records the current warning classes
  and explains why they remain non-blocking migration debt

#### Scenario: New architecture warning appears
- **WHEN** a change introduces a new architecture-maintainability warning or
  broadens an existing one
- **THEN** the change either resolves the warning in the target owner or updates
  the migration debt record with a concrete follow-up boundary

#### Scenario: Migration debt is reduced
- **WHEN** a focused refactor slice resolves a tracked warning
- **THEN** the architecture debt record and validation expectations are updated
  in the same PR so the warning cannot silently reappear

### Requirement: Architecture validation expectations track reduced debt
The architecture-maintainability gate SHALL keep current warning expectations
aligned with the tracked debt ledger. A PR that resolves a warning MUST update
the report expectations and documentation in the same change so the warning
cannot silently reappear.

#### Scenario: Warning count decreases
- **WHEN** `check-architecture-maintainability.sh` reports fewer warnings than
  the documented debt ledger
- **THEN** the implementation updates the ledger and any script expectations
  before the PR is considered complete

#### Scenario: Warning count does not decrease
- **WHEN** a refactor slice moves architecture code but does not reduce the
  warning count
- **THEN** the PR explains why the moved boundary is an intermediate step and
  leaves the debt ledger accurate

#### Scenario: New or broadened warning appears
- **WHEN** a change introduces a new architecture warning or broadens an
  existing warning while reducing another one
- **THEN** the change either resolves the new warning before merge or records a
  concrete follow-up boundary in the debt ledger

### Requirement: Apple client engineering identity is alan-macos
The Apple client SHALL use `alan-macos` as the active engineering identity for
the macOS app project, scheme, target-facing developer commands, source root,
architecture checks, and script path references.

#### Scenario: Developer builds the macOS app
- **WHEN** a developer reads or runs the documented macOS app build command
- **THEN** the command references `clients/apple/alan-macos.xcodeproj`
- **AND** the selected scheme is `alan-macos`
- **AND** the generated app product is `Alan.app`

#### Scenario: Swift app entry is inspected
- **WHEN** a developer inspects the Swift app entry point
- **THEN** the type and file names do not contain `AlanNative`
- **AND** any Swift identifiers that include `Alan` use Swift naming
  conventions rather than command-facing lowercase casing

#### Scenario: Architecture validation runs
- **WHEN** Apple-client architecture validation scans source layout,
  README/build commands, scripts, project metadata, and active OpenSpec work
- **THEN** it recognizes `alan-macos` as the engineering identity and `Alan` as
  the user-visible product brand
- **AND** it rejects reintroduced `AlanNative` project, path, scheme, or target
  identity unless the occurrence is an explicit compatibility or migration
  fixture

### Requirement: Reusable shell domain logic migrates to Rust shell core
The Apple client architecture SHALL treat reusable shell workspace domain logic
as Rust shell core ownership once the corresponding shell core module, Rust
contract tests, and adapter tests exist.

Swift files in the Apple client SHALL remain platform adapters, presentation
layers, terminal runtime hosts, and compatibility wrappers rather than the
stable home for reusable workspace reducer, manifest, action, control-command,
or Terminal Profile domain semantics.

#### Scenario: New reusable shell behavior is added
- **WHEN** a developer adds behavior that changes platform-neutral shell
  workspace semantics after the shell core module for that domain exists
- **THEN** the behavior is implemented in the Rust shell core
- **AND** the Apple client consumes it through a platform adapter rather than
  adding a separate Swift implementation

#### Scenario: Swift logic is replaced
- **WHEN** a Swift shell domain module is replaced by Rust shell core behavior
- **THEN** the replaced Swift implementation is removed or narrowed to adapter
  code
- **AND** `clients/apple/ARCHITECTURE.md` and architecture validation
  expectations are updated when warning debt decreases

### Requirement: Architecture debt burn-down follows shell core adoption
Apple client architecture warning debt SHALL decrease as Swift shell domain
logic is replaced by Rust shell core modules.

#### Scenario: Rust-backed module lands
- **WHEN** a Rust-backed shell core module replaces a Swift reducer, manifest,
  action, control, profile, or settings domain implementation
- **THEN** the implementation slice records which architecture warning class was
  reduced or explains why the replacement is an intermediate adapter-only step
- **AND** new pure domain logic is not added to the large Swift files that the
  slice is meant to retire

### Requirement: Replaced shell-domain Swift logic is deleted or adapter-only
After a shell-domain area is replaced by Rust shell core, the Apple client SHALL
remove the corresponding reusable Swift domain implementation or narrow it to
adapter-only projection code.

#### Scenario: Large Swift shell file is edited after replacement
- **WHEN** a developer edits a large Swift shell model, controller, or service
  file in a replaced core-owned area
- **THEN** the edit does not add a new reusable shell-domain algorithm
- **AND** any remaining Swift code is documented or structured as adapter
  projection, platform effect execution, or platform recovery

#### Scenario: Architecture checks run
- **WHEN** architecture maintainability checks inspect shell-core replaced
  areas
- **THEN** they flag new Swift implementations of core-owned manifest,
  reducer, action, control-command, profile, or settings-domain behavior as
  architecture debt or failures according to the active gate mode

### Requirement: Shell-core authority reduces architecture debt
Each implementation slice MUST remove or narrow the replaced Swift
implementation enough for the architecture debt record to shrink or become more
precise when it makes a shell-domain area core-authoritative.

#### Scenario: Manifest authority slice lands
- **WHEN** the manifest authority slice is completed
- **THEN** Swift no longer contains a runtime manifest default, prune, or
  materialize implementation for the same portable behavior
- **AND** `clients/apple/ARCHITECTURE.md` records the resulting ownership state

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

#### Scenario: Action registry metadata FFI owners are validated
- **WHEN** action registry descriptors, availability, shortcuts, or keyboard
  lookup are routed through shell core
- **THEN** architecture validation allows high-level Swift calls only in
  `ShellActionCoordinator`
- **AND** raw action metadata operation names remain confined to the shell-core
  action adapter owner

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

### Requirement: Active Apple source layout mirrors macOS shell ownership

The native Apple client SHALL organize the active macOS product by app startup, shell views, terminal hosts, models, controllers, services, adapters, and support code under `clients/apple/alan-macos`.

#### Scenario: Source tree is inspected

- **WHEN** a developer inspects the active Apple source root and Xcode target membership
- **THEN** every product source belongs to an active macOS owner
- **AND** no source group exists without an active product owner and focused validation boundary

#### Scenario: Architecture docs describe the tree

- **WHEN** Apple architecture documentation lists source owners
- **THEN** the list matches the active Xcode project and filesystem layout
- **AND** it does not preserve a future attachment owner that has not been designed

### Requirement: Deleted Apple compatibility consumers have no replacement stub

The active macOS target SHALL NOT contain an unavailable placeholder, mock service, disabled control, or alternate data source for a deleted compatibility consumer.

#### Scenario: Obsolete consumer removal is reviewed

- **WHEN** the cleanup removes a source group with no active macOS product owner
- **THEN** their source files and project references are deleted
- **AND** unrelated terminal, workspace, update, helper, and shell-control features continue through their existing owners

### Requirement: Architecture warning debt is reduced through active-owner slices

The Apple client SHALL reduce maintainability warnings through focused, behavior-preserving slices that name an active owner and its verification commands.

#### Scenario: Focused slice resolves a warning

- **WHEN** a refactor removes warnings from `check-architecture-maintainability.sh`
- **THEN** the architecture debt ledger and expected warning count are updated in the same change
- **AND** focused checks protect any active terminal, shell-controller, service, or adapter behavior moved by the slice

### Requirement: Repository quality gate ratchets Apple architecture debt
The repository quality gate SHALL run Apple architecture-maintainability
validation for every commit and pull request. The 15 currently recorded
large-file and bridge-seam warnings SHALL have stable entries in a structured
ledger. The live report MUST exactly match that ledger, and the ledger MUST be
compared with the pre-change Git reference so it cannot be raised in the same
change as the source. A warning reduction MUST tighten the ledger and recorded
count in the same change.

#### Scenario: Apple architecture warning grows
- **WHEN** a change increases the current Apple architecture warning count or
  broadens a recorded warning class
- **THEN** the repository quality gate fails

#### Scenario: Apple source and warning ledger grow together
- **WHEN** an existing large Swift file and its recorded line ceiling are both
  increased in one change
- **THEN** comparison with the pre-change ledger reports the debt growth
- **AND** the repository quality gate fails

#### Scenario: Apple architecture warning is removed
- **WHEN** a focused refactor removes or narrows a recorded warning
- **THEN** the Apple architecture ledger and executable ceiling are tightened in
  the same change
- **AND** later reintroduction fails

#### Scenario: Non-Apple CI runner validates ownership
- **WHEN** the repository quality gate runs on a non-macOS CI runner
- **THEN** source-layout, ownership, project-membership, and warning-ledger
  checks still run without requiring an Apple app launch
