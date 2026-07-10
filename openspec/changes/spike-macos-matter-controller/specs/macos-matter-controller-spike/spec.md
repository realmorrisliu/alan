## ADDED Requirements

### Requirement: Matter integration is a host-backed file server
Alan for macOS SHALL isolate `Matter.framework` behind an aP Matter Service that
posts `/srv/matter` and serves `/mnt/matter`. Alan Kernel, Agent Execution Engine,
portable domain crates, and non-Apple builds SHALL NOT import or model Apple
Matter types.

#### Scenario: Matter Service starts on macOS
- **WHEN** the host adapter becomes ready
- **THEN** controller, commissioning, device, action, status, and events files
  are available through the authorized mount
- **AND** clients do not need a typed RPC or direct framework call

#### Scenario: Non-Apple tests run
- **WHEN** Linux or another non-Apple target builds/tests the file contract
- **THEN** it uses a fake backend without linking `Matter.framework`

### Requirement: Commissioning is an inspectable file lifecycle
The Matter Service SHALL allocate a commissioning directory from ordinary aP
operations and SHALL expose whole request, status, result, events, and adjacent
`ctl` files. A setup payload SHALL commit only on clunk and partial payloads SHALL
NOT start commissioning.

#### Scenario: Valid direct-light payload is committed
- **WHEN** an authorized operator commits a setup payload and starts the attempt
- **THEN** the service creates or loads Alan's controller and attempts to
  commission the light into Alan's fabric
- **AND** progress and concrete success/failure remain inspectable

#### Scenario: Unsupported payload is committed
- **WHEN** the payload targets an excluded bridge, endpoint, or high-risk device
- **THEN** the attempt is rejected with an unsupported result
- **AND** no unsupported physical command is executed

### Requirement: Controller state survives host restart
The Matter Service SHALL persist the protected controller/fabric state required
to reopen commissioned devices after Alan for macOS restarts. It SHALL expose
safe readiness/status files without exposing operational secrets.

#### Scenario: Host restarts after commissioning
- **WHEN** the service restarts with valid stored controller state
- **THEN** it reposts `/srv/matter`, remounts the tree, and lists the light
  without recommissioning

#### Scenario: Stored state is unavailable
- **WHEN** protected state cannot be opened
- **THEN** controller status reports unavailable or uninitialized
- **AND** devices are not reported as confirmed ready

### Requirement: The spike lists and reads one direct light
The Matter Service SHALL list the commissioned direct light and expose current
On/Off state through its device directory.

#### Scenario: Light is reachable
- **WHEN** a client reads the device's status and `onoff` file
- **THEN** the service returns current reachable state and an observed On/Off
  value

#### Scenario: Light is unreachable
- **WHEN** current state cannot be read
- **THEN** status reports unavailable and does not present stale state as current

### Requirement: OnOff writes are whole document mutations
The spike SHALL accept only bounded On/Off writes for the direct light through a
whole `onoff` document committed on clunk. It SHALL serialize the physical write
and create an action result plus event.

#### Scenario: Operator turns the light on
- **WHEN** an authorized client commits `on` to the light's `onoff` document
- **THEN** the service sends the Matter On command and records requested state,
  status, timestamp, observed read-back when available, and errors

#### Scenario: A partial or invalid state is written
- **WHEN** the document is incomplete or not `on`/`off`
- **THEN** commit fails and no physical command is sent

### Requirement: Spike excludes product home-control tooling
The Matter spike SHALL NOT define final `home.*` Tool schemas, a global device
registry, raw cluster/endpoint execution, broad physical-device governance, or
LLM-visible direct Matter commands.

#### Scenario: Agent-facing product control is requested
- **WHEN** a design proposes broad Tools or additional device categories during
  the spike
- **THEN** that work is deferred to a separate OpenSpec change after physical
  verification

### Requirement: Debug clients use canonical file operations
Any spike-only CLI or developer UI SHALL allocate/read/write the Matter Service
files and SHALL NOT call `Matter.framework` or an RPC controller directly. The
debug surface SHALL remain explicitly non-product.

#### Scenario: Setup payload is entered in developer UI
- **WHEN** the operator submits the payload
- **THEN** the UI commits the commissioning request file and watches its events
- **AND** the same attempt is visible to any other authorized file client

### Requirement: Physical verification evidence is retained
The spike SHALL retain bounded commissioning, restart, list, read, On, and Off
result files plus manual environment notes sufficient to distinguish a platform
failure, network/device blocker, unsupported scope, and successful operation.

#### Scenario: Spike readiness is evaluated
- **WHEN** maintainers decide whether to productize Matter control
- **THEN** each required operation has inspectable result files and manual fixture
  notes
- **AND** logs alone are not treated as sufficient proof
