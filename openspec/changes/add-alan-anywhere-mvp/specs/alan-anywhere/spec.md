## ADDED Requirements

### Requirement: Account-bound device enrollment
alan SHALL bind each remote-capable Mac and iPhone app installation to an Alan
account and a stable device identity before allowing Alan Anywhere access.

#### Scenario: Mac enrolls after account login
- **WHEN** a user signs in to Alan Desktop on macOS
- **THEN** the Mac is registered as a device owned by that account
- **AND** the Mac receives device-bound credentials suitable for remote
  availability
- **AND** those credentials are stored in the platform secure store rather than
  in workspace files

#### Scenario: iPhone signs in to the same account
- **WHEN** a user signs in to Alan on iPhone with the same account as the Mac
- **THEN** the iPhone is registered as a device owned by that account
- **AND** the iPhone can request access only to devices associated with that
  account

### Requirement: Automatic Mac remote availability
Alan Desktop SHALL automatically keep the signed-in Mac remotely connectable
while the app is running and the user has not disabled Alan Anywhere.

#### Scenario: Desktop starts while signed in
- **WHEN** Alan Desktop starts with a valid signed-in account and device binding
- **THEN** it establishes an outbound encrypted connection to the Alan remote
  service without requiring inbound network configuration
- **AND** the user is not asked for public IP, router, VPN, tunnel, SSH, or port
  forwarding settings

#### Scenario: Desktop loses remote connectivity
- **WHEN** the Mac loses network connectivity or its outbound remote connection
  drops
- **THEN** Alan Desktop retries connection in the background
- **AND** Alan Cloud marks the device stale or offline without changing local
  runtime state

### Requirement: User-owned device discovery
alan SHALL let a signed-in iPhone discover the user's own online Alan Desktop
devices without exposing relay or tunnel implementation details.

#### Scenario: iPhone lists available Macs
- **WHEN** the iPhone app requests Alan Anywhere devices for the signed-in
  account
- **THEN** the response includes only devices owned by that account
- **AND** each device includes product-facing status such as online/offline,
  last seen, and connectability
- **AND** the response does not require the iPhone user to provide a daemon URL,
  public IP, tunnel URL, or relay node token

#### Scenario: Device is offline
- **WHEN** a previously enrolled Mac is not connected to Alan Cloud
- **THEN** the iPhone may show the Mac as offline or unavailable
- **AND** the iPhone MUST NOT offer remote entry into that Mac
  until it reconnects

### Requirement: Device availability discovery
alan SHALL expose enough device availability status for iPhone users to choose
which owned Alan device to enter remotely. Authoritative workspace, session,
task, and app-continuation discovery happens after attachment through the
returned remote namespace, not through the pre-attachment control plane.

#### Scenario: Mac publishes connectable device status
- **WHEN** Alan Desktop is online
- **THEN** it publishes connectable device status for the signed-in user
- **AND** status includes product-facing availability such as online/offline,
  last seen, and whether remote entry is currently accepted
- **AND** the Mac remains the authority for local namespace, process, app, and
  work state

#### Scenario: iPhone chooses which device to enter
- **WHEN** the iPhone user selects an online Mac
- **THEN** the iPhone can request remote entry into that Mac's Alan OS
- **AND** work discovery proceeds through the returned `Remote Entry Process`
  namespace
- **AND** the UI presents the action as entering another Alan device rather than
  connecting to infrastructure

### Requirement: Remote namespace interaction
alan SHALL allow iPhone to interact with the selected Mac by entering a
`Remote Entry Process` namespace and then using ordinary Alan OS file
operations.

#### Scenario: iPhone sends a message
- **WHEN** the iPhone user sends a message to an agent process through Alan
  Anywhere
- **THEN** the message is written through the attached remote namespace, such as
  an agent `io/` surface
- **AND** the Mac validates authority through the descriptor and process model

#### Scenario: iPhone interrupts an active run
- **WHEN** the iPhone user interrupts a running remote process
- **THEN** the interrupt is written through the relevant remote process control
  surface, such as `/proc/<pid>/ctl`
- **AND** the Mac remains responsible for applying or rejecting the interrupt

#### Scenario: iPhone resumes a pending yield
- **WHEN** a remote agent process is waiting for confirmation or structured input
- **THEN** the iPhone can write the response through the remote request surface
- **AND** the Mac validates the pending request before advancing the process

### Requirement: Realtime remote stream flow
alan SHALL support realtime remote delivery of stream file records from the Mac
to the iPhone through the attached remote namespace.

#### Scenario: Process streams output remotely
- **WHEN** a Mac-authored remote process appends output, events, warnings,
  errors, or request state to stream files
- **THEN** the iPhone receives those records in near real time by reading the
  remote streams
- **AND** stream identity, offsets, and record order remain authored by the Mac

#### Scenario: Relay transports stream bytes
- **WHEN** realtime stream records are delivered through Alan Cloud relay
- **THEN** the relay forwards transport bytes without becoming the authority for
  stream offsets, record order, process state, or runtime state
- **AND** the iPhone can recover missed observations by reattaching and reading
  from saved stream offsets

### Requirement: Reconnect and gap recovery
alan SHALL recover remote iPhone attachments after app backgrounding, network
changes, and relay reconnects without duplicating execution.

#### Scenario: iPhone reconnects with a valid cursor
- **WHEN** the iPhone reattaches to a live lease with saved stream offsets
- **THEN** it resumes reading stream records from those offsets
- **AND** execution is not restarted or re-driven by reconnect

#### Scenario: iPhone cursor has a gap
- **WHEN** the iPhone reattaches after a saved stream offset is no longer
  available
- **THEN** the stream read reports the gap through file/stream semantics
- **AND** the iPhone rebuilds actionable state by rereading current files from
  the returned namespace before continuing stream consumption

### Requirement: Host-authoritative execution boundary
alan SHALL keep Alan Anywhere execution, tool access, governance, namespace
reads, and process state authoritative on the user's destination Alan OS host.

#### Scenario: Cloud brokers remote entry
- **WHEN** Alan Cloud brokers an Alan Anywhere entry attempt
- **THEN** it issues or validates only the product-layer entry ticket and
  transport route
- **AND** Alan Cloud MUST NOT execute tools, read namespace files, decide policy
  outcomes, spawn processes, or mutate runtime state on behalf of the Mac

#### Scenario: Mac rejects unauthorized entry
- **WHEN** a remote entry attempt lacks a valid account, device, target, entry
  intent, expiry, or revocation state
- **THEN** the Mac or product control plane rejects the attempt with a
  machine-readable authorization error
- **AND** no remote entry process is created or reattached

### Requirement: Remote access security and revocation
alan SHALL protect Alan Anywhere with encrypted transport, device binding,
short-lived remote entry tickets, and revocation.

#### Scenario: Remote connection is established
- **WHEN** iPhone connects to a Mac through Alan Anywhere
- **THEN** the connection uses encrypted transport
- **AND** the remote entry ticket is scoped to the signed-in account, client
  device, target Mac device, entry intent, expiry, and revocation state
- **AND** the current single-user default does not require workspace-, session-,
  or operation-scoped tokens before entering the user's remote namespace

#### Scenario: Device is revoked
- **WHEN** a user revokes a Mac or iPhone device
- **THEN** alan invalidates future remote access for that device
- **AND** active remote connections using that device credential are closed or
  rejected before additional state-changing operations are accepted

### Requirement: Zero-configuration product language
alan SHALL present Alan Anywhere as device-to-device Alan continuation, not as
remote desktop or user-managed networking.

#### Scenario: User opens Alan Anywhere on iPhone
- **WHEN** the iPhone user opens the Alan Anywhere surface
- **THEN** the primary UI language describes online Alan devices and entry into
  the selected device
- **AND** it does not require or foreground VPN, tunnel, Cloudflare, SSH, port
  mapping, router configuration, public IP, or daemon URL concepts

#### Scenario: Debug details are needed
- **WHEN** a developer opens an explicit debug or diagnostics surface
- **THEN** alan may expose relay, node, routing, and connection diagnostics
- **AND** those diagnostics remain outside the default user workflow
