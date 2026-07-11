## REMOVED Requirements

### Requirement: Provider and connection vocabulary is stable
**Reason**: The current vocabulary defines effective profiles and immutable bindings around Session creation.
**Migration**: Bind a resolved connection to an Agent Process and its Agent Machine generation path.

### Requirement: Provider descriptors define provider setup capabilities
**Reason**: Descriptor discovery names daemon consumers.
**Migration**: Expose descriptors through direct CLI and the owning provider/connection file surfaces when implemented.

### Requirement: Connection profiles are the operator-facing provider object
**Reason**: Profile behavior is defined partly through Session binding and daemon onboarding.
**Migration**: Keep profiles operator-facing and bind them at Agent Process creation.

### Requirement: Profile resolution and session binding are deterministic
**Reason**: Session creation and `session_id` are the identity and freezing boundary.
**Migration**: Resolve one concrete connection for each Agent Process and preserve it for that Process lifetime.

### Requirement: Agent config pins profiles without carrying credentials
**Reason**: Pin precedence is defined against future Session creation.
**Migration**: Apply pins during Agent Process definition and launch resolution.

### Requirement: Connection management has a stable CLI and daemon surface
**Reason**: Daemon connection routes are removed without replacement.
**Migration**: Retain the direct `alan connection` CLI; future file-server management requires a separate contract.

### Requirement: Provider and auth layers remain explicitly separated
**Reason**: Provider resolution scenarios are expressed through Sessions.
**Migration**: Resolve provider and credential inputs for Agent Process generation while keeping secret ownership host-local.

### Requirement: ChatGPT managed auth has first-class boundaries
**Reason**: Browser completion and observation are coupled to daemon callbacks and Session scopes.
**Migration**: Host auth owns managed login and MAY use a bounded ephemeral browser callback without exposing a daemon API.

### Requirement: Connection and credential stores are channel-scoped
**Reason**: Channel failure is expressed as Session startup.
**Migration**: Enforce channel isolation when resolving and spawning an Agent Process.

## ADDED Requirements

### Requirement: Provider and connection vocabulary is Process-shaped
Alan SHALL distinguish provider family, provider descriptor, credential reference, connection
profile, default profile, pin, resolved connection, and Process connection binding. A Process
connection binding SHALL associate one Agent Process with one resolved provider/model/credential
reference for its lifetime and SHALL contain no secret material.

#### Scenario: An Agent Process resolves a connection
- **WHEN** Alan spawns an Agent Process whose definition, workspace, or operator defaults select a
  connection profile
- **THEN** it resolves one concrete provider/model/credential reference for that Process
- **AND** later default changes do not mutate the running Process binding

### Requirement: Connection management is direct and owner-scoped
Alan SHALL retain direct `alan connection` commands for descriptor discovery, profile mutation,
default and pin management, secret entry, login, and connection testing. The commands SHALL operate
through the owning connection, credential, auth, AgentRoot, and provider components. Any future
file-server management surface requires its own accepted contract.

#### Scenario: Operator lists connection profiles
- **WHEN** an operator runs `alan connection list`
- **THEN** the CLI reads the active channel's connection and credential metadata owners directly
- **AND** the read-only command does not launch or mutate an Agent Process

### Requirement: Host auth remains separate from provider execution
Host auth SHALL own secret storage and managed login state; provider adapters SHALL receive only
resolved credential material needed for a generation. Browser login MAY use a bounded ephemeral
callback owned by the initiating host operation but SHALL NOT require a persistent product API.

#### Scenario: Browser login completes
- **WHEN** the operator initiates managed browser login
- **THEN** the initiating host auth operation receives and validates the callback
- **AND** only the host auth owner may grant or persist credential authority

### Requirement: Connection state remains channel-scoped
Stable and dev channels SHALL keep distinct connection metadata, credential references, and managed
auth roots. Agent Process creation SHALL fail truthfully when its active channel cannot resolve a
profile rather than borrowing another channel's state.

#### Scenario: Dev Process has no dev connection
- **WHEN** a dev-channel Agent Process is spawned without a resolvable dev connection profile
- **THEN** creation reports the missing connection
- **AND** stable connection or credential state is not consumed implicitly
