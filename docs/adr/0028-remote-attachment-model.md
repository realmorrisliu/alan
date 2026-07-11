# Remote Attachment Model

Status: Accepted. Extends ADR-0024 and ADR-0026. Normative scenarios live in
the OpenSpec `remote-access-service` capability.

## Context

Cross-host access should import a real destination Process namespace through aP
rather than invent a separate remote object model.

## Decisions

### D1. Remote entry targets a namespace

A remote principal enters through mountable file trees and executable surfaces.
The initial single-user phase grants the signed-in user's full namespace.
Narrower projections require a separate accepted capability.

### D2. Entry creates a real Process

Fresh entry creates a destination-host Remote Entry Process with ordinary
credentials, descriptors, cwd, namespace, lifecycle, and audit. It begins as a
general shell Process. Agent Processes are spawned beneath it explicitly.

### D3. Fresh entry clones the Login Namespace Template

The new Process receives the user's Login Namespace Template rather than
sharing another live Process namespace.

### D4. Remote Access Service owns bootstrap and leases

A Service-Manager-started File-Server Service posts
`/srv/remote-access`. It authenticates remote principals, creates entry
Processes, and manages attachment leases. It does not own steady-state Process
or Agent Machine truth.

### D5. Bootstrap hands off a namespace root

Fresh entry allocates a bootstrap instance through `new/clone`, then uses
`request`, `status`, `handoff`, and `ctl`. `handoff` is a blocking one-shot
delivery of the entry Process namespace root. Successful handoff consumes the
bootstrap instance.

### D6. Leases bound continuity

The entry Process may survive temporary byte-delivery loss within a bounded
lease. A client reattaches the lease, rereads files, and resumes streams from
caller-held offsets. Existing execution is never recreated by attachment
recovery.

### D7. Revocation terminates the lineage

Device revocation or lease expiry terminates the affected remote Process
lineage by default.

### D8. Remote context is lineage-local files

Remote device identity, transport facts, lease state, and attachment history
are exposed under `/mnt/remote` inside the remote lineage and inherited through
normal namespace rules.

### D9. User credential and device provenance are separate

The entry Process runs under the target user's Credential. Remote device
identity is retained separately for provenance, audit, and lease control.

### D10. Product coordination stays above Alan OS

Account login, device directory, presence, and Remote Entry Ticket issuance
belong to the product plane. Product discovery stops at device availability;
work discovery begins after handoff by reading the namespace.

### D11. Byte delivery is replaceable

Reachability, encryption, latency, energy, and operational cost vary by
transport implementation. Entry Process, namespace, handoff, lease,
revocation, and file semantics do not. Transport selection belongs to a
separate implementation change.

### D12. Imported trees are ordinary mounts

Remote exported trees mount under `/mnt/peer/<remote-id>` by default. Effects
execute on the exporting host and are attributed to the exporting lineage.
Import is directional; reverse sharing requires an explicit export and mount.

### D13. Post-entry control is file operations

Input, interrupts, request answers, app actions, and observation are reads and
writes on the returned namespace, including `/proc/<pid>/ctl` and AgentFS
files.

## Risks

- Full-namespace authority is intentionally powerful; product copy and active
  lease state must make this plain.
- Lease duration balances mobile continuity against lingering authority.
- Interactive quality over real networks is unproven and must be measured
  before product rollout.
- Remote device revocation must be immediate and auditable.

## References

- OpenSpec capability: `remote-access-service`, folded into
  `remove-daemon-era-contracts`.
- Product change: `add-alan-anywhere-mvp`.
