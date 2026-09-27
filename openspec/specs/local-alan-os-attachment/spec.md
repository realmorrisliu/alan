# local-alan-os-attachment Specification

## Purpose
Defines same-user attachment to an explicitly selected foreground alan9
instance over native aP Unix sockets, including peer authorization, fid
ownership, disconnect, and reattachment semantics.

## Requirements

### Requirement: Local attachment uses native aP wire
An alan9 instance SHALL export its ready namespace through the existing aP wire
protocol on the endpoint under its runtime directory. A client SHALL select the
instance explicitly, using `ALAN_INSTANCE_RUNTIME_DIR` where the CLI exposes
local instance management. Alan MUST NOT resolve an install channel to another
terminal's live instance or introduce a separate management transport beside aP.

#### Scenario: Authorized local client attaches
- **WHEN** a same-user client selects a live instance endpoint
- **THEN** it imports that instance's exported tree as an ordinary aP FileServer
- **AND** the returned boot identity binds subsequent Process references to
  that instance

### Requirement: Host OS peer identity ends at authorization
The instance SHALL restrict its endpoint to the current Host OS user and
validate peer identity. It MUST NOT project Host UID, home, cwd, or login
identity into alan9 credentials or namespace.

#### Scenario: Different Host OS user connects
- **WHEN** a peer with a different UID connects
- **THEN** the instance rejects it before namespace access

### Requirement: Connections own fids, not execution identity
Each attachment connection SHALL own independent fid lifecycle. Disconnect
SHALL clunk those fids without terminating Processes while the owning Alan
application remains alive. Reconnection to that same, explicitly selected live
instance SHALL walk stable paths and resume streams from caller-held offsets.
Actual Alan process exit ends the instance and its Processes.

#### Scenario: Renderer disconnects during Agent work
- **WHEN** its Unix socket closes while the owning Alan process remains alive
- **THEN** the Agent Process continues according to `/proc`
- **AND** a later explicitly targeted client can attach to that instance

#### Scenario: The owning application exits
- **WHEN** the foreground Alan invocation ends
- **THEN** its endpoint becomes unavailable and attachments report disconnection
- **AND** they do not launch a replacement or attach to another instance

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
- **AND** it does not start or select another instance for the same channel

#### Scenario: An older instance is still starting
- **WHEN** an explicitly selected instance endpoint is not yet ready
- **THEN** attachment may wait within its existing bounded startup window
- **AND** it reports incompatibility if that instance lacks the required
  protocol
- **AND** it does not start another instance as a fallback
