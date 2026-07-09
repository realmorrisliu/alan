# runtime-core-contract Specification

## Purpose
Defines durable runtime-core contracts for sessions, turns, tape, rollout,
operations, emitted events, compaction, scheduling, rollback, fork, and recovery
semantics.

## Requirements
### Requirement: Runtime core contracts live in OpenSpec
alan SHALL keep durable runtime, kernel, execution, compaction, scheduler,
interaction-inbox, durable-run, and app-server protocol requirements in
OpenSpec rather than in `docs/spec/` contract pages.

#### Scenario: Runtime behavior changes
- **WHEN** a change modifies session, turn, tape, rollout, compaction,
  scheduling, rollback, fork, app-server protocol, or interaction input-mode
  behavior
- **THEN** the requirement is added to this capability, an existing runtime
  capability, or an active OpenSpec change
- **AND** no long-form replacement contract is authored under `docs/spec/`

#### Scenario: Legacy runtime contract is referenced
- **WHEN** active documentation still links to a legacy runtime contract under
  `docs/spec/`
- **THEN** that file is a short bridge to this capability, `daemon-api-contract`,
  `runtime-memory-surfaces`, `child-run-lifecycle`, or another named OpenSpec
  owner
- **AND** the bridge does not restate the full legacy contract

### Requirement: Runtime object boundaries remain explicit
alan SHALL preserve explicit boundaries among host configuration, resolved
agent definitions, workspaces, agent instances, sessions, turns, tape, rollout
records, operations, and emitted events.

#### Scenario: Runtime-owned object model is extended
- **WHEN** a new runtime object or state transition is introduced
- **THEN** the OpenSpec delta identifies which layer owns it
- **AND** the delta states how the object is observed by daemon clients or
  persisted in rollout/session state when applicable

#### Scenario: User input advances execution
- **WHEN** a client submits `turn`, `input`, `resume`, `interrupt`, `compact`,
  or `rollback` operations
- **THEN** the operation semantics are specified in OpenSpec before client
  behavior depends on them

### Requirement: Runtime durability and recovery stay auditable
alan SHALL specify durable run, scheduler, compaction, rollback, replay, and
recovery behavior with auditable state transitions and explicit degradation
semantics.

#### Scenario: Durable state is written or replayed
- **WHEN** runtime execution persists rollout records, checkpoints, scheduled
  wakeups, effect records, or recovery metadata
- **THEN** the OpenSpec requirement identifies the durability scope,
  idempotency expectation, and user-visible failure mode

#### Scenario: Compaction or recovery degrades
- **WHEN** compaction, memory flush, scheduler wake, replay, or recovery cannot
  complete normally
- **THEN** alan records the limitation and continues only through the
  degradation path specified by OpenSpec

### Requirement: App-server protocol objects remain stable
alan SHALL expose a stable protocol layer for multi-client frontends without
requiring clients to depend on runtime internals.

Core protocol objects:

- **Thread**: long-lived conversation container, mapped to the current session
  compatibility surface.
- **Turn**: one execution cycle inside a thread, with explicit states such as
  `running`, `yielded`, `completed`, `interrupted`, and `failed`.
- **Item**: an atomic record inside a turn, such as user input, queued input,
  assistant delta/final output, tool call/result, reasoning delta, yield
  request/resume, or compaction marker.

Protocol layers:

- Control plane: start, resume, fork, archive, rollback, compact, input,
  interrupt, governance responses, and optional scheduler controls.
- Data plane: event streaming, event read/replay, thread/session snapshot read,
  persisted history, and reconnect snapshot.

#### Scenario: Client depends on protocol state
- **WHEN** a TUI, native, web, IDE, relay, or harness client observes runtime
  state
- **THEN** it uses protocol/session objects, events, snapshots, and documented
  route semantics
- **AND** it does not rely on private runtime internals

### Requirement: Compatibility session APIs map to protocol operations
alan SHALL preserve the current `/api/v1/sessions/*` compatibility API while
mapping each route to explicit protocol semantics.

Current compatibility mapping:

- `POST /api/v1/sessions` -> `thread/start`
- `GET /api/v1/sessions/{id}` -> metadata-focused `thread/read`
- `GET /api/v1/sessions/{id}/read` -> `thread/read`
- `GET /api/v1/sessions/{id}/history` -> history-only snapshot read
- `POST /api/v1/sessions/{id}/submit` -> `turn/start`, `turn/input`,
  `turn/resume`, `turn/interrupt`, `thread/compact`, or `thread/rollback`
  depending on submitted op
- `GET /api/v1/sessions/{id}/events` -> `events/stream`
- `GET /api/v1/sessions/{id}/ws` -> `events/stream` over WebSocket
- `GET /api/v1/sessions/{id}/events/read` -> `events/read`
- `GET /api/v1/sessions/{id}/reconnect_snapshot` -> `reconnect_snapshot`
- `POST /api/v1/sessions/{id}/resume` -> compatibility `turn/resume`
- `POST /api/v1/sessions/{id}/rollback` -> `thread/rollback`
- `POST /api/v1/sessions/{id}/compact` -> `thread/compact`
- `POST /api/v1/sessions/{id}/schedule_at` -> scheduler extension
- `POST /api/v1/sessions/{id}/sleep_until` -> scheduler extension
- `DELETE /api/v1/sessions/{id}` -> compatibility hard delete

Compatibility notes:

- Legacy `turn/steer` maps to `turn/input{mode=steer}`.
- Mode-less `Op::Input` defaults to `mode=steer`.
- `POST /api/v1/sessions/{id}/compact` maps to
  `Op::CompactWithOptions`; clients may omit the body or send
  `{ "focus": "..." }`.
- Route path compatibility is owned with `daemon-api-contract`; runtime-core
  owns the operation semantics behind those paths.

#### Scenario: Submit route receives an operation
- **WHEN** `/api/v1/sessions/{id}/submit` receives `turn`, `input`, `resume`,
  `interrupt`, `compact`, or `rollback`
- **THEN** alan executes the matching protocol operation with the semantics in
  this capability
- **AND** clients can reason about the route without inspecting runtime
  implementation details

### Requirement: Input modes have first-class semantics
alan SHALL treat `steer`, `follow_up`, and `next_turn` as distinct input modes
with reproducible turn behavior.

Input structure:

```text
thread_id
input
mode = steer | follow_up | next_turn
expected_turn_id?
```

Semantics:

- `steer` injects guidance into the currently active turn.
- `follow_up` queues input to execute immediately after the current turn
  completes.
- `next_turn` stores context for the next user turn only.
- `turn/input{mode=steer}` applies only to an active turn.
- `turn/input{mode=follow_up|next_turn}` may queue outside active execution.
- `turn/resume` is valid only in yielded state.
- `turn/interrupt` ends with interrupted or failed terminal state.

#### Scenario: Steering input arrives during active execution
- **WHEN** a client submits `input` with `mode=steer` for an active turn
- **THEN** alan applies the guidance to the active turn rather than starting a
  new turn

#### Scenario: Follow-up input is queued
- **WHEN** a client submits `input` with `mode=follow_up`
- **THEN** alan queues it to execute after the current turn completes
- **AND** the queued input is represented as protocol state rather than hidden
  client-local state

#### Scenario: Next-turn input is queued
- **WHEN** a client submits `input` with `mode=next_turn`
- **THEN** alan applies it only as context for the next turn

### Requirement: Events use cursor-based recovery
alan SHALL emit event envelopes with stable cursor metadata and provide a pull
recovery path for clients that miss streaming events.

Event envelope fields:

```text
event_id
sequence
session_id
submission_id
turn_id
item_id
timestamp_ms
event
```

Client recovery rules:

- Clients track the latest processed `event_id`.
- After reconnect, clients call `events/read` with `after_event_id`.
- If `gap=false`, clients apply returned events incrementally.
- If `gap=true`, clients rebuild state from `read`, `history`, or
  `reconnect_snapshot` as appropriate.
- Event streams and event reads surface structured compaction outcomes as
  `compaction_observed`.
- Event streams and event reads surface pre-compaction memory-flush outcomes as
  `memory_flush_observed`.

#### Scenario: Client reconnects after missing events
- **WHEN** a client reconnects with a last processed event id
- **THEN** alan supports compensating reads through `events/read`
- **AND** tells the client whether the replay buffer had a detectable gap

#### Scenario: Replay gap is reported
- **WHEN** `events/read` reports `gap=true`
- **THEN** the client rebuilds from authoritative session snapshots rather than
  assuming the event stream is complete

### Requirement: Session lifecycle distinguishes liveness from existence
alan SHALL distinguish live runtime attachment from persisted compatibility
session existence.

Rules:

- `GET /api/v1/sessions`, `GET /api/v1/sessions/{id}`, and
  `GET /api/v1/sessions/{id}/read` expose an `active` boolean describing
  runtime liveness.
- `active=true` means the daemon currently has a live runtime attached.
- `active=false` means the daemon retained the session binding, rollout path,
  and persisted history, but no live runtime is currently attached.
- TTL cleanup archives expired sessions in place by deactivating the runtime
  while preserving the compatibility record for later reads or resume.
- Explicit `DELETE /api/v1/sessions/{id}` is the destructive removal path and
  deletes the compatibility record entirely.

#### Scenario: Expired session is cleaned up
- **WHEN** TTL cleanup expires a live runtime
- **THEN** alan marks the compatibility session inactive while preserving
  persisted binding, rollout, and history metadata

#### Scenario: Session is explicitly deleted
- **WHEN** a client calls the delete route for a session
- **THEN** alan treats it as destructive removal of the compatibility record

### Requirement: Rollback and compaction expose durability limits
alan SHALL make rollback and compaction durability semantics explicit in
protocol responses and events.

Rules:

- Compatibility `thread/rollback` is non-durable unless a later durable-run
  contract changes that behavior.
- Rollback responses surface `durability.durable=false`, the rollback scope,
  and a warning that rollback is in-memory only when applicable.
- Rollback emits or persists a rollback event so clients can clear affected
  derived state.
- Compaction accepts optional focus text and produces structured compaction
  outcome events.
- Session reads and reconnect snapshots expose the latest compaction attempt so
  clients that missed realtime events can recover state.
- Memory-flush attempts that coordinate with compaction follow the same
  recoverability pattern.

#### Scenario: Client rolls back turns
- **WHEN** a client requests rollback through the compatibility route
- **THEN** alan reports whether the rollback was accepted and whether it is
  durable
- **AND** warns when the rollback will not survive runtime restart

#### Scenario: Client reconnects after compaction
- **WHEN** a client missed a compaction or memory-flush event
- **THEN** `read` or `reconnect_snapshot` exposes the latest attempt metadata
  for state restoration

### Requirement: Reconnect snapshots preserve mobile and TUI recovery state
alan SHALL expose reconnect snapshots that let clients recover execution,
history, pending-yield, and dedupe state without duplicating execution.

Reconnect snapshot requirements:

- Include enough session metadata and persisted history summary for the client
  to restore UI state.
- Include current execution state, pending-yield state when present, and latest
  submission id for reconnect dedupe hints.
- Include latest compaction and memory-flush attempt metadata when available.
- Prefer replay-buffer state for latest runtime attempts and fall back to
  durable rollout recovery when replay does not contain the latest event.
- Treat pending-yield notifications as informational transport signals; clients
  still resume through explicit `turn/resume` or `turn/input`.

#### Scenario: Mobile client reconnects
- **WHEN** a client requests `reconnect_snapshot`
- **THEN** alan returns enough state to restore the current session view and
  avoid duplicate submission
- **AND** follow-up actions still use explicit protocol operations

### Requirement: Errors, backpressure, and governance are protocol-visible
alan SHALL keep request-level errors, execution-level errors, overload
behavior, and governance checkpoints explicit in protocol surfaces.

Rules:

- Request-level errors such as invalid parameters, state conflicts, missing
  resources, and queue-capacity overflow return synchronously with
  machine-readable codes.
- Execution-level errors such as runtime, provider, or tool failures flow
  through events with turn id and context.
- Server transports use bounded queues and overload protection.
- Overload rejection returns explicit retryable errors.
- Approval and user-input checkpoints use unified Yield/Resume flow.
- Sensitive operations remain traceable to policy decisions.
- Protocol, replay, reconnect, and recovery paths do not bypass governance or
  workspace policy boundaries.

#### Scenario: Queue capacity is exceeded
- **WHEN** a client submits input beyond the protocol queue capacity
- **THEN** alan returns a recoverable request-level error with retry guidance

#### Scenario: Runtime tool request requires approval
- **WHEN** policy escalates a tool operation during a turn
- **THEN** alan emits a yield/checkpoint event and resumes only through an
  explicit resume operation

### Requirement: Remote and relay routing preserve protocol authority
alan SHALL allow remote-control metadata to extend protocol routing without
moving turn/run authority out of the target runtime node.

Additive remote metadata may include:

- `node_id`
- `client_id`
- `connection_id`
- `transport_mode = direct | relay`
- `trace_id`
- `node_switch_mode = force`

Relay safeguards:

- Sticky `session_id -> node_id` routing rejects accidental cross-node requests
  with machine-readable conflict code `relay_session_node_conflict` and HTTP
  409.
- Explicit session switch is opt-in through `x-alan-node-switch: force`.
- Relay exposes resolved routing decision through `x-alan-routed-node-id` in
  proxied responses.
- Direct/relay remote-control MVP supports node listing, tunneling, and
  ordinary HTTP path proxying.
- Relay mode intentionally rejects `/events` and `/ws` until streaming and
  WebSocket proxy behavior is explicitly specified and implemented.
- Core turn/run semantics remain node-authoritative.

#### Scenario: Relay routes a session request to the wrong node
- **WHEN** a proxied request targets a session bound to a different node
- **THEN** alan rejects the request with `relay_session_node_conflict`
- **AND** does not silently execute the operation on the wrong node

#### Scenario: Relay request targets event streaming
- **WHEN** relay mode receives `/events` or `/ws` during the MVP phase
- **THEN** alan rejects the request explicitly rather than pretending streaming
  proxy support exists

### Requirement: App-server protocol changes remain backward-compatible
alan SHALL evolve the app-server protocol through additive changes whenever
possible and gate breaking changes through versioned migration windows.

Rules:

- New fields are additive by default.
- Breaking changes require versioned migration plans and OpenSpec updates.
- Schema/type generation checks must cover event and selected daemon payload
  drift.
- alan prefers extending input `mode` over introducing frequent new methods
  when existing semantics can remain stable.

#### Scenario: Protocol payload shape changes
- **WHEN** a protocol event or selected daemon payload changes shape
- **THEN** generated/schema-checked client surfaces detect the drift or the
  OpenSpec change documents the compatibility window

### Requirement: Runtime confirmation resumes persist checkpoint records
alan SHALL persist a rollout checkpoint record when a runtime confirmation
control resume resolves a `tool_escalation` or
`effect_replay_confirmation` checkpoint.

#### Scenario: Tool escalation approval is persisted
- **WHEN** a runtime confirmation resume approves a `tool_escalation`
  checkpoint
- **THEN** rollout persistence includes a checkpoint record with the matching
  checkpoint id and checkpoint type
- **AND** recovery can distinguish the synthetic control payload from an
  ordinary user turn

#### Scenario: Effect replay rejection is persisted
- **WHEN** a runtime confirmation resume rejects an
  `effect_replay_confirmation` checkpoint
- **THEN** rollout persistence includes a checkpoint record with the matching
  checkpoint id and checkpoint type
- **AND** the persisted checkpoint records the resolved choice

### Requirement: Runtime confirmation checkpoints link to current tape roots when available
alan SHALL attach the current namespace `machine/tape` checkpoint root hash to a
persisted runtime confirmation checkpoint when the runtime can read that root.

#### Scenario: Current tape checkpoint root is available
- **WHEN** the runtime can read the current namespace `machine/tape` checkpoint
  root while persisting a runtime confirmation checkpoint
- **THEN** the rollout checkpoint record includes that root hash as
  `knowledge_root`

#### Scenario: Current tape checkpoint root is unavailable
- **WHEN** the runtime cannot read the current namespace `machine/tape`
  checkpoint root while persisting a runtime confirmation checkpoint
- **THEN** the runtime still persists the checkpoint record without
  `knowledge_root`
- **AND** the resume flow does not fail solely because the root read failed
