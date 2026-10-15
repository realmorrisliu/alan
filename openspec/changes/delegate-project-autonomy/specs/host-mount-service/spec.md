# Spec Delta

## ADDED Requirements

### Requirement: Saved project authorization is revalidated by its owning service
Host Mount Service SHALL own explicit saved file delegation and derive fresh live grants after validating Host identity, access and revocation state. Reuse MUST NOT rely solely on a path string or restore stale Process handles. Only a trusted-user authorization operation SHALL save, broaden or renew the delegation.

#### Scenario: Directory identity changed
- **WHEN** a saved directory was replaced or retargeted before reuse
- **THEN** the service refuses reuse and requests new explicit authorization

#### Scenario: Delegation revoked
- **WHEN** the user revokes a saved delegation while work is queued or a child is launching
- **THEN** new effects cannot use the revoked revision and existing projections follow live revocation semantics

#### Scenario: New invocation
- **WHEN** the user opens an explicitly selected previously delegated project
- **THEN** the service validates its saved scope before issuing invocation-local handles; no old task is automatically replayed
