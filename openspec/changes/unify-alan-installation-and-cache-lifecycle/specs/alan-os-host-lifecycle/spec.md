## MODIFIED Requirements

### Requirement: Each foreground invocation owns its alan9 instance
Each bare or redirected Agent-execution invocation of `alan` SHALL own one
foreground alan9 instance, including its Kernel, Process table, services, and
instance-local Root Agent Process. When `ALAN_INSTANCE_RUNTIME_DIR` is unset,
the CLI SHALL allocate a unique temporary runtime directory. When it is set,
that exact directory SHALL select the instance endpoint, and only one invocation
may own it. Host composition SHALL supply explicit product store bindings without selecting an installation channel or singleton live runtime. Service Manager SHALL retain ownership
of services and the Root Agent Process within the invocation. Bare `alan` MUST
NOT launch or attach to a separate background Host. Herdr SHALL NOT be required
for ordinary terminal operation.

#### Scenario: Two terminal sessions start Alan
- **WHEN** two terminal sessions start `alan` for the same user without `ALAN_INSTANCE_RUNTIME_DIR`, or with distinct runtime
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

### Requirement: Product composition preserves production adapters
Foreground product composition SHALL use the existing production providers,
product stores, and governance. Mock providers and ephemeral test stores SHALL
require explicit isolated test bindings; foreground composition MUST NOT
itself select a test Host or weaken authorization.

#### Scenario: Product composition fails
- **WHEN** product composition cannot boot its foreground instance
- **THEN** startup fails with a diagnostic
- **AND** it does not substitute mock providers, test-only authority, or an
  ambient background Host
