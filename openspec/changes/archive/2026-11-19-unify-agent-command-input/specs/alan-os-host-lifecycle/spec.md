## MODIFIED Requirements

### Requirement: Each foreground invocation owns its alan9 instance
Each bare or redirected Agent-execution invocation of `alan` SHALL own an
independent alan9 instance when `ALAN_INSTANCE_RUNTIME_DIR` is unset or unique
to that invocation. It also owns its Kernel and native application lifetime.
An explicit shared runtime directory selects one exact endpoint and permits
only one owner.
In this contract, an invocation means that execution path; metadata,
configuration and explicitly targeted management subcommands SHALL NOT boot an
unrelated Root Agent.
Explicit Host bindings SHALL select the product or isolated test store pair,
not a singleton live runtime. Service Manager
SHALL retain ownership of services and the instance-local Root Agent Process.
The renderer SHALL remain a file client of that instance. Bare `alan` MUST NOT
launch or attach to a separate background Host, including through launchd or
systemd. Herdr SHALL NOT be required for ordinary terminal operation.

#### Scenario: Two terminal sessions start Alan
- **WHEN** two terminal sessions start `alan` for the same user without `ALAN_INSTANCE_RUNTIME_DIR`, or with distinct runtime
  directories
- **THEN** each owns an independent Root Agent, Process table, input queue, cwd,
  and runtime endpoint
- **AND** exiting one invocation does not stop or submit work to the other
- **AND** concurrent use of package, connection, and credential stores preserves
  their existing commit and authorization contracts

#### Scenario: Two invocations select the same runtime directory
- **WHEN** two terminal sessions select the same explicit
  `ALAN_INSTANCE_RUNTIME_DIR`
- **THEN** only one invocation owns that runtime directory
- **AND** the other fails to acquire it instead of attaching to or borrowing its Root Agent

#### Scenario: Terminal host retains a process
- **WHEN** Herdr or another terminal host detaches a view while retaining the Alan process
- **THEN** that instance may keep executing while its process is alive
- **AND** Alan does not spawn a background replacement to provide that lifetime
- **AND** Herdr identifiers grant neither Alan identity nor authority

### Requirement: Host readiness is file-proven
An invocation SHALL expose its renderer or explicitly addressed local attachments
only after the Standard Namespace, required services and `/agent/root` are
readable. Required failure SHALL fail startup, clean up the failed instance and
report an error rather than connect to an unrelated running instance.

#### Scenario: Root Agent fails to start
- **WHEN** `/agent/root` cannot be read during boot
- **THEN** the invocation reports startup failure
- **AND** it does not present a partial system or borrow another terminal's Root Agent

### Requirement: Host restart creates a new boot identity
Every invocation SHALL create a fresh boot identity, Process table and Root Agent
Process. It MUST NOT deserialize live Process state or implicitly select a
previous product-wide rollout. Recovery SHALL require an explicitly selected
durable record. Recovery failure SHALL be reported without silently starting a
fresh task. Current authority SHALL be revalidated; recorded paths or grants are
not live capabilities. Reliable pending work SHALL remain paused, and unknown
effects MUST NOT be replayed automatically.

#### Scenario: A Process reference belongs to another boot
- **WHEN** a client presents a reference with a different instance boot identity
- **THEN** Alan rejects it even if the PID has been reused

#### Scenario: A new invocation starts
- **WHEN** the user starts a new invocation without explicitly selecting recovery
- **THEN** it creates fresh execution in its own instance
- **AND** it does not implicitly attach to or resume another invocation's Root

#### Scenario: User selects durable recovery
- **WHEN** the user explicitly selects valid durable execution evidence
- **THEN** it is interpreted by the existing recovery owners in a fresh instance
- **AND** reliable pending work is exposed paused with current access revalidated

#### Scenario: User selects missing or invalid recovery evidence
- **WHEN** the user explicitly selects a missing or invalid durable recovery record
- **THEN** recovery fails with a diagnostic rather than starting fresh execution

### Requirement: Product composition preserves production adapters
Mock providers and ephemeral test stores SHALL require explicit development/test
selection. Foreground product composition SHALL use the existing product adapters,
product stores and governance; running in-process SHALL NOT itself select a test
Host or weaken authorization.

#### Scenario: Product composition fails
- **WHEN** product composition fails to boot its independent foreground instance
- **THEN** startup fails with a diagnostic
- **AND** it does not substitute mock providers, test-only authority, or an
  ambient background Host

### Requirement: Foreground application exit ends its owned runtime
Actual Alan exit SHALL shut down its owned services and active execution through
the existing lifecycle and native descendant cancellation boundaries. Completed
effects SHALL remain completed. Forced termination or incomplete durable evidence
SHALL NOT be reported as successful cancellation or successful work. Terminal-host
view detach SHALL be distinguished from actual process exit.

#### Scenario: User exits Alan during work
- **WHEN** the user explicitly exits the foreground application
- **THEN** the instance stops admitting work and shuts down its owned execution
- **AND** it does not keep a detached Host running or roll back completed effects

#### Scenario: Native process is forcibly terminated
- **WHEN** Alan cannot complete orderly shutdown or persist a terminal outcome
- **THEN** later explicit recovery reports incomplete or unknown work from available evidence
- **AND** absence of completion does not authorize automatic replay
