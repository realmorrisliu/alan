# Spec Delta

## ADDED Requirements

### Requirement: Connections expose typed decision evaluation through decide
A reachable Connection in `alan-llmfs` SHALL expose `connections/<conn>/decide`
using the clone-via-open ReadWrite lifecycle (`data` commit-on-clunk, `events`,
`ctl` abort, `status`) with an independent versioned `decision.v1` wire format
for `predicate`, `choice`, and `score` questions over verbatim untrusted
`state`. Unsupported Connections SHALL reject `decide` at dial time with
`ErrorCode::Unsupported` without falling back to generation, while keeping
`evaluate` and `clone` unchanged.

#### Scenario: A decision operation succeeds over decide
- **WHEN** a caller opens `connections/<conn>/decide` ReadWrite on a
  decision-capable Connection and commits a valid `decision.v1` request
  containing `predicate`, `choice`, and `score` questions on `data` clunk
- **THEN** `alan-llmfs` dispatches one `decide()` operation against the
  captured provider within its bounded `deadline_ms` (`1..=30_000`)
- **AND** `events` emits one terminal `decision.v1` record carrying typed
  `answers`, explicit `calibration`, captured `provider`/`model` provenance,
  and `usage` (`null` when unavailable, never invented zero)
- **AND** no assistant prose or Tool calls are emitted

#### Scenario: Unsupported Connection rejects decide at dial time
- **WHEN** a caller opens `connections/<conn>/decide` on a Connection whose
  provider returns `supports_decisions() == false`
- **THEN** `open` fails immediately with `ErrorCode::Unsupported`
- **AND** no operation directory is allocated and no generation fallback runs

#### Scenario: Malformed decision request fails at commit time
- **WHEN** a caller commits an unversioned document, unknown schema, unknown
  field, document exceeding 1 MiB, out-of-bounds `deadline_ms`, or invalid
  `score` levels to `data`
- **THEN** `data` clunk fails with `ErrorCode::BadRequest`, `status` becomes
  `rejected`, and no provider request is dispatched

#### Scenario: Decision times out or is aborted via ctl
- **WHEN** `deadline_ms` expires or a caller writes `abort` to `ctl` while
  waiting on the provider lock or awaiting `decide()`
- **THEN** the operation terminates with `decision_timeout` or `aborted`
  without later result publication or implicit retry

#### Scenario: Provider returns malformed decision evidence
- **WHEN** the provider returns `MalformedDecisionResponse` or an answer that
  fails `DecisionRequest::validate_response()`
- **THEN** `alan-llmfs` records a terminal error with
  `malformed_decision_response` or `invalid_decision_answer`
- **AND** no unvalidated answer, secret, or raw provider body is published

### Requirement: Decision metering and capabilities are independent of generation
`alan-llmfs` SHALL advertise decision support (`decisions.v1` and
`decisions.calibration`) in `providers/<provider>/capabilities` and
`connections/<conn>/capabilities` alongside `generation` and
`evaluation.choice_v1`. `ConnectionLimits` SHALL enforce an independent
`max_decisions: Option<u64>` quota metered as `decision_starts` in
`connections/<conn>/meter`, failing exhausted opens at dial time with
`ErrorCode::NoAccess`.

#### Scenario: Capabilities report decision support and calibration
- **WHEN** a caller reads `connections/<conn>/capabilities` for a registered
  Connection
- **THEN** the document reports `decisions.v1` and `decisions.calibration`
  matching the captured provider's `supports_decisions()` and
  `decision_calibration()`
- **AND** `generation` and `evaluation.choice_v1` fields remain preserved

#### Scenario: Decision budget is enforced independently
- **WHEN** a Connection is registered with `ConnectionLimits { max_decisions:
  Some(2), .. }` and two `decide` allocations have already opened
- **THEN** a third open of `connections/<conn>/decide` fails with
  `ErrorCode::NoAccess`
- **AND** `connections/<conn>/meter` reports `limits.max_decisions == 2` and
  `meter.decision_starts == 2` while `generation_starts` and
  `evaluation_starts` remain unaffected
