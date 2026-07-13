# local-alan-os-attachment Specification

## Purpose
Defines channel-scoped same-user local Alan OS attachment over native aP Unix
sockets, including peer authorization, fid ownership, disconnect, and
reattachment semantics.

## Requirements

### Requirement: Local attachment uses native aP wire
The Alan OS Host SHALL export its ready namespace through the existing aP wire
protocol on a channel-specific Unix domain socket in a platform runtime
directory. It MUST NOT expose HTTP, WebSocket, Session, or relay protocols.

#### Scenario: Authorized local client attaches
- **WHEN** a same-user client connects to the matching channel endpoint
- **THEN** it imports the exported tree as an ordinary aP FileServer

### Requirement: Host OS peer identity ends at authorization
The Host SHALL restrict the endpoint to the current Host OS user and validate
peer identity. It MUST NOT project Host UID, home, cwd, or login identity into
Alan OS credentials or namespace.

#### Scenario: Different Host OS user connects
- **WHEN** a peer with a different UID connects
- **THEN** the Host rejects it before namespace access

### Requirement: Connections own fids, not execution identity
Each attachment connection SHALL own independent fid lifecycle. Disconnect
SHALL clunk those fids and MUST NOT terminate Alan OS Processes; reconnect SHALL
walk stable paths and resume streams from caller-held offsets.

#### Scenario: Renderer disconnects during Agent work
- **WHEN** its Unix socket closes
- **THEN** the Agent Process continues according to `/proc`
- **AND** a later client can attach without recreating execution
