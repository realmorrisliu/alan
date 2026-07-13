# host-mount-service Specification

## Purpose
Defines Host Mount Service as the sole grant, projection, revocation, and audit
authority for Host-backed file trees.

## Requirements

### Requirement: Host Mount Service is the grant authority
Host Mount Service SHALL own request, user grant, hostfs export, namespace
projection, revocation, audit, and status. Alan OS-visible records SHALL expose
grant identity, label, access, provenance, and `/mnt` path but MUST NOT expose
the raw Host OS path.

#### Scenario: User approves a writable directory
- **WHEN** a Host adapter returns an approved hostfs export
- **THEN** Host Mount Service records the grant and mounts it into the requesting
  Process live namespace at the approved Alan OS path

### Requirement: Grant authority is capability-passed
Knowing a grant ID SHALL confer no access. A Process SHALL gain access only when
the grant handle or mount is explicitly projected into its namespace, and
native sandbox rights SHALL derive from that same grant.

#### Scenario: Child does not receive parent grant
- **WHEN** a parent launches a child without passing a Host Mount
- **THEN** the child cannot use the grant or its Host backing

### Requirement: Revocation invalidates projections
Revoking a Host Mount grant SHALL make every associated mount unavailable and
record the actor, time, and affected Process references.

#### Scenario: Active grant is revoked
- **WHEN** the user revokes it through a Host adapter
- **THEN** subsequent aP access fails
- **AND** no stale sandbox authorization remains valid for new Tool Processes
