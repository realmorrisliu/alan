## MODIFIED Requirements

### Requirement: Shell host controller narrows to orchestration
`ShellHostController` SHALL remain the observable renderer-facing composition
owner and SHALL stop owning shell-core-backed domain routing, terminal runtime
lifecycle, persistence scheduling, and large platform workflow implementations
directly. It MAY publish projections and own transient view state, diagnostics,
notifications, and explicit platform effects, but it MUST delegate durable
state and workflows to their accepted concrete owners.

#### Scenario: Manifest startup or persistence changes
- **WHEN** workspace manifest startup or persistence behavior changes
- **THEN** the controller delegates loading, shell-core materialization,
  pruning, failure diagnostics, and write scheduling to
  `ShellWorkspacePersistenceCoordinator`
- **AND** the controller does not regain portable manifest behavior or a second
  persistence scheduler

#### Scenario: Shell action or reducer command routing changes
- **WHEN** shell action dispatch, reducer-backed commands, or control response
  adoption changes
- **THEN** portable execution flows through the existing shell-core coordinator
  or local command executor rather than an unrelated controller switch
- **AND** Swift platform effects remain explicit and separate from portable
  shell-domain validation

#### Scenario: Terminal lifecycle changes
- **WHEN** terminal runtime creation, release, finalization, active-task state,
  or publication behavior changes
- **THEN** the stateful behavior lives in `TerminalRuntimeRegistry` or its
  existing stateful publication owner
- **AND** the controller only publishes the resulting renderer projection

#### Scenario: Controller split is validated
- **WHEN** a controller-owned shell-core, terminal, close, or persistence path
  is moved
- **THEN** the affected focused shell script runs with shell contract and
  architecture-maintainability validation
- **AND** a touched rendered surface is verified in a freshly built and
  relaunched Alan Dev app

## ADDED Requirements

### Requirement: Shell host state has one mutable authority
Portable workspace, selection, and mutation truth SHALL come from
`ShellStateSnapshot`; terminal runtime, active-task, render, and lifecycle truth
SHALL come from `TerminalRuntimeRegistry`; and persistence scheduling SHALL come
from `ShellWorkspacePersistenceCoordinator`. Controller-published values MUST be
derived projections and MUST NOT require synchronization between duplicate
mutable truths.

#### Scenario: Workspace selection changes
- **WHEN** shell core selects a Space, Tab, or pane
- **THEN** the controller publishes selection derived from the adopted
  `ShellStateSnapshot`
- **AND** no independently mutable selected-Space or selected-Tab field is
  reconciled with it

#### Scenario: Terminal activity changes
- **WHEN** a terminal runtime starts, stops, or changes active-task state
- **THEN** `TerminalRuntimeRegistry` records the lifecycle truth
- **AND** any controller publication is a projection of that registry state

#### Scenario: Duplicate state returns
- **WHEN** architecture validation finds controller-owned selection, terminal
  runtime, active-task, projection-queue, or persistence-scheduling state that
  duplicates an accepted owner
- **THEN** validation fails rather than adding a synchronization helper

#### Scenario: Replacement store uses an escaped Swift identifier
- **WHEN** architecture validation scans an executable declaration such as a
  backticked `ShellStore`
- **THEN** it normalizes every Swift source before matching ownership symbols
- **AND** it rejects the replacement owner while continuing to ignore comments
  and string literals

### Requirement: Local control commands have one execution path
Portable and state-mutating shell control commands SHALL execute through
`AlanShellLocalCommandExecutor` for both socket and in-process callers. The
Apple host MAY handle terminal delivery, diagnostics, render metrics, and
explicit platform effects locally, but MUST NOT duplicate the portable command
switch or shell-state mutation behavior.

#### Scenario: In-process control mutates shell state
- **WHEN** `ShellHostController` receives a portable or state-mutating control
  command
- **THEN** it invokes `AlanShellLocalCommandExecutor`, adopts the returned state
  and response, and applies only explicit platform effects

#### Scenario: Socket control mutates shell state
- **WHEN** the socket server receives the same command
- **THEN** it uses the same local command executor semantics
- **AND** socket transport remains separate from command execution

#### Scenario: Duplicate controller command switch is inspected
- **WHEN** architecture validation scans active Apple sources
- **THEN** `ShellHostControlCommandHandling.swift` or an equivalent second
  portable command implementation is absent

### Requirement: Stateful platform workflows have concrete owners
Close confirmation and graceful shutdown SHALL be one concrete close workflow;
terminal release, finalization, buffering, active-task state, and publication
deduplication SHALL remain with terminal runtime owners; and manifest write
scheduling SHALL remain with the persistence coordinator. These collaborators
MUST own the state they coordinate and MUST NOT be pass-through services.

#### Scenario: A window or pane closes
- **WHEN** close confirmation, terminal shutdown, state mutation, and persistence
  must be coordinated
- **THEN** one close workflow orders those steps and reports its result to the
  controller
- **AND** pane mutation still executes through shell-core authority

#### Scenario: Terminal content disappears
- **WHEN** shell state no longer projects an active terminal content mount
- **THEN** `TerminalRuntimeRegistry` releases or finalizes the corresponding
  runtime directly
- **AND** no lifecycle-only forwarding adapter owns the operation

### Requirement: Adapters must hide real boundary complexity
An Apple adapter SHALL remain only when it owns FFI or platform isolation,
mapping or normalization policy, stateful reporting or deduplication, failure
translation, or a substantial projection algorithm. A stateless pass-through
that merely renames an existing owner operation SHALL be deleted or folded into
that owner.

#### Scenario: Terminal adapters are audited
- **WHEN** the terminal adapter set is reviewed
- **THEN** projection, publication-policy, metadata, activity-normalization,
  stateful reporter, and C ABI boundaries remain in focused owners
- **AND** `TerminalContentLifecycleAdapter` is removed after its projection and
  lifecycle calls move to shell state and `TerminalRuntimeRegistry`

#### Scenario: A new adapter is proposed
- **WHEN** a proposed adapter would have one implementation and only forward its
  inputs to an existing concrete owner
- **THEN** the adapter is not introduced
- **AND** the caller uses the existing owner directly
