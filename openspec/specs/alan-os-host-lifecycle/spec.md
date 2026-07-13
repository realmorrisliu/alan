# alan-os-host-lifecycle Specification

## Purpose
Defines singleton Alan OS Host ownership, file-proven readiness, per-boot
identity, and test-only ephemeral hosting.

## Requirements

### Requirement: One system Host owns each channel
Alan SHALL run at most one Alan OS Host per user, device, and install channel.
The dedicated Host SHALL own Kernel and whole-system lifetime; renderer hosts
MUST attach and MUST NOT boot competing product instances.

#### Scenario: CLI and macOS use stable
- **WHEN** both stable clients start for the same user
- **THEN** both target the same stable Alan OS Host
- **AND** neither creates an app-private Kernel

### Requirement: Host readiness is file-proven
The Host SHALL accept product attachments only after the Standard Namespace,
required fixed services, and `/agent/root` are readable. Required failure SHALL
fail boot rather than expose a partial system as ready.

#### Scenario: Root Agent fails to start
- **WHEN** `/agent/root` cannot be read during boot
- **THEN** the Host reports boot failure and rejects normal attachments

### Requirement: Host restart creates a new boot identity
Every Host boot SHALL publish a fresh boot identity and create a new Process
table and Root Agent Process. It MUST NOT deserialize live Process state.

#### Scenario: Stored Process Reference predates restart
- **WHEN** a client presents a reference with the previous boot identity
- **THEN** Alan rejects it even if the PID has been reused

### Requirement: Ephemeral Host is test-only
An in-process or ephemeral Host SHALL require explicit development/test
selection and MUST NOT be a product fallback when the dedicated Host is absent.

#### Scenario: Product Host fails
- **WHEN** a normal client cannot start or attach the dedicated Host
- **THEN** it reports the failure instead of silently booting an embedded system
