## ADDED Requirements

### Requirement: macOS Matter controller spike is platform-scoped
Alan for macOS SHALL isolate Apple `Matter.framework` integration inside the
Apple client/service layer and MUST NOT require `alan-runtime` to link against
or model Apple Matter types.

#### Scenario: Runtime invokes future home capability
- **WHEN** Matter controller behavior is exposed beyond the Apple service layer
- **THEN** the boundary is expressed as typed local service/RPC behavior suitable
  for future `home.*` tools
- **AND** `alan-runtime` remains platform-agnostic

#### Scenario: Linux or non-Apple runtime builds are evaluated
- **WHEN** non-macOS runtime crates are built or tested
- **THEN** they do not require Apple `Matter.framework`, `MTRDeviceController`,
  or Apple Matter storage types

### Requirement: Spike commissions one direct Matter light
The macOS Matter controller spike SHALL support commissioning one directly
Matter-capable light into Alan's own Matter fabric from a setup payload.

#### Scenario: Setup payload is provided
- **WHEN** the operator provides a valid setup payload for a direct Matter light
  in pairing or multi-admin pairing mode
- **THEN** Alan creates or loads a local Matter controller
- **AND** Alan attempts to commission the light into Alan's own fabric
- **AND** the result records success or the concrete commissioning failure

#### Scenario: Payload targets unsupported scope
- **WHEN** the setup payload is for a HomeKit-only device, bridge-only topology,
  bridged endpoint, or unsupported high-risk device type
- **THEN** the spike does not claim support for that target
- **AND** the result reports an unsupported target or verification blocker

### Requirement: Controller state survives app restart
The macOS Matter controller spike SHALL persist the controller state needed to
reuse a commissioned direct Matter light after Alan for macOS restarts.

#### Scenario: App restarts after commissioning
- **WHEN** the light was successfully commissioned and Alan for macOS restarts
- **THEN** Alan reloads the local Matter controller state
- **AND** the commissioned light remains listable without repeating
  commissioning

#### Scenario: Persisted state is unavailable
- **WHEN** controller or fabric state cannot be loaded
- **THEN** Alan reports the controller as unavailable or uninitialized
- **AND** Alan does not pretend that previously commissioned devices are ready

### Requirement: Spike lists and reads the commissioned light
The macOS Matter controller spike SHALL list the commissioned direct Matter light
and read its On/Off state through the macOS Matter controller service.

#### Scenario: Commissioned light is reachable
- **WHEN** the commissioned light is online and reachable on the local Matter
  fabric
- **THEN** Alan lists the light as a commissioned node
- **AND** Alan can read its On/Off state with a structured result

#### Scenario: Commissioned light is unreachable
- **WHEN** the commissioned light is offline or unreachable
- **THEN** Alan returns a structured unavailable result
- **AND** Alan does not report stale state as confirmed current state

### Requirement: Spike performs low-risk light OnOff writes
The macOS Matter controller spike SHALL support setting the commissioned direct
Matter light On or Off and SHALL record a structured physical-action result.

#### Scenario: Turn light on
- **WHEN** the operator requests turning the commissioned light on through the
  spike path
- **THEN** Alan sends the On command through the macOS Matter controller service
- **AND** Alan records target, requested action, status, timestamp, and any error
  detail in the action result

#### Scenario: Turn light off
- **WHEN** the operator requests turning the commissioned light off through the
  spike path
- **THEN** Alan sends the Off command through the macOS Matter controller service
- **AND** Alan records target, requested action, status, timestamp, and any error
  detail in the action result

### Requirement: Spike excludes product home-control tooling
The macOS Matter controller spike SHALL NOT define final `home.*` tool schemas,
general physical-device governance policy, or LLM-visible raw Matter
endpoint/cluster command access.

#### Scenario: Agent-facing control is requested during the spike
- **WHEN** a design or implementation path would expose raw Matter cluster
  commands or broad physical-device writes to the LLM
- **THEN** that behavior is deferred to the later home-control product change
- **AND** the spike remains limited to service feasibility and low-risk light
  verification

#### Scenario: Product scope is evaluated after spike success
- **WHEN** the direct Matter light spike passes commissioning, persistence,
  list, read, and OnOff write verification
- **THEN** productized `home.*` tools, device registry naming, governance risk
  levels, and skill instructions are handled by a separate OpenSpec change
