## ADDED Requirements

### Requirement: Daemon-era remote control is frozen legacy
alan SHALL treat the daemon-era remote-control surface this capability
records — relay HTTP routes under `/api/v1/relay/*`, `/sessions/*`
compatibility routes, reconnect snapshots, notification signals, session auth
scopes, and remote metadata headers — as frozen legacy per ADR-0028 D11. It SHALL NOT
gain new requirements, routes, scopes, metadata fields, or clients. New remote
work SHALL enter through the `remote-access-service` capability (aP bootstrap,
Remote Entry Process handoff, leases, returned namespaces). The durable
invariants this contract pioneered — execution stays authoritative on the
user's host, and no transport or relay bypasses local governance — transfer to
`remote-access-service` and continue to bind the legacy surface until it is
deleted.

#### Scenario: A change proposes extending the legacy remote surface
- **WHEN** a change adds routes, scopes, notification types, or metadata to
  the daemon-era remote-control surface
- **THEN** the change is rejected or re-shaped onto `remote-access-service`
- **AND** only removals, security fixes, and deletion-path work land on the
  legacy surface

#### Scenario: The legacy surface is retired
- **WHEN** the last daemon-backed remote client is removed
- **THEN** the legacy routes and this capability are deleted rather than
  migrated
