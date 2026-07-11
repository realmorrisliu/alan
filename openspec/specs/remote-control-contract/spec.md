# remote-control-contract Specification

## Purpose
Defines remote-control contracts for direct and relay modes, node/client
identity, scopes, reconnect behavior, notification signals, and non-bypass
governance.

## Requirements
### Requirement: Remote control contracts live in OpenSpec
alan SHALL specify remote control topology, direct and relay modes, remote
security, node/client identity, scopes, reconnect snapshots, notification
signals, and non-bypass governance rules in OpenSpec.

#### Scenario: Remote control behavior changes
- **WHEN** a change modifies direct-mode daemon configuration, relay routing,
  app-server protocol extensions, node discovery, reconnect behavior,
  notification signaling, token lifecycle, or revocation
- **THEN** the OpenSpec delta updates this capability, `alan-anywhere`,
  `daemon-api-contract`, or another named remote-control owner
- **AND** remote control docs under `docs/maintainer/` remain planning/runbook
  surfaces rather than the contract source

### Requirement: Remote governance cannot bypass local policy
alan SHALL preserve local governance and workspace authorization boundaries
when sessions are controlled through direct remote clients, relay transports, or
mobile-style reconnect flows.

#### Scenario: Remote client resumes a yielded session
- **WHEN** a remote client submits approval, resume, interrupt, or follow-up
  input
- **THEN** the daemon applies the same session governance and authorization
  rules as a local client
- **AND** remote notification signals remain informational rather than policy
  bypasses

### Requirement: Remote control topology preserves node authority
alan SHALL keep runtime execution, governance, event ordering, and cursor replay
authoritative on the agent node in direct and relay remote-control modes.

Components:

- **Agent Node**: runs daemon, runtime, scheduler, durable stores, governance,
  and tool execution.
- **Remote Client**: mobile, web, desktop, or other UI that sends control ops
  and consumes events.
- **Relay**: optional NAT/mobile routing layer that never becomes execution
  source of truth.

Responsibility matrix:

| Capability | Agent Node | Relay | Remote Client |
| --- | --- | --- | --- |
| Runtime execution | Authoritative | Not allowed | Not allowed |
| Governance and execution-backend enforcement | Authoritative | Not allowed | Not allowed |
| Event ordering (`event_id`) | Authoritative | Transport only | Consumer |
| Cursor replay decisions | Authoritative | Pass-through | Initiates requests |
| Session control ops | Validates and applies | Routes | Initiates |
| Auth scope re-validation | Required | Coarse pre-check | Provides credentials |

Planes:

1. control plane: session/thread lifecycle, submit/resume/interrupt, approvals
2. data plane: event stream, cursor replay, snapshots
3. management plane: node registration, tunnel health, token rotation,
   revocation

Core invariants:

1. execution always stays on the agent node
2. relay never bypasses governance or execution-backend checks
3. event replay uses stable cursors and deterministic gap handling
4. remote resume/approval uses the same `Yield`/`Resume` contract as local
   clients

#### Scenario: Remote client submits a control op
- **WHEN** a remote client submits turn, input, interrupt, resume, rollback, or
  delete intent
- **THEN** the agent node validates and applies the operation
- **AND** relay remains a transport/router only

### Requirement: Direct and relay transports expose explicit MVP surfaces
alan SHALL document direct and relay remote-control paths and their current MVP
limitations.

Direct mode:

1. Client connects directly to `alan-agentd` over TLS or an equivalent local
   trusted channel.
2. Client uses `/sessions/*` compatibility routes or future thread/turn aliases.
3. Reconnect uses `/events/read?after_event_id=...` to fill gaps.

Relay mode:

1. Agent node opens an outbound persistent tunnel to relay.
2. Client connects to relay endpoint with a scoped token.
3. Relay routes opaque protocol frames between client and node.
4. Event ordering remains node-authored.

Implemented relay MVP surface:

1. `GET /api/v1/relay/tunnel` WebSocket for node tunnel
2. `GET /api/v1/relay/nodes` for node status
3. `ANY /api/v1/relay/nodes/{node_id}/{*path}` for proxied requests
4. proxied target paths constrained to `/api/v1/*`
5. proxied target paths explicitly exclude `/api/v1/relay/*`

Current MVP limitations:

1. long-lived `/events` streaming is not proxied through relay
2. session WebSocket `/ws` upgrade paths are not proxied through relay
3. clients should use `/events/read` cursor polling for reconnect-safe remote
   consumption

Relay operational rules:

- Relay accepts node tunnel auth via `x-alan-node-id` plus bearer token when
  strict token mode is configured.
- Node maintains heartbeat and reconnect loop over the tunnel.
- Relay forwards control/data HTTP requests through the node tunnel without
  becoming execution authority.
- For proxied `create_session` and `fork_session`, relay rewrites returned
  session URLs with `/api/v1/relay/nodes/{node_id}` prefix.

#### Scenario: Relay client requests event stream
- **WHEN** a relay client requests long-lived `/events` or `/ws` during MVP
- **THEN** relay rejects the path and clients use `/events/read` polling or
  reconnect snapshots instead

### Requirement: Relay node discovery and sticky binding are deterministic
alan SHALL expose node routing signals and prevent silent cross-node
misrouting.

`GET /api/v1/relay/nodes` returns:

1. `node_id`
2. `connection_id`
3. `connected_at_ms`
4. `last_heartbeat_ms`
5. `heartbeat_age_ms`
6. `health`
7. `selectable`
8. `pending_requests`
9. `bound_sessions`
10. `last_binding_update_ms`

Relay proxied responses include `x-alan-routed-node-id`.

Sticky session-to-node rules:

1. First successful session operation establishes `session_id -> node_id`.
2. Subsequent requests for the same session to another node fail with HTTP
   `409` and code `relay_session_node_conflict`.
3. Client may explicitly request switch with `x-alan-node-switch: force`.
4. Switch takes effect only after a successful response from the target node.
5. Successful `DELETE /api/v1/sessions/{id}` removes sticky binding.

#### Scenario: Client targets wrong node for bound session
- **WHEN** a relay request targets a session bound to a different node
- **THEN** relay rejects it with `relay_session_node_conflict`
- **AND** it does not silently reroute unless the client explicitly requests a
  forced switch and the target node succeeds

### Requirement: Reconnect snapshots preserve remote continuity without re-execution
alan SHALL provide side-effect-free reconnect snapshots for direct and relay
clients.

Routes:

1. direct: `GET /api/v1/sessions/{id}/reconnect_snapshot`
2. relay: `GET /api/v1/relay/nodes/{node_id}/api/v1/sessions/{id}/reconnect_snapshot`

Snapshot requirements:

1. include `latest_event_id`, `oldest_event_id`, buffered count, and
   `latest_submission_id` dedupe hints
2. include actionable execution state such as `run_status`, `next_action`,
   `resume_required`, and pending yield checkpoint details
3. include notification signal state when available
4. be side-effect free
5. never re-drive runtime execution

`resume_required=true` means only explicit resume can advance execution.
Notification signals may be sparse and clients must tolerate an empty list.

#### Scenario: Client reconnects after cursor gap
- **WHEN** cursor replay reports a gap or the client has no reliable cursor
- **THEN** the client reads reconnect snapshot and resumes consumption from the
  node-authored latest event state
- **AND** the snapshot read does not advance execution

### Requirement: Remote notification signals are informational
alan SHALL treat remote reliability notifications as dedupe-friendly signals,
not as implicit authorization or state advancement.

Initial signal types:

1. `pending_yield`
2. `pending_structured_input`
3. `resume_failed`
4. `gap_detected`

Signal constraints:

1. stable `signal_id` for dedupe
2. `informational=true` in transport payload
3. recoverable through reconnect snapshots
4. loss of signal delivery does not lose the underlying runtime state

Non-bypass rules:

1. Signals never authorize execution changes by themselves.
2. Approval or resume still requires explicit node-authority operations.
3. Token scopes and policy checks remain unchanged.
4. Relay and clients cannot convert notification delivery into implicit resume.

#### Scenario: Client receives pending-yield notification
- **WHEN** a remote client receives a pending-yield notification signal
- **THEN** the signal may prompt UI attention
- **AND** execution advances only after an authorized resume operation reaches
  the agent node

### Requirement: Remote reconnect and multi-client consistency use node-authored cursors
alan SHALL keep reconnect and multi-client consistency based on stable event
cursors, snapshots, and explicit conflict responses.

Recommended handshake:

1. Client authenticates and selects `node_id`.
2. Client binds to or creates `session_id`.
3. Client provides latest known `event_id` cursor.
4. Node or relay replies with accepted cursor status, turn/run status, and
   replay-window metadata.

Reconnect rules:

1. If cursor is valid, replay from `after_event_id`.
2. If cursor is evicted, return `gap=true`, fetch snapshot, then continue.
3. Reconnect never triggers implicit turn re-execution.
4. If compatibility metadata reports `active=false`, clients treat the session
   as retained but inactive and fall back to snapshot/read or explicit resume.
5. TTL cleanup may transition an idle session from live to inactive without
   changing `session_id`; explicit delete remains destructive removal.

Multi-client rules:

- All clients observe the same node-authored event stream for a session.
- Last-writer semantics for control ops are explicit through submission and
  turn ids.
- Conflict responses are machine-readable, such as `state_conflict` and
  `stale_turn_id`.
- UIs remain eventually consistent through snapshot plus replay.

#### Scenario: Inactive retained session is opened remotely
- **WHEN** a remote client opens a session whose metadata reports
  `active=false`
- **THEN** the client treats it as retained persisted state rather than a live
  stream and uses read/snapshot/resume paths explicitly

### Requirement: Remote metadata extends protocol without changing runtime semantics
alan SHALL treat remote routing metadata as additive diagnostics and routing
context, not as runtime semantic changes.

Recommended additive fields:

1. `node_id`
2. `client_id`
3. `connection_id`
4. `trace_id`
5. `transport_mode`
6. `node_switch_mode`

Rules:

- Metadata fields do not change turn, input, resume, interrupt, or event
  semantics.
- Existing `/sessions/*` endpoints may accept optional metadata headers first.
- Relay conflict/switch signals are machine-readable.
- Future canonical APIs may map to thread/turn surface without breaking flow.

#### Scenario: Relay adds routed-node metadata
- **WHEN** relay forwards a response from a node
- **THEN** metadata such as `x-alan-routed-node-id` helps clients diagnose
  routing without changing runtime state

### Requirement: Remote auth scopes and daemon configuration are explicit
alan SHALL enforce remote bearer scopes in direct mode when enabled and SHALL
keep node-side authorization final.

Minimum auth scope classes:

1. `session.read`
2. `session.write`
3. `session.resume`
4. `session.admin`
5. `node.manage`

Rules:

- `session.resume` is required for remote approval actions.
- Relay enforces coarse routing scopes.
- Node revalidates all authorization.
- Node-side authorization is final source of truth.
- `/submit` and `/ws` route-level precheck may accept any mutating scope
  (`session.write`, `session.resume`, or `session.admin`), then exact operation
  scope is enforced on each submitted `Op`.

Direct-mode daemon config:

1. `ALAN_REMOTE_AUTH_ENABLED` truthy values enable bearer-scope checks.
2. `ALAN_REMOTE_AUTH_TOKENS` is semicolon-delimited `token=scopes`; scope list
   is comma-delimited and `*` grants all scopes.

Accepted remote metadata headers:

1. `x-alan-node-id`
2. `x-alan-client-id`
3. `x-alan-trace-id`
4. `x-alan-transport-mode`
5. `x-alan-node-switch`

#### Scenario: Remote submit lacks required scope
- **WHEN** a remote client submits an operation without the required exact
  session scope
- **THEN** alan rejects the operation even if the transport route precheck
  allowed the connection

### Requirement: Relay credentials and runtime configuration are scoped and revocable
alan SHALL keep relay routing credentials separate from execution authority and
make relay/node configuration explicit.

Relay server config:

1. `ALAN_RELAY_SERVER_ENABLED` truthy values enable relay tunnel/proxy routes.
2. `ALAN_RELAY_NODE_TOKENS` optionally maps `node_id=token` for tunnel auth.
3. When configured, tunnel connect requires both `x-alan-node-id` and matching
   bearer token.
4. Relay MVP proxy rejects long-lived `/events` streaming paths.
5. Relay MVP proxy rejects `/ws` upgrade paths.

Agent node outbound tunnel config:

1. `ALAN_RELAY_URL`
2. `ALAN_RELAY_NODE_ID`
3. `ALAN_RELAY_NODE_TOKEN`
4. `ALAN_RELAY_LOCAL_BASE_URL`

Token lifecycle:

1. short-lived access tokens plus refresh or rotation support
2. server-side revocation list for compromised tokens
3. node credential rotation with overlap window
4. explicit auth failure codes for denied or revoked tokens

Recommended revocation flow:

1. mark token or cert revoked
2. propagate revocation cache to relay and node
3. terminate active connections bound to revoked credential
4. require re-auth for resumed control sessions

#### Scenario: Node token is revoked
- **WHEN** a relay node credential is revoked
- **THEN** active relay connections bound to that credential terminate and
  future tunnel attempts fail explicitly

### Requirement: Remote security preserves replay integrity and audit trails
alan SHALL prevent replay, routing, and notification surfaces from bypassing
policy and yield boundaries.

Trust boundaries:

1. Agent node boundary: trusted execution plus governance and host
   execution-backend enforcement.
2. Relay boundary: transport/router boundary, untrusted for execution
   semantics.
3. Client boundary: user device/app boundary with scoped credentials.

Replay and integrity rules:

1. Requests include nonce/timestamp windows where supported.
2. Event stream cursors are monotonic and session-bound.
3. Connection-level trace ids propagate for audit correlation.
4. Resume decisions are tied to `request_id` plus scoped principal.
5. Replay/recovery paths use the same authorization checks as live paths.

Relay security constraints:

1. Relay forwards protocol payloads without changing runtime authority.
2. Relay cannot manufacture terminal runtime state transitions.
3. Relay may rewrite node-local session URLs only to keep clients on relay API
   surface.
4. Relay stores minimal metadata needed for routing and diagnostics.
5. Relay enforces sticky session routing.
6. Cross-node requests without explicit switch are rejected.

Audit fields:

1. `node_id`
2. `client_id`
3. `session_id`
4. `request_id`
5. `submission_id`
6. `scope_check_result`
7. `policy_action`
8. `transport_mode`
9. `resolved_by`
10. `switch_mode`
11. `bound_node_id`
12. `requested_node_id`
13. `notification_signal_id`
14. `signal_type`

Threat mitigations:

- Stolen client token: short TTL, scoped claims, revocation, device binding.
- Relay compromise: node-side authz finality, payload integrity checks, audit
  trails.
- Replay on flaky links: nonce/timestamp checks and request-id idempotency.
- Approval bypass attempt: resume only through valid pending `request_id` plus
  required scope.

#### Scenario: Relay is compromised
- **WHEN** relay transport is untrusted or compromised
- **THEN** node-side authorization, runtime policy, scoped resume validation,
  and audit trails remain the final authority for execution changes

### Requirement: Local daemon defaults are channel-scoped
Direct-mode local daemon and client defaults SHALL be scoped by active install
channel so stable Alan and Alan Dev do not bind or connect to each other's
default daemon endpoint.

#### Scenario: Stable daemon defaults are resolved
- **WHEN** stable-channel daemon or client configuration is resolved without explicit overrides
- **THEN** the host config path is `~/.alan/host.toml`
- **AND** the stable default bind address remains `0.0.0.0:8090`
- **AND** the stable default daemon URL remains `http://127.0.0.1:8090`

#### Scenario: Dev daemon defaults are resolved
- **WHEN** dev-channel daemon or client configuration is resolved without explicit overrides
- **THEN** the host config path is `~/.alan-dev/host.toml`
- **AND** the dev default bind address is `127.0.0.1:8091`
- **AND** the dev default daemon URL is `http://127.0.0.1:8091`
- **AND** missing dev host config does not cause the dev client to connect to the stable daemon URL

#### Scenario: Channel endpoint override is used
- **WHEN** an operator explicitly supplies a daemon bind address or daemon URL override for a process
- **THEN** the override applies only to that launched process and active channel
- **AND** it does not mutate the other channel's host config
- **AND** documentation identifies overrides as advanced/debug controls rather than the primary dev-channel selection mechanism
