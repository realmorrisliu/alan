# remote-access-service Specification

## Purpose
Defines the file-native Alan OS boundary for authenticated cross-host entry:
the Remote Bootstrap Tree, one-shot handoff, Remote Entry Processes, attachment
leases, lineage revocation, remote context, mounted trees, and the single
semantic boundary that every byte transport must preserve.
## Requirements
### Requirement: Remote Access Service is a Service-Manager-started file server
Alan OS SHALL own cross-host entry through a `Remote Access Service`: a file server started by
Service Manager that terminates remote transports, authenticates remote principals, manages Remote
Attachment Leases, and creates or reattaches Remote Entry Processes. Its handle SHALL be posted at
`/srv/remote-access`; a local inspection mount MAY appear at `/mnt/remote-access`. After handoff,
reads, writes, watches, and spawns SHALL reach the attached process tree rather than pass through
the entry service as a steady-state proxy.

#### Scenario: Entry service is discovered and bounded
- **WHEN** a host boots with remote access enabled
- **THEN** Service Manager starts the Remote Access Service and posts its handle at
  `/srv/remote-access`
- **AND** after handoff client operations reach the attached process tree directly

### Requirement: Remote bootstrap is an aP file surface with distinct entry views
The Remote Access Service SHALL export a minimal aP Remote Bootstrap Tree as its only entry surface.
Fresh entry SHALL allocate a bootstrap instance through clone-via-open at `new/clone`, exposing
`request`, `status`, `handoff`, and `ctl`. Reattachment SHALL be discovered by walking `leases/`,
whose entries expose only neutral lease metadata and no project or app-continuation view.

#### Scenario: Fresh entry is allocated by clone-via-open
- **WHEN** a remote client opens `new/clone`
- **THEN** a concrete bootstrap directory is allocated with `request`, `status`, `handoff`, and
  `ctl`
- **AND** entry creation uses only aP file operations

#### Scenario: Reattachment is discovered by walking leases
- **WHEN** a remote client lists `leases/`
- **THEN** it sees reattachable leases with minimal neutral metadata
- **AND** reattachment remains distinct from fresh entry

### Requirement: Handoff is a blocking one-shot capability delivery
The bootstrap `handoff` file SHALL block until a Remote Entry Process root handle is ready and then
deliver that namespace root itself. Successful handoff SHALL consume the bootstrap instance;
`status` SHALL expose progress separately from delivery. Failure or cancellation before delivery
SHALL clean up partial state, while post-delivery lifecycle belongs to the process tree and lease.

#### Scenario: Handoff delivers once
- **WHEN** a bootstrap flow completes handoff
- **THEN** the client holds the entry process namespace root
- **AND** the consumed bootstrap instance cannot deliver the same root again

#### Scenario: Pre-handoff failure leaves no orphans
- **WHEN** a bootstrap flow is cancelled or fails before handoff
- **THEN** it removes partial state including any partially created Remote Entry Process

### Requirement: Remote Entry Process is a neutral shell process cloning the login template
A Remote Entry Process SHALL be a real Process on the destination host, created under the target
user's normal Credential with remote device identity retained separately for provenance and lease
control. It SHALL begin as a general shell entry with a namespace cloned from the Login Namespace
Template. Fresh attach SHALL create a new entry process; lineage reuse SHALL require explicit lease
reattachment.

#### Scenario: Fresh remote entry lands neutrally
- **WHEN** a fresh remote attach completes
- **THEN** the client enters a new general shell Process with the user's standard login namespace
- **AND** no app, project, or prior task surface is implicitly restored
- **AND** Agent Processes run only when explicitly spawned beneath the entry lineage

### Requirement: Leases bound continuity and recovery never re-drive execution
A Remote Attachment Lease SHALL keep the entry process alive across transport loss for a bounded
window and SHALL become active atomically when handoff becomes ready. Recovery SHALL reattach the
lease, resume Streams from saved offsets, and reread current files; it SHALL NOT re-drive execution
or rely on a separately synthesized recovery snapshot as runtime truth.

#### Scenario: Transport drops between readiness and receipt
- **WHEN** transport fails after handoff readiness but before root receipt
- **THEN** the active lease preserves the entry process
- **AND** the client recovers through explicit lease reattachment

#### Scenario: Remote client reconnects
- **WHEN** a client reconnects within the lease window
- **THEN** it reattaches the lease, resumes Streams from saved offsets, and rereads current files
- **AND** no execution is re-driven

### Requirement: Revocation terminates the remote lineage
Alan OS SHALL terminate the remote-attached process lineage when the remote device authorization is
revoked or the lease expires. Any future exception SHALL require a separate accepted design.

#### Scenario: Device is revoked during an attachment
- **WHEN** the owning user revokes a remote device with an active attachment
- **THEN** the Remote Entry Process and descendants terminate
- **AND** no survivor retains remote-originated authority

### Requirement: Remote context is exposed as lineage-local inherited files
Alan OS SHALL expose remote-only attachment facts as a Remote Context Tree mounted at `/mnt/remote`
inside the attached lineage and inherited through normal namespace mechanics unless policy removes
it. Service discovery at `/srv/remote-access` and lineage provenance at `/mnt/remote` SHALL remain
distinct.

#### Scenario: A tool inspects remote provenance
- **WHEN** a process beneath a remote entry lineage reads `/mnt/remote`
- **THEN** it observes the lineage's attachment facts through ordinary file reads
- **AND** unrelated local processes receive no view of that lineage context

### Requirement: Imported remote trees are ordinary mounts with remote-side effects
A remote tree imported over aP SHALL be an ordinary file server mounted by default under
`/mnt/peer/<remote-id>`. Mutations and executable effects on the imported tree SHALL execute on the
remote host under the exporting lineage. Importing a tree SHALL NOT expose the local namespace back
to that lineage.

#### Scenario: A local process uses a peer tool tree
- **WHEN** a local process spawns an executable from `/mnt/peer/<remote-id>`
- **THEN** the effect executes on the remote host under the exporting lineage
- **AND** the remote lineage gains no implicit local namespace view

### Requirement: Attachment scope defaults to the user namespace with an explicit threat model
In the single-user phase, the default attachment scope SHALL be the signed-in user's full namespace.
Product and operator surfaces SHALL state that authority plainly and identify short-lived tickets,
bounded leases, and lineage revocation as containment. Narrower process-lineage, mounted-domain,
or app projection SHALL require a separate accepted capability and SHALL NOT be implied.

#### Scenario: Attachment scope is documented
- **WHEN** a current surface describes remote attachment authority
- **THEN** it states the full-namespace default and containment levers
- **AND** it does not imply narrower scope exists

### Requirement: Remote access has one file-native semantic boundary
Alan OS remote entry SHALL occur only through aP operations on the Remote Bootstrap Tree and the
returned namespace. Authenticated byte transports MAY vary byte delivery, reachability,
encryption, latency, and ticketing, but SHALL NOT introduce a second semantic entry, control,
recovery, or application API.

#### Scenario: A transport proposes a second semantic surface
- **WHEN** remote transport work proposes functionality unavailable through the Remote Access
  Service and returned namespace
- **THEN** the change is rejected or reshaped as file operations under the owning service tree
