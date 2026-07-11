## REMOVED Requirements

### Requirement: Remote control contracts live in OpenSpec
**Reason**: The daemon-era remote-control capability is removed rather than maintained as a current contract owner.
**Migration**: All future remote-entry requirements belong to `remote-access-service`.

### Requirement: Remote governance cannot bypass local policy
**Reason**: This invariant no longer belongs to a remote Session control surface.
**Migration**: Remote Entry Processes remain governed by normal namespace, credential, policy, and Process rules under `remote-access-service`.

### Requirement: Remote control topology preserves node authority
**Reason**: Agent-node daemon topology and Session routing are removed.
**Migration**: Use host-local Process authority and Remote Access Service handoff.

### Requirement: Direct and relay transports expose explicit MVP surfaces
**Reason**: HTTP routes, relay tunnels, and transport-specific product surfaces are removed.
**Migration**: Transports carry aP bytes for `remote-access-service` without owning entry semantics.

### Requirement: Relay node discovery and sticky binding are deterministic
**Reason**: Relay node and Session binding state are removed.
**Migration**: Remote attachment continuity uses Remote Attachment Leases.

### Requirement: Reconnect snapshots preserve remote continuity without re-execution
**Reason**: Reconnect snapshots are removed as a source of remote truth.
**Migration**: Reattach a lease and resume file streams from offsets.

### Requirement: Remote notification signals are informational
**Reason**: Daemon remote notifications are removed with the remote-control surface.
**Migration**: Observe authoritative files and streams in the attached namespace.

### Requirement: Remote reconnect and multi-client consistency use node-authored cursors
**Reason**: Node-authored Session cursors are removed.
**Migration**: Use file stream offsets and service-owned consistency semantics.

### Requirement: Remote metadata extends protocol without changing runtime semantics
**Reason**: Daemon protocol metadata is no longer a remote extension point.
**Migration**: Remote provenance is exposed through the lineage-local Remote Context Tree.

### Requirement: Remote auth scopes and daemon configuration are explicit
**Reason**: Daemon endpoints, route scopes, and bind configuration are removed.
**Migration**: Remote authentication and lease authority belong to `remote-access-service`.

### Requirement: Relay credentials and runtime configuration are scoped and revocable
**Reason**: Relay credentials and node runtime configuration are removed.
**Migration**: Use remote principal authentication, entry tickets, leases, and lineage revocation.

### Requirement: Remote security preserves replay integrity and audit trails
**Reason**: The requirement is coupled to daemon relay/session replay.
**Migration**: Security and audit requirements attach to Remote Entry Processes, namespaces, file operations, and service-owned leases.

### Requirement: Local daemon defaults are channel-scoped
**Reason**: Stable/dev daemon endpoints and port defaults are removed.
**Migration**: None; future host attachment requires a separate design.
