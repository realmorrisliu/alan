## MODIFIED Requirements

### Requirement: Local attachment uses native aP wire
The foreground renderer SHALL use its own instance's mounted namespace. If local
cross-process attachment is exposed, it SHALL use the existing native aP wire
protocol on an explicitly addressed instance endpoint in a private platform
runtime directory. Alan MUST NOT introduce a competing management transport or
silently resolve a channel name to another terminal's live Root Agent.

#### Scenario: Authorized local client attaches
- **WHEN** a same-user client explicitly selects a live instance endpoint
- **THEN** it imports that instance's exported tree as an ordinary aP FileServer
- **AND** the returned boot identity binds subsequent Process references to that instance

### Requirement: Connections own fids, not execution identity
Each attachment connection SHALL own independent fid lifecycle. Disconnect SHALL
clunk those fids and MUST NOT itself terminate alan9 Processes. Reconnection to a
still-live instance SHALL walk paths and resume streams from caller-held offsets.
This connection rule SHALL NOT extend lifetime beyond the owning foreground
application; actual application exit shuts down that instance.

#### Scenario: Renderer disconnects during Agent work
- **WHEN** its Unix socket closes while the owning Alan application remains alive
- **THEN** Agent execution continues according to `/proc`
- **AND** a later explicitly targeted attachment can reconnect to that instance

#### Scenario: The owning application exits
- **WHEN** the foreground Alan invocation ends
- **THEN** its endpoint becomes unavailable and attachments report disconnection
- **AND** they do not launch a replacement or attach to a different instance

### Requirement: Local attachment upgrades fail closed
Adding a local attachment operation MUST preserve the existing `attach`
operation's Shell Process semantics for older clients. A processless client MUST
use its distinct advertised operation and MUST NOT fall back to the legacy
operation when the selected ready instance does not support it.

#### Scenario: A ready instance lacks processless attachment support
- **WHEN** a processless client connects to an explicitly selected older endpoint
- **THEN** it reports the protocol incompatibility
- **AND** it neither creates a Shell Process through fallback nor starts a replacement instance

#### Scenario: A stale instance endpoint is selected
- **WHEN** the selected instance has exited or no longer accepts its attachment
  socket
- **THEN** the client reports that target as unavailable
- **AND** it does not start or select another instance for the same channel

#### Scenario: An older instance is still starting
- **WHEN** an explicitly selected instance endpoint is not yet ready
- **THEN** attachment may wait within its existing bounded startup window
- **AND** it reports incompatibility if that instance lacks the required protocol
- **AND** it does not start another instance as a fallback
