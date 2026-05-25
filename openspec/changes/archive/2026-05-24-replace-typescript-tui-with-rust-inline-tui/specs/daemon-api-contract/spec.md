## MODIFIED Requirements

### Requirement: Generated Client Endpoint Helpers
The repository SHALL generate, expose, or verify client endpoint helpers from
the daemon endpoint contract for shipped daemon clients. The Rust TUI daemon
client SHALL use the shared Rust endpoint contract or generated Rust helpers for
API path construction instead of embedding raw canonical daemon route strings.
TypeScript helper generation MAY remain only for non-deleted TypeScript clients;
it MUST NOT be the authoritative TUI protocol surface.

#### Scenario: generated endpoint helper is current
- **WHEN** the daemon endpoint contract changes
- **THEN** generated endpoint helpers or contract snapshots for shipped daemon
  clients change deterministically or a drift check fails

#### Scenario: Rust TUI client avoids raw daemon route construction
- **WHEN** the Rust TUI daemon client calls a supported API endpoint
- **THEN** it constructs the path through the Rust endpoint contract/helper
  surface rather than embedding a raw `/api/v1/...` string

#### Scenario: TypeScript TUI helper is removed
- **WHEN** the TypeScript TUI is deleted
- **THEN** no production TUI build, test, or runtime path requires generated
  TypeScript endpoint helpers
- **AND** any remaining TypeScript helper checks are scoped to actual remaining
  TypeScript clients

### Requirement: Protocol And Payload Drift Checks
The repository SHALL keep protocol event types and selected daemon API payloads
schema-checked or generated for shipped daemon clients, including the Rust TUI.
The drift surface SHALL be based on alan's Rust protocol and daemon API
contracts, not on static hand-written TypeScript files for the removed TUI.

#### Scenario: protocol event list drifts
- **WHEN** the Rust protocol event enum adds, removes, or renames a serialized
  event type used by shipped daemon clients
- **THEN** the generated, schema-checked, or snapshot-checked client protocol
  surface detects the difference

#### Scenario: selected daemon payload drifts
- **WHEN** a selected daemon API response shape used by the Rust TUI changes in
  Rust
- **THEN** the Rust TUI client contract checks or generated payload surface
  detects the difference

#### Scenario: removed TypeScript protocol files are not authoritative
- **WHEN** `clients/tui` and its static generated TypeScript protocol files are
  removed
- **THEN** daemon API verification still covers shipped clients through Rust
  contract helpers, schema checks, or generated Rust surfaces

## ADDED Requirements

### Requirement: Rust TUI client preserves session API compatibility
The daemon API contract SHALL support the Rust TUI as a first-party daemon
client without changing existing public session, event, connection, skill,
relay, WebSocket, and health route paths unless a separate breaking route change
explicitly updates migration notes.

#### Scenario: TUI migration keeps route paths stable
- **WHEN** the TypeScript TUI is replaced by the Rust TUI
- **THEN** existing public daemon route paths used for session lifecycle,
  event streaming, submissions, reconnect snapshots, history reads, connection
  queries, and skill catalog reads continue to resolve

#### Scenario: Rust TUI submits protocol operations
- **WHEN** the Rust TUI sends turns, follow-up input, resume data, interrupts,
  rollback requests, or compaction requests
- **THEN** the daemon accepts the same protocol operation shapes exposed by the
  existing public session submit APIs
