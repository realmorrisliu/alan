## MODIFIED Requirements

### Requirement: Local attachment uses native aP wire
An alan9 instance SHALL export its ready namespace through the existing aP wire
protocol on the endpoint under its runtime directory. A client SHALL select the
instance explicitly, using `ALAN_INSTANCE_RUNTIME_DIR` where the CLI exposes
local instance management. Alan MUST NOT use an executable name or historical install channel to select another
terminal's live instance or introduce a separate management transport beside aP.

#### Scenario: Authorized local client attaches
- **WHEN** a same-user client selects a live instance endpoint
- **THEN** it imports that instance's exported tree as an ordinary aP FileServer
- **AND** the returned boot identity binds subsequent Process references to
  that instance

### Requirement: Local attachment upgrades fail closed
Adding a local attachment operation MUST preserve the existing `attach`
operation's Shell Process semantics for older clients. A processless client MUST
use its distinct advertised operation and MUST NOT fall back to the legacy
operation when the selected ready instance does not support it.

#### Scenario: A ready instance lacks processless attachment support
- **WHEN** a processless client connects to an explicitly selected older endpoint
- **THEN** it reports the protocol incompatibility
- **AND** it neither creates a Shell Process through fallback nor starts a
  replacement instance

#### Scenario: A stale instance endpoint is selected
- **WHEN** the selected instance has exited or no longer accepts its attachment
  socket
- **THEN** the client reports that endpoint as unavailable
- **AND** it does not start or select another instance or historical channel endpoint

#### Scenario: An older instance is still starting
- **WHEN** an explicitly selected instance endpoint is not yet ready
- **THEN** attachment may wait within its existing bounded startup window
- **AND** it reports incompatibility if that instance lacks the required
  protocol
- **AND** it does not start another instance as a fallback
#### Scenario: Endpoint metadata uses a retired channel layout
- **WHEN** a current client encounters incompatible channel-bearing endpoint metadata
- **THEN** it reports incompatibility at the explicitly selected endpoint
- **AND** it does not retry another channel path or borrow a different Root
