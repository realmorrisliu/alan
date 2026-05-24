# daemon-api-contract Specification

## Purpose
Define alan daemon API route contract requirements: endpoint metadata, shared URL
construction, remote access scope, relay policy, generated client helpers,
payload drift checks, public route compatibility, and raw route-string
guardrails.
## Requirements
### Requirement: Canonical Endpoint Registry
The daemon SHALL define a canonical endpoint registry that lists every supported
HTTP and WebSocket endpoint with a stable endpoint id, HTTP method, route pattern,
path parameters, and API area.

#### Scenario: all registered daemon routes have endpoint metadata
- **WHEN** the daemon router is built
- **THEN** every public route registered by the daemon has a matching endpoint
  registry entry

#### Scenario: adding a route without metadata fails verification
- **WHEN** a developer adds a public daemon route without adding a registry entry
- **THEN** the route contract verification fails

### Requirement: Shared URL Construction
The daemon SHALL use endpoint contract helpers to construct session response URLs
and client-facing API paths instead of hand-written route strings.

#### Scenario: create-session response uses contract builders
- **WHEN** a session is created
- **THEN** the returned `websocket_url`, `events_url`, and `submit_url` values are
  built from the canonical endpoint contract

#### Scenario: fork-session response uses contract builders
- **WHEN** a session is forked
- **THEN** the returned child session URLs are built from the canonical endpoint
  contract

### Requirement: Remote Access Scope Metadata
The daemon SHALL derive remote-control authorization requirements from endpoint
contract metadata rather than independent path prefix or suffix rules.

#### Scenario: endpoint scope is resolved from metadata
- **WHEN** remote-control middleware evaluates a daemon request
- **THEN** it resolves the required `SessionScope` from the matched endpoint
  metadata

#### Scenario: unknown API path is not silently treated as a known endpoint
- **WHEN** remote-control middleware evaluates an unknown `/api/v1/...` path
- **THEN** it applies the configured unknown-path behavior explicitly and records
  that no endpoint metadata matched

### Requirement: Relay Policy Metadata
The daemon SHALL derive relay forwarding, streaming exclusion, WebSocket
exclusion, session binding extraction, and response URL rewriting from endpoint
contract metadata.

#### Scenario: relay forwarding allows only contract-approved endpoints
- **WHEN** a relay request attempts to forward a daemon API path
- **THEN** the relay layer allows forwarding only when the matched endpoint
  metadata permits relay forwarding

#### Scenario: relay URL rewriting uses response URL metadata
- **WHEN** a relayed session lifecycle response contains daemon-relative URL
  fields
- **THEN** the relay layer rewrites only the response fields identified by the
  endpoint contract

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

### Requirement: Public Route Compatibility
The first implementation SHALL preserve existing public daemon route paths unless
a task explicitly marks a route change as breaking and provides migration notes.

#### Scenario: contract migration keeps route paths stable
- **WHEN** the endpoint registry is introduced
- **THEN** existing public session, connection, skill, relay, WebSocket, and
  health route paths continue to resolve as before

### Requirement: Session route semantics match runtime protocol mapping
The daemon SHALL keep public session route paths aligned with the runtime
protocol semantics owned by `runtime-core-contract`.

Compatibility mapping:

- `POST /api/v1/sessions` creates a thread/session and returns session metadata,
  route URLs, provider binding metadata, execution backend, streaming mode,
  partial-stream recovery mode, and durability state.
- `GET /api/v1/sessions` lists compatibility sessions and reports runtime
  liveness through `active`.
- `GET /api/v1/sessions/{id}` returns metadata-focused compatibility state.
- `GET /api/v1/sessions/{id}/read` returns session metadata plus persisted
  message/history state needed by reconnecting clients.
- `GET /api/v1/sessions/{id}/history` returns persisted message history only.
- `POST /api/v1/sessions/{id}/submit` accepts protocol operations including
  `turn`, `input`, `resume`, `interrupt`, dynamic tool registration, client
  capabilities, compact-with-options, and rollback.
- `GET /api/v1/sessions/{id}/events` streams event envelopes as NDJSON.
- `GET /api/v1/sessions/{id}/ws` exposes the realtime WebSocket transport.
- `GET /api/v1/sessions/{id}/events/read` returns buffered events after a
  cursor and includes gap, oldest, and latest cursor metadata.
- `GET /api/v1/sessions/{id}/reconnect_snapshot` returns mobile/TUI recovery
  state, current execution state, pending-yield context when present, and
  dedupe hints.
- `POST /api/v1/sessions/{id}/resume` is a compatibility resume route.
- `POST /api/v1/sessions/{id}/fork` creates a fork from latest rollout state.
- `POST /api/v1/sessions/{id}/rollback` exposes in-memory rollback and returns
  durability warning metadata.
- `POST /api/v1/sessions/{id}/compact` triggers compaction with optional focus.
- `POST /api/v1/sessions/{id}/schedule_at` and
  `POST /api/v1/sessions/{id}/sleep_until` are scheduler extensions.
- `DELETE /api/v1/sessions/{id}` is the destructive compatibility removal
  path.

#### Scenario: Events read route is used after reconnect
- **WHEN** a client calls `events/read` with `after_event_id`
- **THEN** the daemon returns ordered event envelopes, cursor metadata, and
  `gap` state so the client can decide whether to replay incrementally or
  rebuild from a snapshot

#### Scenario: Rollback route is used
- **WHEN** a client calls the rollback route
- **THEN** the response includes accepted state and durability metadata
- **AND** in-memory-only rollback includes a warning that it will not survive
  runtime restart

#### Scenario: Archived compatibility session is read
- **WHEN** a session has no live runtime but retained rollout and history state
- **THEN** read/list routes return `active=false` rather than treating the
  session as missing

#### Scenario: Relay handles streaming routes during MVP
- **WHEN** the relay layer receives `/events` or `/ws` in the current MVP
- **THEN** it rejects those paths explicitly unless a later OpenSpec change
  adds streaming or WebSocket proxy support

### Requirement: Raw Route String Guardrail
The repository SHALL include focused guardrails that prevent new raw canonical
daemon route strings in production client, relay, remote-control, and daemon URL
construction code outside the approved contract or generated files.

#### Scenario: raw route string is added outside allowed files
- **WHEN** production code outside the approved contract or generated files adds
  a new raw `/api/v1/...` route string
- **THEN** the route string guardrail fails and points to the endpoint contract
  helper surface

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
