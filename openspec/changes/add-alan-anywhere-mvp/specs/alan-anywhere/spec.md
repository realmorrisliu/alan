## ADDED Requirements

### Requirement: Alan Anywhere uses explicit owned-device enrollment
Alan Anywhere SHALL require account sign-in, device-key establishment, a
user-facing device name, and explicit confirmation before a Mac becomes an
owned remote-entry destination.

#### Scenario: User enrolls a Mac
- **WHEN** the signed-in user confirms enrollment on the Mac
- **THEN** Alan records the owned device and its public identity material
- **AND** remote entry remains disabled until enrollment completes

### Requirement: Device availability is product-facing and bounded
Alan Anywhere SHALL show enrolled-device availability using human-readable
device identity and expiry-bounded presence. It SHALL NOT publish destination
workspace, app, or Process catalogs before entry.

#### Scenario: iPhone lists devices
- **WHEN** the user opens Alan Anywhere on iPhone
- **THEN** it shows owned devices with honest availability freshness
- **AND** infrastructure identifiers remain behind explicit diagnostics

### Requirement: Remote Entry Tickets are short-lived and intent-bound
Alan Anywhere SHALL authorize entry with a single-use or replay-protected ticket
bound to account, source device, destination device, entry intent, nonce, and
expiry. The destination host SHALL validate the ticket and local policy.

#### Scenario: Ticket is replayed
- **WHEN** a used or expired ticket is presented again
- **THEN** the destination refuses entry
- **AND** bounded audit evidence records the refusal

### Requirement: Entry hands off a general Remote Entry Process
Successful product entry SHALL consume `remote-access-service` handoff and give
the client the granted Remote Entry Process namespace. Product discovery SHALL
continue through ordinary file operations after handoff.

#### Scenario: Entry succeeds
- **WHEN** the destination validates the ticket and creates the entry Process
- **THEN** the client receives the granted namespace root
- **AND** files, Processes, Agent Processes, and apps are discovered from that
  namespace

### Requirement: Network continuity uses leases and stream offsets
Alan Anywhere SHALL preserve active entry across temporary byte-delivery loss by
reattaching the current lease, rereading files, and resuming streams from
caller-held offsets. It SHALL NOT recreate already-running execution.

#### Scenario: Mobile connectivity changes
- **WHEN** byte delivery is interrupted while the lease remains valid
- **THEN** the client reattaches and resumes observable streams from saved
  offsets
- **AND** destination Process identity remains unchanged

### Requirement: Revocation terminates remote lineage
Device, account, lease, and explicit operator revocation SHALL prevent new entry
and terminate the affected active remote lineage according to
`remote-access-service`.

#### Scenario: User revokes a lost iPhone
- **WHEN** the user revokes the source device from another authorized surface
- **THEN** active entry lineage for that device terminates
- **AND** future tickets for the device are rejected

### Requirement: Alan Cloud has bounded coordination authority
Alan Cloud SHALL limit its coordination authority to accounts, enrolled-device
metadata, coarse presence, ticket issuance, and byte-delivery coordination. It
SHALL NOT author destination files, Process state, Agent Machine state, policy
decisions, or Tool results.

#### Scenario: Cloud state disagrees with destination state
- **WHEN** cloud metadata claims a destination action completed but destination
  files do not
- **THEN** clients treat destination files and Process state as authoritative
- **AND** the cloud claim cannot commit or fabricate the action

### Requirement: The product contract is transport-neutral
Alan Anywhere SHALL require authenticated encrypted byte delivery with measured
interactive quality, while leaving the concrete transport to a separate
accepted implementation change.

#### Scenario: Transport implementation changes
- **WHEN** Alan adopts a different byte-delivery mechanism
- **THEN** device, ticket, handoff, Process, lease, and revocation semantics stay
  unchanged
