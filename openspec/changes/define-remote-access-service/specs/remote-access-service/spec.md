## ADDED Requirements

### Requirement: Remote Access Service is a Service-Manager-started file server
Alan OS SHALL own cross-host entry through a `Remote Access Service`: a file
server started by Service Manager that terminates remote transports,
authenticates remote principals, manages remote attachment leases, and creates
or reattaches `Remote Entry Process`es. Its handle SHALL be posted at
`/srv/remote-access`; a local inspection mount MAY appear at
`/mnt/remote-access`. The service SHALL NOT own runtime truth: after handoff,
reads, writes, watches, and spawns go to the attached process tree, never
through the entry service as a steady-state proxy.

#### Scenario: Entry service is discovered and bounded
- **WHEN** a host boots with remote access enabled
- **THEN** Service Manager starts the Remote Access Service and its handle is
  posted at `/srv/remote-access`
- **AND** after a completed handoff, client operations reach the attached
  process tree without passing through the entry service

### Requirement: Remote bootstrap is an aP file surface with distinct entry views
The Remote Access Service SHALL export a minimal aP `Remote Bootstrap Tree` as
the only remote entry surface — no separate login or attach RPC. Fresh entry
and lease reattachment SHALL be distinct file-surface views: fresh entry
allocates a bootstrap instance via clone-via-open (`new/clone`) exposing a
standard minimal file set (`request`, `status`, `handoff`, `ctl`); lease
reattachment is discovered by walking a browsable `leases/` directory whose
entries expose only minimal neutral metadata (lease identity, timestamps,
entry kind, transport mode, reattachability) — no workspace, session, or
app-continuation views.

#### Scenario: Fresh entry is allocated by clone-via-open
- **WHEN** a remote client opens `new/clone` under the bootstrap tree
- **THEN** a concrete bootstrap directory is allocated with `request`,
  `status`, `handoff`, and `ctl` files
- **AND** entry creation does not require any RPC outside file operations

#### Scenario: Reattachment is discovered by walking leases
- **WHEN** a remote client lists the `leases/` view
- **THEN** it sees reattachable leases with minimal neutral metadata only
- **AND** resuming a lease is an explicit act distinct from fresh entry

### Requirement: Handoff is a blocking one-shot capability delivery
The bootstrap `handoff` file SHALL block until the `Remote Entry Process` root
handle is ready and then deliver that namespace root itself — not an endpoint
bundle or session object. A successful handoff SHALL consume the bootstrap
instance: it becomes terminal and cannot deliver the same root handle again.
`status` SHALL expose progress and readiness separately from delivery. If
authentication, transport setup, request validation, or `ctl` cancellation
terminates the flow before delivery, the bootstrap instance SHALL clean up any
partial state, including a partially created entry process; after delivery,
lifecycle ownership belongs to the process tree and lease.

#### Scenario: Handoff delivers once
- **WHEN** a bootstrap flow completes handoff
- **THEN** the client holds the entry process's namespace root
- **AND** re-reading the consumed bootstrap instance cannot yield the root
  handle again; later attachment uses a new clone or an explicit lease reattach

#### Scenario: Pre-handoff failure leaves no orphans
- **WHEN** a bootstrap flow is cancelled or fails before handoff
- **THEN** the bootstrap instance cleans up partial state, including any
  partially created Remote Entry Process

### Requirement: Remote Entry Process is a neutral shell process cloning the login template
A `Remote Entry Process` SHALL be a real `Process` on the destination host,
created before any client interaction, running under the target user's normal
`Credential` (the remote device tracked separately as `Remote Device Identity`
for provenance and lease control). It SHALL start as a general shell entry —
not an Agent Process — landing in a neutral state with the user's standard
login namespace, cloned from the `Login Namespace Template` rather than shared
with a live process. A fresh attach SHALL create a new entry process by
default; resuming an existing lineage SHALL require explicit reattach intent.

#### Scenario: Fresh remote entry lands neutrally
- **WHEN** a fresh remote attach completes
- **THEN** the client is in a new general shell entry process with the user's
  standard login namespace
- **AND** no app, workspace, or prior task surface is implicitly restored
- **AND** agents run only as processes explicitly spawned beneath the entry
  lineage

### Requirement: Leases bound continuity and recovery never re-drives execution
A `Remote Attachment Lease` SHALL keep the entry process alive across
transport loss for a bounded window, reattachable by the same remote client
identity. The lease SHALL become active atomically when the root handle
becomes handoff-ready, so a transport failure between readiness and client
receipt is recovered by explicit reattachment rather than by discarding a
valid entry process. Reconnect recovery SHALL use lease reattachment, saved
stream offsets, and ordinary file reads; reconnection SHALL NOT re-drive
execution, and no daemon-style reconnect snapshot is a source of truth.

#### Scenario: Transport drops between readiness and receipt
- **WHEN** the transport fails after handoff readiness but before the client
  receives the root handle
- **THEN** the lease is already active and the entry process survives
- **AND** the client recovers through explicit lease reattachment

#### Scenario: Mobile client reconnects
- **WHEN** a remote client reconnects after transport loss within the lease
  window
- **THEN** it reattaches the lease, resumes streams from saved offsets, and
  rereads current files
- **AND** no execution is re-driven by the reconnect

### Requirement: Revocation terminates the remote lineage
Alan OS SHALL terminate the whole remote-attached process lineage by default
when the remote device's authorization is revoked or its lease expires. This
phase defines no local takeover operation; any future exception requires an
explicit separate design.

#### Scenario: Device is revoked mid-session
- **WHEN** the owning user revokes a remote device with an active attachment
- **THEN** the remote entry process and its descendants terminate
- **AND** no silent local survivor retains remote-originated authority

### Requirement: Remote context is exposed as lineage-local inherited files
Alan OS SHALL expose remote-only attachment facts (device identity, transport
mode, lease state, reattachment history) as a `Remote Context Tree` mounted at
`/mnt/remote` inside the attached lineage only — never as a host-global tree —
inherited by descendant processes through normal namespace mechanics unless
explicit policy strips it. Service discovery (`/srv/remote-access`) and
lineage provenance (`/mnt/remote`) SHALL remain distinct paths.

#### Scenario: A tool inspects its remote provenance
- **WHEN** a process spawned beneath a remote entry lineage reads
  `/mnt/remote`
- **THEN** it observes the lineage's attachment facts through ordinary file
  reads
- **AND** an unrelated local process has no `/mnt/remote` view of that lineage

### Requirement: Imported remote trees are ordinary mounts with remote-side effects
A remote tree imported over aP SHALL be an ordinary mountable file server,
defaulting under `/mnt/peer/<remote-id>` where `<remote-id>` names the
exported entry tree (not the device — one device may host several lineages).
Mutating file operations and executable effects on an imported tree SHALL
execute on the remote host, attributed to the exporting lineage. Cross-host
cooperation SHALL compose through files and processes — no dedicated
agent-to-agent RPC protocol. Visibility SHALL be directional: importing a
remote tree does not implicitly expose the local namespace back to the remote
lineage.

#### Scenario: A local agent uses a peer's tool tree
- **WHEN** a local process walks `/mnt/peer/<remote-id>` and spawns an
  executable from it
- **THEN** the effect executes on the remote host under the exporting lineage
- **AND** the remote lineage gains no implicit view of the local namespace

### Requirement: Attachment scope defaults to the user namespace with an explicit threat model
In the current single-user phase, the default attachment scope SHALL be the
signed-in user's full namespace (`User Namespace Attachment`). Documentation
and product surfaces SHALL state the resulting threat model plainly: a remote
client holds the user's whole world; short-lived entry tickets, bounded
leases, and lineage revocation are the containment levers. Scoped
(session-, workspace-, or app-projected) attachment is named follow-up work
and SHALL NOT be silently assumed present.

#### Scenario: Attachment scope is documented
- **WHEN** a spec or user-facing surface describes remote attachment authority
- **THEN** it states the full-namespace default and its containment levers
- **AND** it does not imply per-workspace or per-session scoping exists today

### Requirement: Remote access has no compatibility gateway
Alan OS remote access SHALL NOT keep or introduce an HTTP, WebSocket,
daemon-session, or translation gateway for remote clients. Transports (direct,
relay, LAN, future brokers) vary only in byte delivery — reachability,
encryption, latency, ticketing, reconnect — never in attach semantics.
Daemon-era remote APIs are deletion targets, not migration surfaces; new
remote work enters only through aP, the Remote Access Service, and returned
namespaces.

#### Scenario: A remote feature proposes an HTTP surface
- **WHEN** a change proposes remote client functionality over an HTTP/WS
  endpoint or a daemon-session translation layer
- **THEN** the change is rejected or re-shaped to enter through the Remote
  Access Service and file operations on a returned namespace
