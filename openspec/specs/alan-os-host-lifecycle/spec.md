# alan-os-host-lifecycle Specification

## Purpose
Defines foreground invocation ownership of an alan9 instance, file-proven
readiness, per-invocation boot identity, and production composition.

## Requirements

### Requirement: Each foreground invocation owns its alan9 instance
Each bare or redirected Agent-execution invocation of `alan` SHALL own one
foreground alan9 instance, including its Kernel, Process table, services, and
instance-local Root Agent Process. When `ALAN_INSTANCE_RUNTIME_DIR` is unset,
the CLI SHALL allocate a unique temporary runtime directory. When it is set,
that exact directory SHALL select the instance endpoint, and only one invocation
may own it. The install channel SHALL select configuration and persistent store
boundaries, not a singleton live runtime. Service Manager SHALL retain ownership
of services and the Root Agent Process within the invocation. Bare `alan` MUST
NOT launch or attach to a separate background Host. Herdr SHALL NOT be required
for ordinary terminal operation.

#### Scenario: Two terminal sessions start Alan
- **WHEN** two terminal sessions start `alan` for the same user and install
  channel without `ALAN_INSTANCE_RUNTIME_DIR`, or with distinct runtime
  directories
- **THEN** each owns an independent Root Agent, Process table, input queue, cwd,
  and runtime endpoint
- **AND** exiting one invocation does not stop or submit work to the other
- **AND** concurrent use of package, connection, and credential stores preserves
  their existing commit and authorization contracts

#### Scenario: Two invocations select the same runtime directory
- **WHEN** two terminal sessions select the same `ALAN_INSTANCE_RUNTIME_DIR`
- **THEN** only one invocation owns that endpoint
- **AND** the other reports that it cannot acquire the instance instead of
  attaching to or borrowing its Root Agent

#### Scenario: Terminal host retains a process
- **WHEN** Herdr or another terminal host detaches a view while retaining Alan
- **THEN** that instance may keep executing while its native process is alive
- **AND** Alan does not spawn a background replacement to provide that lifetime
- **AND** Herdr identifiers grant neither Alan identity nor authority

### Requirement: Host readiness is file-proven
An invocation SHALL expose its renderer or an explicitly addressed local
attachment only after the Standard Namespace, required services, and its
`/agent/root` are readable. Required failure SHALL fail startup, clean up the
failed instance, and report an error rather than connect to another invocation.

#### Scenario: Root Agent fails to start
- **WHEN** `/agent/root` cannot be read during boot
- **THEN** the invocation reports startup failure
- **AND** it does not present a partial system or borrow another terminal's Root
  Agent

### Requirement: Host restart creates a new boot identity
Each boot SHALL create a fresh boot identity, Process table and Root Agent.
Recovery SHALL require explicit durable selection, never live Process state or
another invocation's namespace. Invalid selection SHALL fail visibly. Recorded
paths or grants MUST NOT restore live authority. Reliable pending work SHALL stay
paused until explicit continuation under current authority; unknown effects MUST
NOT replay automatically.

#### Scenario: A Process reference belongs to another boot
- **WHEN** a client presents a reference with a different instance boot identity
- **THEN** Alan rejects it even if the PID has been reused

#### Scenario: A new invocation starts
- **WHEN** a user starts another bare `alan` invocation without selecting recovery
- **THEN** it creates fresh execution in its own instance
- **AND** it does not implicitly attach to or resume another invocation's Root

#### Scenario: User selects durable recovery
- **WHEN** the user explicitly selects valid durable execution evidence
- **THEN** existing recovery owners interpret it in a fresh instance
- **AND** reliable pending work is exposed paused with current access revalidated
- **AND** restoring directory authority alone does not continue the queue

#### Scenario: User selects missing or invalid recovery evidence
- **WHEN** the selected durable record is missing or invalid
- **THEN** startup fails with a diagnostic rather than silently starting fresh

### Requirement: Product composition preserves production adapters
Foreground product composition SHALL use the existing production providers,
channel stores, and governance. Mock providers and ephemeral test stores SHALL
require explicit development or test selection; foreground composition MUST NOT
itself select a test Host or weaken authorization.

#### Scenario: Product composition fails
- **WHEN** product composition cannot boot its foreground instance
- **THEN** startup fails with a diagnostic
- **AND** it does not substitute mock providers, test-only authority, or an
  ambient background Host

### Requirement: Foreground application exit ends its owned runtime
Actual Alan process exit SHALL shut down its owned services and active execution
through existing lifecycle and native descendant-cancellation boundaries.
Completed effects SHALL remain completed. Terminal-host view detach SHALL be
distinguished from actual process exit. Forced termination or incomplete
durable evidence SHALL NOT be reported as successful cancellation or work.

#### Scenario: User exits Alan during work
- **WHEN** the foreground application exits
- **THEN** it stops admitting work and shuts down its owned execution
- **AND** it does not keep a detached Host running or roll back completed effects

#### Scenario: Native process is forcibly terminated
- **WHEN** Alan cannot complete orderly shutdown or persist a terminal outcome
- **THEN** the result remains incomplete or unknown from available evidence
- **AND** absence of completion does not authorize automatic replay
